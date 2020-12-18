import asyncio
import re
from functools import partial as make_func
from inspect import cleandoc
from io import BytesIO
from itertools import groupby
from random import choice as pick_random

import discord
import gtts as gTTS
from discord.ext import commands
from mutagen.mp3 import MP3, HeaderNotFoundError

from utils import basic
from patched_FFmpegPCM import FFmpegPCMAudio


def setup(bot):
    bot.add_cog(Main(bot))


class Main(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    async def get_tts(self, message, text, lang, max_length):
        mp3 = await self.bot.cache.get(text, lang, message.id)
        if not mp3:
            make_tts_func = make_func(self.make_tts, text, lang)
            temp_store_for_mp3 = await self.bot.loop.run_in_executor(None, make_tts_func)

            try:
                temp_store_for_mp3.seek(0)
                file_length = int(MP3(temp_store_for_mp3).info.length)
            except HeaderNotFoundError:
                return

            # Discard if over max length seconds
            if file_length > int(max_length):
                return

            temp_store_for_mp3.seek(0)
            mp3 = temp_store_for_mp3.read()

            await self.bot.cache.set(text, lang, message.id, mp3)

        self.bot.queue[message.guild.id][message.id] = mp3

    def make_tts(self, text, lang) -> BytesIO:
        temp_store_for_mp3 = BytesIO()
        in_vcs = len(self.bot.voice_clients)
        if in_vcs < 5:
            max_range = 50
        elif in_vcs < 20:
            max_range = 20
        else:
            max_range = 10

        for attempt in range(1, max_range):
            try:
                gTTS.gTTS(text=text, lang=lang).write_to_fp(temp_store_for_mp3)
                break
            except (ValueError, gtts.tts.gTTSError):
                if attempt == max_range:
                    raise

        return temp_store_for_mp3

    def finish_future(self, fut, *args):
        if not fut.done():
            self.bot.loop.call_soon_threadsafe(fut.set_result, "done")

    @commands.Cog.listener()
    async def on_message(self, message):
        if message.guild is not None:
            saythis = message.clean_content.lower()

            # Get settings
            autojoin, bot_ignore, channel = await self.bot.settings.get(
                message.guild,
                settings=(
                    "auto_join",
                    "bot_ignore",
                    "channel"
                )
            )

            starts_with_tts = saythis.startswith(f"{self.bot.command_prefix}tts")

            # if author is a bot and bot ignore is on
            if bot_ignore and message.author.bot:
                return

            # if not a webhook but still a user, return to fix errors
            if message.author.discriminator != "0000" and isinstance(message.author, discord.User):
                return

            # if author is not a bot, and is not in a voice channel, and doesn't start with -tts
            if not message.author.bot and message.author.voice is None and starts_with_tts is False:
                return

            # if bot **not** in voice channel and autojoin **is off**, return
            if message.guild.voice_client is None and autojoin is False:
                return

            # Check if a setup channel
            if message.channel.id != int(channel):
                return

            # If message is **not** empty **or** there is an attachment
            if saythis or message.attachments:

                # Ignore messages starting with -
                if saythis.startswith(self.bot.command_prefix) is False or starts_with_tts:

                    # This line :( | if autojoin is True **or** message starts with -tts **or** author in same voice channel as bot
                    if autojoin or starts_with_tts or message.author.bot or message.author.voice.channel == message.guild.voice_client.channel:

                        # Fix values
                        if message.guild.id not in self.bot.queue:
                            self.bot.queue[message.guild.id] = dict()
                        if message.guild.id not in self.bot.message_locks:
                            self.bot.message_locks[message.guild.id] = asyncio.Lock()

                        leaving = self.bot.should_return.get(message.guild.id)

                        # Auto Join
                        if message.guild.voice_client is None and autojoin and not leaving:
                            try:
                                channel = message.author.voice.channel
                            except AttributeError:
                                return

                            self.bot.should_return[message.guild.id] = True
                            await channel.connect()
                            self.bot.should_return[message.guild.id] = False

                        # Get settings
                        lang = await self.bot.setlangs.get(message.author)
                        xsaid, repeated_chars_limit, msg_length = await self.bot.settings.get(
                            message.guild,
                            settings=(
                                "xsaid",
                                "repeated_chars",
                                "msg_length"
                            )
                        )

                        # Emoji filter
                        saythis = basic.emojitoword(saythis)

                        # Acronyms and removing -tts
                        saythis = f" {saythis} "
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

                        if starts_with_tts:
                            acronyms["-tts"] = ""

                        for toreplace, replacewith in acronyms.items():
                            saythis = saythis.replace(f" {toreplace} ", f" {replacewith} ")

                        saythis = saythis[1:-1]
                        if saythis == "?":
                            saythis = "what"

                        # Regex replacements
                        regex_replacements = {
                            r"\|\|.*?\|\|": ". spoiler avoided.",
                            r"```.*?```": ". code block.",
                            r"`.*?`": ". code snippet.",
                        }

                        for regex, replacewith in regex_replacements.items():
                            saythis = re.sub(regex, replacewith, saythis, flags=re.DOTALL)

                        # Url filter
                        contained_url = False
                        for word in saythis.split(" "):
                            if word.startswith(("https://", "http://", "www.")):
                                saythis = saythis.replace(word, "")
                                contained_url = True

                        # Toggleable xsaid and attachment + links detection
                        if xsaid:
                            said_name = await self.bot.nicknames.get(message.guild, message.author)
                            format = basic.exts_to_format(message.attachments)

                            if contained_url:
                                if saythis:
                                    saythis += " and sent a link."
                                else:
                                    saythis = "a link."

                            if message.attachments:
                                if not saythis:
                                    saythis = f"{said_name} sent {format}"
                                else:
                                    saythis = f"{said_name} sent {format} and said {saythis}"
                            else:
                                saythis = f"{said_name} said: {saythis}"

                        elif contained_url:
                            if saythis:
                                saythis += ". This message contained a link"
                            else:
                                saythis = "a link."

                        if basic.remove_chars(saythis, " ", "?", ".", ")", "'", "!", '"') == "":
                            return

                        # Repeated chars removal if setting is not 0
                        repeated_chars_limit = int(repeated_chars_limit)
                        if saythis.isprintable() and repeated_chars_limit != 0:
                            saythis_chars = ["".join(grp) for num, grp in groupby(saythis)]
                            saythis_list = list()

                            for char in saythis_chars:
                                if len(char) > repeated_chars_limit:
                                    saythis_list.append(char[0] * repeated_chars_limit)
                                else:
                                    saythis_list.append(char)

                            saythis = "".join(saythis_list)

                        # Adds filtered message to queue
                        try:
                            await self.get_tts(message, saythis, lang, msg_length)
                        except ValueError:
                            return print(f"Run out of attempts generating {saythis}.")
                        except AssertionError:
                            return print(f"Skipped {saythis}, apparently blank message.")

                        async with self.bot.message_locks[message.guild.id]:
                            if self.bot.should_return[message.guild.id]:
                                return

                            while self.bot.queue.get(message.guild.id) not in (dict(), None):
                                # Sort Queue
                                self.bot.queue[message.guild.id] = basic.sort_dict(self.bot.queue[message.guild.id])

                                # Select first in queue
                                message_id_to_read = next(iter(self.bot.queue[message.guild.id]))
                                selected = self.bot.queue[message.guild.id][message_id_to_read]

                                # Play selected audio
                                vc = message.guild.voice_client
                                if vc is not None:
                                    self.bot.currently_playing[message.guild.id] = self.bot.loop.create_future()
                                    finish_future = make_func(self.finish_future, self.bot.currently_playing[message.guild.id])

                                    try:
                                        vc.play(FFmpegPCMAudio(selected, pipe=True, options='-loglevel "quiet"'), after=finish_future)
                                    except discord.errors.ClientException:
                                        self.bot.currently_playing[message.guild.id].set_result("done")

                                    try:
                                        result = await asyncio.wait_for(self.bot.currently_playing[message.guild.id], timeout=int(msg_length) + 1)
                                    except asyncio.TimeoutError:
                                        await self.bot.channels["errors"].send(f"```asyncio.TimeoutError``` Future Failed to be finished in guild: `{message.guild.id}`")
                                        result = "failed"

                                    if result == "skipped":
                                        self.bot.queue[message.guild.id] = dict()

                                    # Delete said message from queue
                                    elif message_id_to_read in self.bot.queue.get(message.guild.id, ()):
                                        del self.bot.queue[message.guild.id][message_id_to_read]

                                else:
                                    # If not in a voice channel anymore, clear the queue
                                    self.bot.queue[message.guild.id] = dict()

        elif message.author.bot is False:
            pins = await message.author.pins()

            if [True for pinned_message in pins if pinned_message.embeds and pinned_message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!"]:
                if "https://discord.gg/" in message.content.lower():
                    await message.author.send(f"Join https://discord.gg/zWPWwQC and look in <#694127922801410119> to invite {self.bot.user.mention}!")

                elif message.content.lower() == "help":
                    await message.channel.send("We cannot help you unless you ask a question, if you want the help command just do `-help`!")
                    await self.bot.channels["logs"].send(f"{message.author} just got the 'dont ask to ask' message")

                elif not await self.bot.blocked_users.check(message.author):
                    files = [await attachment.to_file() for attachment in message.attachments]
                    if not files and not message.content:
                        return

                    webhook = await basic.ensure_webhook(self.bot.channels["dm_logs"], name="TTS-DM-LOGS")
                    await webhook.send(message.content, username=str(message.author), avatar_url=message.author.avatar_url, files=files)

            else:
                if len(pins) >= 49:
                    return await message.channel.send("Error: Pinned messages are full, cannot pin the Welcome to Support DMs message!")

                embed_message = cleandoc("""
                    **All messages after this will be sent to a private channel where we can assist you.**
                    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
                    There are some basic rules if you want to get help though:
                    `1.` Ask your question, don't just ask for help
                    `2.` Don't spam, troll, or send random stuff (including server invites)
                    `3.` Many questions are answered in `-help`, try that first (also the prefix is `-`)
                """)

                embed = discord.Embed(title=f"Welcome to {self.bot.user.name} Support DMs!", description=embed_message)
                embed.set_footer(text=pick_random(basic.footer_messages))

                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await self.bot.channels["logs"].send(f"{message.author} just got the 'Welcome to Support DMs' message")
                await dm_message.pin()

    @commands.Cog.listener()
    async def on_voice_state_update(self, member, before, after):
        guild = member.guild
        vc = guild.voice_client
        no_speak = self.bot.should_return.get(guild.id)

        if member == self.bot.user:
            return  # someone other than bot left vc
        if not (before.channel and not after.channel):
            return  # user left voice channel
        if not vc:
            return  # bot in a voice channel

        if len([member for member in vc.channel.members if not member.bot]) != 0:
            return  # bot is only one left
        if no_speak:
            return  # bot not already joining/leaving a voice channel

        self.bot.should_return[guild.id] = True
        await vc.disconnect(force=True)
