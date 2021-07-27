from __future__ import annotations

import asyncio
import re
from itertools import groupby
from typing import List, Optional, TYPE_CHECKING, cast

import discord
from discord.ext import commands

import utils
from player import TTSVoicePlayer


if TYPE_CHECKING:
    from typing_extensions import TypedDict

    from main import TTSBotPremium
    from .cmds_settings import cmds_settings

    class _TRANSLATION(TypedDict):
        text: str
        detected_source_language: str

    class _DEEPL_RESP(TypedDict):
        translations: List[_TRANSLATION]
else:
    cmds_settings = None

async def do_autojoin(author: utils.TypedMember) -> bool:
    try:
        voice_channel = author.voice.channel # type: ignore
        permissions = voice_channel.permissions_for(author.guild.me)
        if not (permissions.view_channel and permissions.speak):
            return False

        return bool(await voice_channel.connect(cls=TTSVoicePlayer))
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
        if not getattr(self.bot, "patreon_role", False) or self.bot.patreon_role is None:
            return

        if str(message.author.id) not in self.bot.trusted:
            premium_user_for_guild = self.bot.patreon_json.get(str(message.guild.id))
            if premium_user_for_guild not in [member.id for member in self.bot.patreon_role.members]:
                return

        # Get settings
        repeated_limit, to_translate, target_lang, bot_ignore, max_length, autojoin, channel, is_formal, prefix, xsaid = await self.bot.settings.get(
            message.guild,
            settings=[
                "repeated_chars",
                "to_translate",
                "target_lang",
                "bot_ignore",
                "msg_length",
                "auto_join",
                "channel",
                "formal",
                "prefix",
                "xsaid",
            ]
        )

        message_clean = utils.removeprefix(
            message.clean_content.lower(), f"{prefix}tts"
        )

        # Check if a setup channel
        if message.channel.id != channel:
            return

        if message_clean.startswith(prefix):
            return

        bot_voice_client = message.guild.voice_client
        if message.author.bot:
            if bot_ignore or not bot_voice_client:
                return
        elif (
            not isinstance(message.author, discord.Member)
            or not message.author.voice
        ):
            return
        elif not bot_voice_client:
            if not autojoin:
                return

            if not await do_autojoin(message.author):
                return

        # Fix linter issues
        if TYPE_CHECKING:
            message.guild.voice_client = cast(TTSVoicePlayer, message.guild.voice_client)

        # Get voice and parse it into a useable format
        user_voice = None
        guild_voice = None
        lang, variant = cast(List[Optional[str]], await asyncio.gather(
            self.bot.userinfo.get("lang", message.author, default=None),
            self.bot.userinfo.get("variant", message.author, default="")
        ))

        if lang is not None:
            user_voice = " ".join((lang, variant)) # type: ignore
        else:
            guild_voice = (await self.bot.settings.get(message.guild, ["default_lang"]))[0]

        str_voice: str = user_voice or guild_voice or "en-us a"

        settings_cog = cast(cmds_settings, self.bot.get_cog("Settings"))
        voice = await settings_cog.get_voice(*str_voice.split())

        # Emoji filter
        message_clean = utils.EMOJI_REGEX.sub(utils.emoji_match_to_cleaned, message_clean)

        # Acronyms
        if lang == "en":
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
        if xsaid:
            said_name = await self.bot.nicknames.get(message.guild, message.author)
            file_format = utils.exts_to_format(message.attachments)

            if contained_url:

                if message_clean:
                    message_clean += " and sent a link."
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
        await message.guild.voice_client.queue(
            message_clean, voice.tuple, max_length
        )


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
