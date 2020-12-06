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

def setup(bot):
    bot.add_cog(Main(bot))

class Main(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    async def get_tts(self, message, text, lang):
        cache_mp3 = await self.bot.cache.get(text, lang, message.id)
        if not cache_mp3:
            make_tts_func = make_func(self.make_tts, text, lang)
            temp_store_for_mp3 = await self.bot.loop.run_in_executor(None, make_tts_func)

            try:
                temp_store_for_mp3.seek(0)
                file_length = int(MP3(temp_store_for_mp3).info.length)
            except HeaderNotFoundError:
                return

            # Discard if over max length seconds
            max_length = await self.bot.settings.get(message.guild, "msg_length")
            if file_length < int(max_length):
                temp_store_for_mp3.seek(0)
                temp_store_for_mp3 = temp_store_for_mp3.read()

                self.bot.queue[message.guild.id][message.id] = temp_store_for_mp3
                await self.bot.cache.set(text, lang, message.id, temp_store_for_mp3)
        else:
            self.bot.queue[message.guild.id][message.id] = cache_mp3

    def make_tts(self, text, lang) -> BytesIO:
        temp_store_for_mp3 = BytesIO()
        in_vcs = len(self.bot.voice_clients)
        if   in_vcs < 5:  max_range = 50
        elif in_vcs < 20: max_range = 20
        else:
            max_range = 10

        for attempt in range(1, max_range):
            try:
                gTTS.gTTS(text=text, lang=lang).write_to_fp(temp_store_for_mp3)
                break
            except ValueError:
                if attempt == max_range:
                    raise

        return temp_store_for_mp3

    @commands.Cog.listener()
    async def on_message(self, message):
        try:    self.bot.starting_message.content
        except: return print("Skipping message, bot not started!")

        if message.guild is not None:
            saythis = message.clean_content.lower()

            # Get settings
            autojoin, bot_ignore, channel = await asyncio.gather(
                self.bot.settings.get(message.guild, "auto_join"),
                self.bot.settings.get(message.guild, "bot_ignore"),
                self.bot.settings.get(message.guild, "channel")
                )

            starts_with_tts = saythis.startswith("-tts")

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
                        if message.guild.id not in self.bot.playing:
                            self.bot.playing[message.guild.id] = 0

                        # Auto Join
                        if message.guild.voice_client is None and autojoin and self.bot.playing[message.guild.id] in (0, 1):
                            try:  channel = message.author.voice.channel
                            except AttributeError: return

                            self.bot.playing[message.guild.id] = 3
                            await channel.connect()
                            self.bot.playing[message.guild.id] = 0

                        # Get settings
                        lang, xsaid, repeated_chars_limit = await asyncio.gather(
                            self.bot.setlangs.get(message.author),
                            self.bot.settings.get(message.guild, "xsaid"),
                            self.bot.settings.get(message.guild, "repeated_chars")
                        )

                        # Emoji filter
                        saythis = basic.emojitoword(saythis)

                        # Acronyms and removing -tts
                        saythis = f" {saythis} "
                        acronyms = {
                            "iirc": "if I recall correctly",
                            "wdym": "what do you mean",
                            "imo": "in my opinion",
                            "irl": "in real life",
                            "gtg": "got to go",
                            ":)": "smiley face",
                            "rn": "right now",
                            ":(": "sad face",
                            "uwu": "oowoo",
                            "@": "at",
                            "™️": "tm"
                        }

                        if starts_with_tts: acronyms["-tts"] = ""
                        for toreplace, replacewith in acronyms.items():
                            saythis = saythis.replace(f" {toreplace} ", f" {replacewith} ")

                        saythis = saythis[1:-1]
                        if saythis == "?":  saythis = "what"

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

                        # Toggleable X said and attachment detection
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

                        try:
                            await self.get_tts(message, saythis, lang)
                        except ValueError:
                            return print(f"Run out of attempts generating {saythis}.")
                        except AssertionError:
                            return print(f"Skipped {saythis}, apparently blank message.")

                        # Queue, please don't touch this, it works somehow
                        while self.bot.playing[message.guild.id] != 0:
                            if self.bot.playing[message.guild.id] == 2: return
                            await asyncio.sleep(0.5)

                        self.bot.playing[message.guild.id] = 1

                        while self.bot.queue[message.guild.id] != dict():
                            # Sort Queue
                            self.bot.queue[message.guild.id] = basic.sort_dict(self.bot.queue[message.guild.id])

                            # Select first in queue
                            message_id_to_read = next(iter(self.bot.queue[message.guild.id]))
                            selected = self.bot.queue[message.guild.id][message_id_to_read]

                            # Play selected audio
                            vc = message.guild.voice_client
                            if vc is not None:
                                try:    vc.play(FFmpegPCMAudio(selected, pipe=True, options='-loglevel "quiet"'))
                                except discord.errors.ClientException:  pass # sliences desyncs between discord.py and discord, implement actual fix soon!

                                while vc.is_playing():  await asyncio.sleep(0.5)

                                # Delete said message from queue
                                if message_id_to_read in self.bot.queue[message.guild.id]:
                                    del self.bot.queue[message.guild.id][message_id_to_read]

                            else:
                                # If not in a voice channel anymore, clear the queue
                                self.bot.queue[message.guild.id] = dict()

                        # Queue should be empty now, let next on_message though
                        self.bot.playing[message.guild.id] = 0
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
                    if not files and not message.content: return

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
        playing = basic.get_value(self.bot.playing, guild.id)

        if member == self.bot.user:   return # someone other than bot left vc
        elif not (before.channel and not after.channel):   return # user left voice channel
        elif not vc:   return # bot in a voice channel

        elif len([member for member in vc.channel.members if not member.bot]) != 0:    return # bot is only one left
        elif playing not in (0, 1):   return # bot not already joining/leaving a voice channel

        else:
            self.bot.playing[guild.id] = 2
            await vc.disconnect(force=True)
            self.bot.playing[guild.id] = 0
