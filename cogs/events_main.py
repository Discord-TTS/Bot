from __future__ import annotations

import asyncio
import re
from itertools import groupby
from typing import TYPE_CHECKING, cast

import discord
from discord.ext import commands

import utils
from player import TTSVoicePlayer

if TYPE_CHECKING:
    from main import TTSBot


async def do_autojoin(author: utils.TypedMember) -> bool:
    try:
        voice_channel = author.voice.channel # type: ignore
        permissions = voice_channel.permissions_for(author.guild.me)
        if not (permissions.view_channel and permissions.speak):
            return False

        return bool(await voice_channel.connect(cls=TTSVoicePlayer)) # type: ignore
    except (asyncio.TimeoutError, AttributeError):
        return False


def setup(bot: TTSBot):
    bot.add_cog(MainEvents(bot))

class MainEvents(utils.CommonCog):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.bot.blocked = False


    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedGuildMessage):
        if not message.attachments and not message.content:
            return

        if message.guild is None:
            return

        # Get settings
        settings = await self.bot.settings.get(message.guild.id)
        userinfo = await self.bot.userinfo.get(message.author.id)

        prefix: str = settings["prefix"]
        channel: int = settings["channel"]
        autojoin: bool = settings["auto_join"]
        bot_ignore: bool = settings["bot_ignore"]
        default_lang: str = settings["default_lang"]
        repeated_limit: int = settings["repeated_chars"]

        lang: str = userinfo.get("lang", None) or default_lang or "en"

        # Check if a setup channel
        if message.channel.id != channel:
            return

        message_clean = message.clean_content.lower()
        message_clean = utils.removeprefix(message_clean, f"{prefix}tts")

        if len(message_clean) >= 1500:
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
        elif message.author.voice.channel != bot_voice_client.channel:
            return

        # Fix linter issues
        if TYPE_CHECKING:
            message.guild.voice_client = cast(TTSVoicePlayer, message.guild.voice_client)


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

        # Adds filtered message to queue
        await message.guild.voice_client.queue(
            message_clean, lang, channel, prefix, settings["msg_length"]
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
