from __future__ import annotations

import asyncio
import re
from inspect import cleandoc
from itertools import groupby
from random import choice as pick_random
from typing import TYPE_CHECKING, cast

import discord
from discord.ext import commands

import utils
from player import TTSVoicePlayer


if TYPE_CHECKING:
    from main import TTSBot


DM_WELCOME_MESSAGE = cleandoc("""
    **All messages after this will be sent to a private channel where we can assist you.**
    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
    There are some basic rules if you want to get help though:
    `1.` Ask your question, don't just ask for help
    `2.` Don't spam, troll, or send random stuff (including server invites)
    `3.` Many questions are answered in `-help`, try that first (also the default prefix is `-`)
""")

async def do_autojoin(author: utils.TypedMember) -> bool:
    try:
        voice_channel = author.voice.channel # type: ignore
        permissions = voice_channel.permissions_for(author.guild.me)
        if not (permissions.view_channel and permissions.speak):
            return False

        return bool(await voice_channel.connect(cls=TTSVoicePlayer))
    except (asyncio.TimeoutError, AttributeError):
        return False


def setup(bot: TTSBot):
    bot.add_cog(events_main(bot))

class events_main(utils.CommonCog):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.dm_pins = {}
        self.bot.blocked = False


    def is_welcome_message(self, message: discord.Message) -> bool:
        if not message.embeds:
            return False

        return message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!"


    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedMessage):
        if not message.attachments and not message.content:
            return

        if message.guild is not None:
            # Get settings
            repeated_limit, bot_ignore, max_length, autojoin, channel, prefix, xsaid = await self.bot.settings.get(
                message.guild,
                settings=[
                    "repeated_chars",
                    "bot_ignore",
                    "msg_length",
                    "auto_join",
                    "channel",
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
                message.author = cast(utils.TypedMember, message.author)

                message.guild.voice_client = cast(
                    TTSVoicePlayer,
                    message.guild.voice_client
                )
                message.author.voice = cast(
                    utils.TypedVoiceState,
                    message.author.voice
                )
                message.author.voice.channel = cast(
                    utils.VoiceChannel,
                    message.author.voice.channel
                )

            # Get lang
            guild_lang = None
            user_lang: str = await self.bot.userinfo.get( # type: ignore
                "lang", message.author, default=None
            )
            if not user_lang:
                guild_lang = cast(str, (await self.bot.settings.get(
                    message.guild, ["default_lang"]
                ))[0])

            lang = user_lang or guild_lang or "en"

            # Emoji filter
            message_clean = utils.emojitoword(message_clean)

            # Acronyms
            message_clean = f" {message_clean} "
            acronyms = {
                "iirc": "if I recall correctly",
                "afaik": "as far as I know",
                "wdym": "what do you mean",
                "imo": "in my opinion",
                "brb": "be right back",
                "irl": "in real life",
                "jk": "just kidding",
                "btw": "by the way",
                ":)": "smiley face",
                "gtg": "got to go",
                "rn": "right now",
                ":(": "sad face",
                "ig": "i guess",
                "rly": "really",
                "cya": "see ya",
                "ik": "i know",
                "uwu": "oowoo",
                "@": "at",
                "™️": "tm"
            }

            for toreplace, replacewith in acronyms.items():
                message_clean = message_clean.replace(f" {toreplace} ", f" {replacewith} ")

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

            # Adds filtered message to queue
            await message.guild.voice_client.queue(
                message, message_clean, lang, channel, prefix, max_length
            )


        elif not (message.author.bot or message.content.startswith("-")):
            pins = self.dm_pins.get(message.author.id, None)
            if not pins:
                self.dm_pins[message.author.id] = pins = await message.author.pins()

            if any(map(self.is_welcome_message, pins)):
                if "https://discord.gg/" in message.content.lower():
                    await message.author.send(f"Join https://discord.gg/zWPWwQC and look in <#694127922801410119> to invite {self.bot.user.mention}!")

                elif message.content.lower() == "help":
                    await asyncio.gather(
                        self.bot.channels["logs"].send(f"{message.author} just got the 'dont ask to ask' message"),
                        message.channel.send("We cannot help you unless you ask a question, if you want the help command just do `-help`!")
                    )

                elif not await self.bot.userinfo.get("blocked", message.author, default=False):
                    files = [await attachment.to_file() for attachment in message.attachments]
                    await self.bot.channels["dm_logs"].send(
                        message.content,
                        files=files,
                        username=str(message.author),
                        avatar_url=message.author.avatar_url
                    )

            else:
                if len(pins) >= 49:
                    return await message.channel.send("Error: Pinned messages are full, cannot pin the Welcome to Support DMs message!")

                embed = discord.Embed(
                    title=f"Welcome to {self.bot.user.name} Support DMs!",
                    description=DM_WELCOME_MESSAGE
                ).set_footer(text=pick_random(utils.FOOTER_MSGS))

                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await asyncio.gather(
                    self.bot.channels["logs"].send(f"{message.author} just got the 'Welcome to Support DMs' message"),
                    dm_message.pin()
                )

    @commands.Cog.listener()
    async def on_voice_state_update(
        self,
        member: utils.TypedMember,
        before: discord.VoiceState,
        after: discord.VoiceState
    ):
        vc = member.guild.voice_client

        if member == self.bot.user:
            return  # ignore bot leaving vc
        if not before.channel or after.channel:
            return  # ignore everything but vc leaves
        if not vc:
            return  # ignore if bot isn't in the vc

        if any(not member.bot for member in vc.channel.members):
            return  # ignore if bot isn't lonely

        await vc.disconnect(force=True)

    @commands.Cog.listener()
    async def on_private_channel_pins_update(self, channel: discord.DMChannel, _):
        self.dm_pins.pop(channel.recipient.id, None)
