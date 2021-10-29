from __future__ import annotations

import asyncio
import re
from itertools import groupby
from typing import List, Optional, TYPE_CHECKING, cast

import discord
from discord.ext import commands

import utils
from player import TTSVoiceClient

if TYPE_CHECKING:
    from typing_extensions import TypedDict
    from main import TTSBotPremium

    class _TRANSLATION(TypedDict):
        text: str
        detected_source_language: str

    class _DEEPL_RESP(TypedDict):
        translations: List[_TRANSLATION]


async def do_autojoin(author: utils.TypedMember) -> bool:
    try:
        voice_channel = author.voice.channel # type: ignore
        permissions = voice_channel.permissions_for(author.guild.me)
        if not (permissions.view_channel and permissions.speak):
            return False

        return bool(await voice_channel.connect(cls=TTSVoiceClient)) # type: ignore
    except (asyncio.TimeoutError, AttributeError):
        return False

def setup(bot: TTSBotPremium):
    bot.add_cog(MainEvents(bot))

class MainEvents(utils.CommonCog):
    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedGuildMessage):
        if (not message.attachments and not message.content) or not message.guild:
            return

        # Premium Check
        if str(message.author.id) not in self.bot.trusted:
            premium_user: Optional[int] = self.bot.settings[message.guild.id]["premium_user"]
            if premium_user is None:
                return

            if self.bot.patreon_members is None:
                self.bot.patreon_members = await self.bot.fill_patreon_members()

            if premium_user not in self.bot.patreon_members:
                return

        # Get settings
        settings = await self.bot.settings.get(message.guild.id)
        userinfo = await self.bot.userinfo.get(message.author.id)

        prefix: str = settings["prefix"]
        channel: int = settings["channel"]
        autojoin: bool = settings["auto_join"]
        bot_ignore: bool = settings["bot_ignore"]
        default_lang: str = settings["default_lang"]
        to_translate: bool = settings["to_translate"]
        is_formal: Optional[bool] = settings["formal"]
        repeated_limit: int = settings["repeated_chars"]
        target_lang: Optional[str] = settings["target_lang"]

        # Check if a setup channel
        if message.channel.id != channel:
            return

        message_clean = message.clean_content.lower()
        message_clean = message_clean.removeprefix(f"{prefix}tts")

        if len(message_clean) >= 1500:
            return

        if message_clean.startswith(prefix):
            return

        if message.author.bot:
            if bot_ignore or not message.guild.voice_client:
                return
        elif (
            not isinstance(message.author, discord.Member)
            or not message.author.voice
        ):
            return
        elif not message.guild.voice_client:
            if not autojoin:
                return

            if not await do_autojoin(message.author):
                return

            if not message.guild.voice_client:
                return
        elif message.author.voice.channel != message.guild.voice_client.channel:
            return

        # Fix linter issues
        if TYPE_CHECKING:
            message.guild.voice_client = cast(TTSVoiceClient, message.guild.voice_client)

        # Get voice and parse it into a useable format
        user_voice: Optional[str] = None
        lang: Optional[str] = userinfo["lang"]
        variant: Optional[str] = userinfo["variant"]
        if lang is not None and variant is not None:
            user_voice = f"{lang} {variant}"

        str_voice: str = user_voice or default_lang or "en-us a"
        voice = await self.bot.get_voice(*str_voice.split())

        # Emoji filter
        message_clean = utils.EMOJI_REGEX.sub(utils.emoji_match_to_cleaned, message_clean)

        # Acronyms
        if voice.lang.startswith("en"):
            message_clean = f" {message_clean} "
            for toreplace, replacewith in utils.ACRONYMS.items():
                message_clean = message_clean.replace(
                    f" {toreplace} ", f" {replacewith} "
                )
            message_clean = message_clean[1:-1]

        if message_clean == "?":
            message_clean = "what"

        # Do Regex replacements
        for regex, replacewith in utils.REGEX_REPLACEMENTS.items():
            message_clean = re.sub(regex, replacewith, message_clean)

        # Url filter
        with_urls = " ".join(message_clean.split())
        link_starters = ("https://", "http://", "www.")
        message_clean = " ".join(w if not w.startswith(link_starters) else "" for w in with_urls.split())

        contained_url = message_clean != with_urls
        # Toggleable xsaid and attachment + links detection
        if settings["xsaid"]:
            nicknames = await self.bot.nicknames.get((message.guild.id, message.author.id))
            said_name: str = nicknames.get("name") or message.author.display_name

            file_format = utils.exts_to_format(message.attachments)

            if contained_url:
                if message_clean:
                    message_clean += " and sent a link."
                else:
                    message_clean = "a link."

            if message.attachments:
                if not message_clean:
                    message_clean = f"{said_name} sent {file_format}"
                else:
                    message_clean = f"{said_name} sent {file_format} and said {message_clean}"
            else:
                message_clean = f"{said_name} said: {message_clean}"

        elif contained_url:
            if message_clean:
                message_clean += ". This message contained a link"
            else:
                message_clean = "a link."

        if message_clean.strip(" ?.)'!\":") == "":
            return

        # Repeated chars removal if setting is not 0
        if message_clean.isprintable() and repeated_limit != 0:
            message_clean_list = []

            for char in ("".join(g) for _, g in groupby(message_clean)):
                if len(char) > repeated_limit:
                    message_clean_list.append(char[0] * repeated_limit)
                else:
                    message_clean_list.append(char)

            message_clean = "".join(message_clean_list)

        if len(message_clean) >= 1500:
            return

        # Premium Translation
        if to_translate and target_lang:
            body = {
                "text": message_clean,
                "preserve_formatting": 1,
                "target_lang": target_lang,
                "auth_key": self.bot.config["Translation"]["key"],
            }

            if is_formal is not None:
                body["formality"] = int(is_formal)

            resp_json: Optional[_DEEPL_RESP] = None
            url = f"{utils.TRANSLATION_URL}/translate"
            async with self.bot.session.get(url, params=body) as resp:
                if resp.status in {429, 529}:
                    self.bot.log("on_translate_ratelimit")
                    await self.bot.channels["logs"].send(
                        f"Hit ratelimit on deepL. {await resp.read()}"
                    )
                elif resp.status == 418:
                    self.bot.log("on_translate_too_long")
                elif resp.ok:
                    resp_json = await resp.json()
                else:
                    return resp.raise_for_status()

            if resp_json:
                translation = resp_json["translations"][0]
                detected_lang = translation.get("detected_source_language")
                if detected_lang.lower() != voice.lang:
                    message_clean = translation["text"]

        # Adds filtered message to queue
        await message.guild.voice_client.queue(message_clean, voice.tuple, settings["msg_length"])


    @commands.Cog.listener()
    async def on_voice_state_update(
        self,
        member: utils.TypedMember,
        before: discord.VoiceState,
        after: discord.VoiceState
    ):
        vc = member.guild.voice_client

        if (
            not vc                         # ignore if bot isn't in the vc
            or not before.channel          # ignore vc joins
            or member == self.bot.user     # ignore bot leaving vc
            or after.channel == vc.channel # ignore no change in voice channel
            or any(not member.bot for member in vc.channel.members) # ignore if bot isn't lonely
        ):
            return

        await vc.disconnect(force=True)

    @commands.Cog.listener()
    async def on_member_update(self,
        before: utils.TypedMember,
        after: utils.TypedMember
    ):
        if self.bot.get_support_server() != after.guild:
            return

        patreon_role = self.bot.get_patreon_role()
        if self.bot.patreon_members is None:
            self.bot.patreon_members = await self.bot.fill_patreon_members()

        if patreon_role in before.roles and patreon_role not in after.roles:
            self.bot.patreon_members.remove(before.id)
        elif patreon_role not in before.roles and patreon_role in after.roles:
            self.bot.patreon_members.append(after.id)
