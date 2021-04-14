import asyncio
import re
from functools import partial as make_func
from inspect import cleandoc
from io import BytesIO
from itertools import groupby
from random import choice as pick_random

import asyncgTTS
import discord
from discord.ext import commands
from mutagen import mp3 as mutagen
from pydub import AudioSegment
from voxpopuli import Voice

from utils import basic
from patched_FFmpegPCM import FFmpegPCMAudio


def setup(bot):
    bot.add_cog(events_main(bot))


class events_main(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.bot.blocked = False

    async def get_tts(self, message, text, lang, max_length, prefix):
        lang = lang.split("-")[0]
        cached_mp3 = await self.bot.cache.get(text, lang, message.id)

        if cached_mp3:
            self.bot.queue[message.guild.id][message.id] = cached_mp3
            return

        if self.bot.blocked:
            make_espeak_func = make_func(self.make_espeak, text, lang, max_length)
            wav = await self.bot.loop.run_in_executor(None, make_espeak_func)

            if wav:
                self.bot.queue[message.guild.id][message.id] = wav

            return

        try:
            gtts_resp = await self.bot.gtts.get(text=text, lang=lang)
        except asyncgTTS.RatelimitException:
            if self.bot.blocked:
                return

            self.bot.blocked = True
            return await asyncio.gather(
                self.rate_limit_handler(),
                self.send_fallback_messages(prefix),
            )
        except asyncgTTS.easygttsException as e:
            error_message = str(e)
            status_code = error_message[:3]

            if status_code == "400":
                return

            raise

        try:
            temp_store_for_mp3 = BytesIO(gtts_resp)
            temp_store_for_mp3.seek(0)

            file_length = int(mutagen.MP3(temp_store_for_mp3).info.length)
        except mutagen.HeaderNotFoundError:
            return

        # Discard if over max length seconds
        if file_length > int(max_length):
            return

        temp_store_for_mp3.seek(0)
        mp3 = temp_store_for_mp3.read()

        await self.bot.cache.set(text, lang, message.id, mp3)
        self.bot.queue[message.guild.id][message.id] = mp3

    def make_espeak(self, text, lang, max_length):
        voice = Voice(lang=basic.gtts_to_espeak[lang], speed=130, volume=2) if lang in basic.gtts_to_espeak else Voice(lang="en",speed=130)
        wav = voice.to_audio(text)

        pydub_wav = AudioSegment.from_file_using_temporary_files(BytesIO(wav))
        if len(pydub_wav)/1000 > int(max_length):
            return

        return wav

    async def send_fallback_messages(self, prefix):
        embed = discord.Embed(title="TTS Bot has been blocked by Google")
        embed.description = cleandoc(f"""
            During this temporary block, voice has been swapped to a worse quality voice.
            If you want to avoid this, consider TTS Bot Premium, which you can get by donating via Patreon: `{prefix}donate`
            """)
        embed.set_footer(text="You can join the support server for more info: discord.gg/zWPWwQC")

        for voice_client in self.bot.voice_clients:
            channel_id = await self.bot.settings.get(voice_client.guild, setting="channel")
            channel = voice_client.guild.get_channel(int(channel_id))

            if not channel:
                continue

            permissions = channel.permissions_for(voice_client.guild.me)
            if permissions.send_messages and permissions.embed_links:
                try:
                    await channel.send(embed=embed)
                    await asyncio.sleep(1)
                except:
                    pass

        await self.bot.channels["logs"].send("**Fallback/RL messages have been sent.**")

    async def rate_limit_handler(self):
        await self.bot.channels["logs"].send("**Swapped to espeak**")

        # I know this code isn't pretty
        while True:
            try:
                await self.bot.gtts.get(text="Rate limit test", lang="en")
                break
            except asyncgTTS.RatelimitException:
                await self.bot.channels["logs"].send("**Rate limit still in place, waiting another hour.**")
            except:
                await self.bot.channels["logs"].send("**Failed to connect to easygTTS for unknown reason.**")

            await asyncio.sleep(3601)

        await self.bot.channels["logs"].send("**Swapping back to easygTTS**")
        self.bot.blocked = False



    @commands.Cog.listener()
    async def on_message(self, message):
        if message.guild is not None:
            message_clean = message.clean_content.lower()

            # Get settings
            repeated_chars_limit, bot_ignore, msg_length, autojoin, channel, prefix, xsaid = await self.bot.settings.get(
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
            if message.channel.id != int(channel):
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

            # Fix values
            if message.guild.id not in self.bot.queue:
                self.bot.queue[message.guild.id] = dict()
            if message.guild.id not in self.bot.message_locks:
                self.bot.message_locks[message.guild.id] = asyncio.Lock()

            should_return = self.bot.should_return.get(message.guild.id)

            # Auto Join
            if message.guild.voice_client is None and autojoin and not should_return:
                try:
                    channel = message.author.voice.channel
                except AttributeError:
                    return

                self.bot.should_return[message.guild.id] = True
                await channel.connect()
                self.bot.should_return[message.guild.id] = False

            # Get lang
            lang = await self.bot.setlangs.get(message.author)

            # Emoji filter
            message_clean = basic.emojitoword(message_clean)

            # Acronyms and removing -tts
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
            link_starters = ("https://", "http://", "www.")
            contained_url = any(map(lambda w: w.startswith(link_starters), message_clean.split()))

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

            if basic.remove_chars(message_clean, " ", "?", ".", ")", "'", "!", '"') == "":
                return

            # Repeated chars removal if setting is not 0
            repeated_chars_limit = int(repeated_chars_limit)
            if message_clean.isprintable() and repeated_chars_limit != 0:
                message_clean_chars = ["".join(grp) for num, grp in groupby(message_clean)]

                for char in message_clean_chars:
                    if len(char) > repeated_chars_limit:
                        message_clean_list.append(char[0] * repeated_chars_limit)
                    else:
                        message_clean_list.append(char)

                message_clean = "".join(message_clean_list)

            # Adds filtered message to queue
            await self.get_tts(message, message_clean, lang, msg_length, prefix)

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
                    if vc:
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

        elif not message.author.bot:
            pins = await message.author.pins()

            if [True for pinned_message in pins if pinned_message.embeds and pinned_message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!"]:
                if "https://discord.gg/" in message.content.lower():
                    await message.author.send(f"Join https://discord.gg/zWPWwQC and look in <#694127922801410119> to invite {self.bot.user.mention}!")

                elif message.content.lower() == "help":
                    await message.channel.send("We cannot help you unless you ask a question, if you want the help command just do `-help`!")
                    await self.bot.channels["logs"].send(f"{message.author} just got the 'dont ask to ask' message")

                elif not await self.bot.blocked_users.check(message.author):
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

                embed_message = cleandoc("""
                    **All messages after this will be sent to a private channel where we can assist you.**
                    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
                    There are some basic rules if you want to get help though:
                    `1.` Ask your question, don't just ask for help
                    `2.` Don't spam, troll, or send random stuff (including server invites)
                    `3.` Many questions are answered in `-help`, try that first (also the default prefix is `-`)
                """)

                embed = discord.Embed(title=f"Welcome to {self.bot.user.name} Support DMs!", description=embed_message)
                embed.set_footer(text=pick_random(basic.footer_messages))

                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await self.bot.channels["logs"].send(f"{message.author} just got the 'Welcome to Support DMs' message")
                await dm_message.pin()

    def finish_future(self, fut, *args):
        if not fut.done():
            self.bot.loop.call_soon_threadsafe(fut.set_result, "done")

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
