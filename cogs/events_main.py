import asyncio
import re
from inspect import cleandoc
from itertools import groupby
from random import choice as pick_random

import discord
from discord.ext import commands

from player import TTSVoicePlayer
from utils import basic


DM_WELCOME_MESSAGE = cleandoc("""
    **All messages after this will be sent to a private channel where we can assist you.**
    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
    There are some basic rules if you want to get help though:
    `1.` Ask your question, don't just ask for help
    `2.` Don't spam, troll, or send random stuff (including server invites)
    `3.` Many questions are answered in `-help`, try that first (also the default prefix is `-`)
""")

def setup(bot):
    bot.add_cog(events_main(bot))


class events_main(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.dm_pins = dict()


    def is_welcome_message(self, message):
        if not message.embeds:
            return False

        return message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!"


    @commands.Cog.listener()
    async def on_message(self, message):
        if message.guild is not None:
            # Premium Check
            if not getattr(self.bot, "patreon_role", False):
                return

            if str(message.author.id) not in self.bot.trusted:
                premium_user_for_guild = self.bot.patreon_json.get(str(message.guild.id))
                if premium_user_for_guild not in [member.id for member in self.bot.patreon_role.members]:
                    return

            # Get settings
            repeated_chars_limit, bot_ignore, max_length, autojoin, channel, prefix, xsaid = await self.bot.settings.get(
                message.guild,
                settings=(
                    "repeated_chars",
                    "bot_ignore",
                    "msg_length",
                    "auto_join",
                    "channel",
                    "prefix",
                    "xsaid",
                )
            )

            message_clean = message.clean_content.lower()
            starts_with_tts = message_clean.startswith(f"{prefix}tts")

            # if author is a bot and bot ignore is on
            if bot_ignore and message.author.bot:
                return

            # if not a webhook but still a user, return to fix errors
            if message.author.discriminator != "0000" and isinstance(message.author, discord.User):
                return

            # if author is not a bot, and is not in a voice channel, and doesn't start with -tts
            if not message.author.bot and not message.author.voice and not starts_with_tts:
                return

            # if bot not in voice channel and autojoin is off
            if not message.guild.voice_client and not autojoin:
                return

            # Check if a setup channel
            if message.channel.id != channel:
                return

            # If message is empty and there is no attachments
            if not message_clean and not message.attachments:
                return

            # Ignore messages starting with -
            if message_clean.startswith(prefix) and not starts_with_tts:
                return

            # if not autojoin and message doesn't start with tts and the author isn't a bot and the author is in the wrong voice channel
            if not autojoin and not starts_with_tts and not message.author.bot and message.author.voice.channel != message.guild.voice_client.channel:
                return

            # Auto Join
            if not message.guild.voice_client and autojoin:
                try:
                    voice_channel = message.author.voice.channel
                except AttributeError:
                    return

                await voice_channel.connect(cls=TTSVoicePlayer)

            # Get voice and parse it into a useable format
            lang, variant = await asyncio.gather(
                self.bot.userinfo.get("lang", message.author),
                self.bot.userinfo.get("variant", message.author)
            )

            voice = (await self.bot.get_cog("Settings").get_voice(lang, variant)).tuple

            # Emoji filter
            message_clean = basic.emojitoword(message_clean)

            # Acronyms and removing -tts
            message_clean = f" {message_clean} "
            acronyms = {
                "iirc": "if I recall correctly",
                "asap": "as soon as possible",
                "afaik": "as far as I know",
                "hyd": "how are you doing",
                "wdym": "what do you mean",
                "imo": "in my opinion",
                "brb": "be right back",
                "idk": "i don't know",
                "irl": "in real life",
                "jk": "just kidding",
                "btw": "by the way",
                ":)": "smiley face",
                "np": "no problem",
                "etc": "et cetera",
                "gtg": "got to go",
                "rn": "right now",
                "ty": "thank you",
                ":(": "sad face",
                "ig": "i guess",
                "rly": "really",
                "cya": "see ya",
                "ik": "i know",
                "uwu": "oowoo",
                "@": "at",
                "™️": "tm"
            }

            if starts_with_tts:
                acronyms[f"{prefix}tts"] = ""

            for toreplace, replacewith in acronyms.items():
                message_clean = message_clean.replace(f" {toreplace} ", f" {replacewith} ")

            message_clean = message_clean[1:-1]
            if message_clean == "?":
                message_clean = "what"

            # Regex replacements
            regex_replacements = {
                r"\|\|.*?\|\|": ". spoiler avoided.",
                r"```.*?```": ". code block.",
                r"`.*?`": ". code snippet.",
            }

            for regex, replacewith in regex_replacements.items():
                message_clean = re.sub(regex, replacewith, message_clean, flags=re.DOTALL)

            # Url filter
            with_urls = message_clean
            link_starters = ("https://", "http://", "www.")
            message_clean = " ".join(w if not w.startswith(link_starters) else "" for w in with_urls.split())

            contained_url = message_clean != with_urls
            # Toggleable xsaid and attachment + links detection
            if xsaid:
                said_name = await self.bot.nicknames.get(message.guild, message.author)
                file_format = basic.exts_to_format(message.attachments)

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

            if basic.remove_chars(message_clean, " ", "?", ".", ")", "'", "!", '"', ":") == "":
                return

            # Repeated chars removal if setting is not 0
            if message_clean.isprintable() and repeated_chars_limit != 0:
                message_clean_list = list()
                message_clean_chars = ["".join(grp) for num, grp in groupby(message_clean)]

                for char in message_clean_chars:
                    if len(char) > repeated_chars_limit:
                        message_clean_list.append(char[0] * repeated_chars_limit)
                    else:
                        message_clean_list.append(char)

                message_clean = "".join(message_clean_list)

            # Adds filtered message to queue
            await message.guild.voice_client.queue(message, message_clean, voice, max_length)


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
                    if not message.attachments and not message.content:
                        return

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
                ).set_footer(text=pick_random(basic.footer_messages))

                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await asyncio.gather(
                    self.bot.channels["logs"].send(f"{message.author} just got the 'Welcome to Support DMs' message"),
                    dm_message.pin()
                )

    @commands.Cog.listener()
    async def on_voice_state_update(self, member, before, after):
        vc = member.guild.voice_client

        if member == self.bot.user:
            return  # ignore bot leaving vc
        if not (before.channel and not after.channel):
            return  # ignore everything but vc leaves
        if not vc:
            return  # ignore if bot isn't in the vc

        if len([member for member in vc.channel.members if not member.bot]):
            return  # ignore if bot isn't lonely

        await vc.disconnect(force=True)

    @commands.Cog.listener()
    async def on_private_channel_pins_update(self, channel, last_pin):
        self.dm_pins.pop(channel.recipient.id, None)
