import asyncio
import re
import shutil
from asyncio.exceptions import TimeoutError as asyncio_TimeoutError
from concurrent.futures._base import TimeoutError as concurrent_TimeoutError
from configparser import ConfigParser
from functools import partial as make_func
from inspect import cleandoc
from io import BytesIO, StringIO
from itertools import groupby
from os import listdir, remove
from os.path import exists
from random import choice as pick_random
from subprocess import call
from sys import exc_info
from time import monotonic
from traceback import format_exception
from typing import Optional

import asyncpg
import discord
import gtts as gTTS
from cryptography.fernet import Fernet
from discord.ext import commands, tasks
from mutagen.mp3 import MP3, HeaderNotFoundError

from cogs import common_trusted
from patched_FFmpegPCM import FFmpegPCMAudio
from utils import basic, cache, settings

#//////////////////////////////////////////////////////
config = ConfigParser()
config.read("config.ini")
t = config["Main"]["Token"]
config_channels = config["Channels"]

if "key" not in config["Main"]:
    key = Fernet.generate_key()
    config["Main"]["key"] = str(key)
    with open("config.ini", "w") as configfile: config.write(configfile)

cache_key_str = config["Main"]["key"][2:-1]
cache_key_bytes = cache_key_str.encode()

# Define random variables
BOT_PREFIX = "-"
before = monotonic()
NoneType = type(None)
to_enabled = {True: "Enabled", False: "Disabled"}

tts_langs = gTTS.lang.tts_langs()

footer_messages = (
    "If you find a bug or want to ask a question, join the support server: discord.gg/zWPWwQC",
    "if you want to support the development of TTS Bot, check out -donate!",
    "There are loads of customizable settings, check out -settings help",
)

if exists("activity.txt"):
    with open("activity.txt") as f2, open("activitytype.txt") as f3, open("status.txt") as f4:
        activity = f2.read()
        activitytype = f3.read()
        status = f4.read()

    config["Activity"] = {"name": activity, "type": activitytype, "status": status}

    with open("config.ini", "w") as configfile: config.write(configfile)
    remove("activitytype.txt")
    remove("activity.txt")
    remove("status.txt")

async def require_chunk(ctx):
    if ctx.guild and not ctx.guild.chunked:
        try:    chunk_guilds.start()
        except RuntimeError: pass

        if ctx.guild.id not in bot.chunk_queue:
            bot.chunk_queue.append(ctx.guild.id)

    return True

@tasks.loop(seconds=1)
async def chunk_guilds():
    chunk_queue = bot.chunk_queue

    for guild in chunk_queue:
        guild = bot.get_guild(guild)

        if not guild.chunked:
            await guild.chunk(cache=True)

        bot.chunk_queue.remove(guild.id)

# Define bot and remove overwritten commands
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True)
status = getattr(discord.Status, config["Activity"]["status"])

bot = commands.AutoShardedBot(
    status=status,
    intents=intents,
    activity=activity,
    case_insensitive=True,
    command_prefix=BOT_PREFIX,
    chunk_guilds_at_startup=False,
)

bot.queue = dict()
bot.playing = dict()
bot.channels = dict()
bot.chunk_queue = list()
bot.trusted = basic.remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")

if exists("cogs/common_user.py"):
    bot.load_extension("cogs.common_owner")
    bot.load_extension("cogs.common_user")
else:
    print("Error: Cannot find cogs to load? Did you do 'git clone --recurse-submodules'?")
    raise SystemExit

for overwriten_command in ("help", "end", "botstats"):
    bot.remove_command(overwriten_command)
#//////////////////////////////////////////////////////
class Main(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    def cog_unload(self):
        self.cache_cleanup.cancel()

    def is_trusted(ctx):
        if str(ctx.author.id) in bot.trusted: return True
        else: raise commands.errors.NotOwner

    @tasks.loop(seconds=60.0)
    async def cache_cleanup(self):
        try:
            cache_size = basic.get_size("cache")
            if cache_size >= 1073741824:
                print("Deleting 100 messages from cache!")
                cache_folder = listdir("cache")
                cache_folder.sort(reverse=False, key=lambda x: int(''.join(filter(str.isdigit, x))))

                for count, cached_message in enumerate(cache_folder):
                    remove(f"cache/{cached_message}")
                    cache.remove(cached_message)

                    if count == 100: break

        except Exception as e:
            error = getattr(e, 'original', e)

            temp = f"```{''.join(format_exception(type(error), error, error.__traceback__))}```"
            if len(temp) >= 1900:
                with open("temp.txt", "w") as f:  f.write(temp)
                await self.bot.channels["errors"].send(file=discord.File("temp.txt"))
            else:
                await self.bot.channels["errors"].send(temp)

    @cache_cleanup.before_loop
    async def before_file_saving_loop(self):
        await self.bot.wait_until_ready()

    async def get_tts(self, message, text, lang):
        cache_mp3 = await cache.get(text, lang, message.id)
        if not cache_mp3:
            make_tts_func = make_func(self.make_tts, text, lang)
            temp_store_for_mp3 = await self.bot.loop.run_in_executor(None, make_tts_func)

            try:
                temp_store_for_mp3.seek(0)
                file_length = int(MP3(temp_store_for_mp3).info.length)
            except HeaderNotFoundError:
                return

            # Discard if over max length seconds
            max_length = await settings.get(message.guild, "msg_length")
            if file_length < int(max_length):
                temp_store_for_mp3.seek(0)
                temp_store_for_mp3 = temp_store_for_mp3.read()

                self.bot.queue[message.guild.id][message.id] = temp_store_for_mp3
                await cache.set(text, lang, message.id, temp_store_for_mp3)
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
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    @commands.is_owner()
    async def end(self, ctx):
        self.cache_cleanup.cancel()
        await self.bot.close()

    @commands.command()
    @commands.is_owner()
    async def leave_unused_guilds(self, ctx, sure: bool = False):
        return await ctx.send("Disabled until rewritten, json -> psql rewrite broke this command.")

        guilds_to_leave = []
        with open("settings.json") as f:
            temp_settings = f.read()

        for guild in self.bot.guilds:
            guild_id = str(guild.id)
            if guild_id not in temp_settings:
                guilds_to_leave.append(guild)

        if not sure:
            await ctx.send(f"Are you sure you want me to leave {len(guilds_to_leave)} guilds?")
        else:
            for guild in guilds_to_leave:
                try:    await guild.owner.send("Hey! TTS Bot has not been setup on your server so I have left! If you want to reinvite me, join https://discord.gg/zWPWwQC and look in #invites-and-rules.")
                except: pass
                await guild.leave()

            await self.bot.channels["logs"].send(f"Just left {len(guilds_to_leave)} guilds due to no setup, requested by {ctx.author.name}")

    @commands.command()
    @commands.is_owner()
    async def channellist(self, ctx):
        channellist = str()
        for guild1 in self.bot.guilds:
            try:  channellist = f"{channellist} \n{str(guild1.voice_client.channel)} in {guild1.name}"
            except: pass

        tempplaying = dict()
        for key in self.bot.playing:
            if self.bot.playing[key] != 0:
                tempplaying[key] = self.bot.playing[key]
        await ctx.send(f"TTS Bot Voice Channels:\n{channellist}\nAnd just incase {str(tempplaying)}")

    @commands.command()
    @commands.check(is_trusted)
    async def cleanup(self, ctx):
        guild_id_list = [str(guild.id) for guild in self.bot.guilds]

        user_id_list = list()
        [[user_id_list.append(str(member.id)) for member in guild.members] for guild in bot.guilds]

        if exists("servers"):
            shutil.rmtree("servers", ignore_errors=True)

        await ctx.send("Done!")
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.Cog.listener()
    async def on_ready(self):
        await self.bot.wait_until_ready()

        global cache
        global settings
        global setlangs
        global nicknames
        global blocked_users
        global starting_message

        support_server_id = int(config["Main"]["main_server"])
        self.bot.supportserver = self.bot.get_guild(support_server_id)

        while self.bot.supportserver is None:
            print("Waiting 5 seconds")
            await asyncio.sleep(5)
            self.bot.supportserver = self.bot.get_guild(support_server_id)

        for channel_name in config_channels:
            channel_id = int(config_channels[channel_name])
            channel_object = self.bot.supportserver.get_channel(channel_id)
            self.bot.channels[channel_name] = channel_object

        try:
            await starting_message.edit(content=f"~~{starting_message.content}~~")
            starting_message = await self.bot.channels["logs"].send(f"Restarted as {self.bot.user.name}!")
            print(f":wagu: Restarting as {self.bot.user.name}!")
        except NameError:
            print(f"Starting as {self.bot.user.name}")

            if self.bot.user.id == 513423712582762502:
                try:
                    bot.load_extension("bot_lists")
                except:
                    print("Bot lists cog failed to load, skipped!")

            pool = await asyncpg.create_pool(
                        host=config["PostgreSQL Info"]["ip"],
                        user=config["PostgreSQL Info"]["name"],
                        database=config["PostgreSQL Info"]["db"],
                        password=config["PostgreSQL Info"]["pass"]
                    )

            blocked_users = settings.blocked_users_class(pool)
            cache = cache.cache(cache_key_bytes, pool)
            nicknames = settings.nickname_class(pool)
            setlangs = settings.setlangs_class(pool)
            settings = settings.settings_class(pool)
            common_trusted.setup_with_db(bot, pool)

            try:    self.cache_cleanup.start()
            except RuntimeError:    pass

            for guild in self.bot.guilds:
                self.bot.playing[guild.id] = 0
                self.bot.queue[guild.id] = dict()

            starting_message = await self.bot.channels["logs"].send(f"Started and ready! Took `{int(monotonic() - before)} seconds`")

    @commands.Cog.listener()
    async def on_message(self, message):
        try:    starting_message.content
        except: return print("Skipping message, bot not started!")

        if message.channel.id == 749971061843558440 and message.embeds and str(message.author) == "GitHub#0000":
            if " new commit" in message.embeds[0].title:
                update_for_main = message.embeds[0].title.startswith("[Discord-TTS-Bot:master]") and self.bot.user.id == 513423712582762502
                update_for_dev = message.embeds[0].title.startswith("[Discord-TTS-Bot:dev]") and self.bot.user.id == 698218518335848538

                if update_for_main or update_for_dev:
                    await self.bot.channels['logs'].send("Detected new bot commit! Pulling changes")
                    call(['git', 'pull'])
                    print("===============================================")
                    await self.bot.channels['logs'].send("Restarting bot...")
                    await self.end(message)

        elif message.guild is not None:
            saythis = message.clean_content.lower()

            # Get settings
            autojoin, bot_ignore, channel = await asyncio.gather(
                settings.get(message.guild, "auto_join"),
                settings.get(message.guild, "bot_ignore"),
                settings.get(message.guild, "channel")
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
                if saythis.startswith(BOT_PREFIX) is False or starts_with_tts:

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
                            setlangs.get(message.author),
                            settings.get(message.guild, "xsaid"),
                            settings.get(message.guild, "repeated_chars")
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
                            said_name = await nicknames.get(message.guild, message.author)
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
                    await self.bot.channels["logs"].send(f"{str(message.author)} just got the 'dont ask to ask' message")

                elif not await blocked_users.check(message.author):
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
                embed.set_footer(text=pick_random(footer_messages))

                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await self.bot.channels["logs"].send(f"{str(message.author)} just got the 'Welcome to Support DMs' message")
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

    @bot.event
    async def on_error(event, *args, **kwargs):
        errors = exc_info()
        info = "No Info"

        if event == "on_message":
            message = args[0]

            if message.guild is None:
                info = f"DM support | Sent by {message.author}"
            else:
                info = f"General TTS | Sent by {message.author}"

        elif event in ("on_guild_join", "on_guild_remove"):
            guild = args[0]
            info = f"Guild = {guild.name} | {guild.id}"

        try:    error_message = f"Event: `{event}`\nInfo: `{info}`\n```{''.join(format_exception(errors[0], errors[1], errors[2]))}```"
        except: error_message = f"```{''.join(format_exception(errors[0], errors[1], errors[2]))}```"

        await bot.channels["errors"].send(cleandoc(error_message))

    @commands.Cog.listener()
    async def on_command_error(self, ctx, error):
        if hasattr(ctx.command, 'on_error') or isinstance(error, (commands.CommandNotFound, commands.NotOwner)):
            return

        if ctx.guild is not None and not ctx.guild.chunked:
            message = "**Warning:** The server you are in hasn't been fully loaded yet, this could cause issues!"

            try:  await ctx.send(message)
            except:
                try:    await ctx.author.send(message)
                except: pass

        error = getattr(error, 'original', error)

        if isinstance(error, (commands.BadArgument, commands.MissingRequiredArgument, commands.UnexpectedQuoteError, commands.ExpectedClosingQuoteError)):
            return await ctx.send(f"Did you type the command right, {ctx.author.mention}? Try doing -help!")

        elif isinstance(error, (concurrent_TimeoutError, asyncio_TimeoutError)):
            return await ctx.send("**Timeout Error!** Do I have perms to see the channel you are in? (if yes, join https://discord.gg/zWPWwQC and ping Gnome!#6669)")

        elif isinstance(error, commands.NoPrivateMessage):
            return await ctx.author.send("**Error:** This command cannot be used in private messages!")

        elif isinstance(error, commands.MissingPermissions):
            return await ctx.send(f"**Error:** You are missing {', '.join(error.missing_perms)} to run this command!")

        elif isinstance(error, commands.BotMissingPermissions):
            if "send_messages" in error.missing_perms:
                return await ctx.author.send("**Error:** I could not complete this command as I don't have send messages permissions!")

            return await ctx.send(f"**Error:** I am missing the permissions: {', '.join(error.missing_perms)}")

        elif isinstance(error, discord.errors.Forbidden):
            await self.bot.channels["errors"].send(f"```discord.errors.Forbidden``` caused by {str(ctx.message.content)} sent by {str(ctx.author)}")
            return await ctx.author.send("Unknown Permission Error, please give TTS Bot the required permissions. If you want this bug fixed, please do `-suggest *what command you just run*`")

        first_part = f"{str(ctx.author)} caused an error with the message: {ctx.message.clean_content}"
        second_part = ''.join(format_exception(type(error), error, error.__traceback__))
        temp = f"{first_part}\n```{second_part}```"

        if len(temp) >= 2000:
            await self.bot.channels["errors"].send(
                file=discord.File(
                    StringIO(f"{first_part}\n{second_part}"),
                    filename="long error.txt"
                ))
        else:
            await self.bot.channels["errors"].send(temp)

    @commands.Cog.listener()
    async def on_guild_join(self, guild):
        self.bot.queue[guild.id] = dict()

        await self.bot.channels["servers"].send(f"Just joined {guild.name}! I am now in {str(len(self.bot.guilds))} different servers!".replace("@", "@ "))

        owner = await guild.fetch_member(guild.owner_id)
        try:    await owner.send(cleandoc(f"""
            Hello, I am {self.bot.user.name} and I have just joined your server {guild.name}
            If you want me to start working do `-setup <#text-channel>` and everything will work in there
            If you want to get support for {self.bot.user.name}, join the support server!
            https://discord.gg/zWPWwQC
            """))
        except discord.errors.HTTPException:    pass

        try:
            if owner.id in [member.id for member in self.bot.supportserver.members if not isinstance(member, NoneType)]:
                role = self.bot.supportserver.get_role(738009431052386304)
                await self.bot.supportserver.get_member(owner.id).add_roles(role)

                embed = discord.Embed(description=f"**Role Added:** {role.mention} to {owner.mention}\n**Reason:** Owner of {guild.name}")
                embed.set_author(name=f"{str(owner)} (ID {owner.id})", icon_url=owner.avatar_url)

                await self.bot.channels["logs"].send(embed=embed)
        except AttributeError:  pass

    @commands.Cog.listener()
    async def on_guild_remove(self, guild):
        await settings.remove(guild)
        self.bot.playing[guild.id] = 2

        if guild.id in self.bot.queue:  self.bot.queue.pop(guild.id, None)
        if guild.id in self.bot.playing:  self.bot.playing.pop(guild.id, None)
        await self.bot.channels["servers"].send(f"Just left/got kicked from {str(guild.name)}. I am now in {str(len(self.bot.guilds))} servers".replace("@", "@ "))
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    async def uptime(self, ctx):
        await ctx.send(f"{self.bot.user.mention} has been up for {int((monotonic() - before) // 60)} minutes")

    @commands.command()
    async def debug(self, ctx, reset="nope"):
        if reset.lower() == "reset":
            self.bot.playing[ctx.guild.id] = 0
            self.bot.queue[ctx.guild.id] = dict()
            embed = discord.Embed(
                title="Values Reset!",
                description="Playing and queue values for this guild have been reset, hopefully this will fix issues."
            )
            embed.set_footer(text="Debug Command, please only run if told.")
            return await ctx.send(embed=embed)

        with open("queue.txt", "w") as f:   f.write(str(self.bot.queue[ctx.guild.id]))
        await ctx.author.send(
            cleandoc(f"""
                **TTS Bot debug info!**
                Playing is currently set to {str(self.bot.playing[ctx.guild.id])}
                Guild is chunked: {str(ctx.guild.chunked)}
                Queue for {ctx.guild.name} | {ctx.guild.id} is attached:
            """),
            file=discord.File("queue.txt"))

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command(aliases=["commands"])
    async def help(self, ctx):
        message = """
          `-setup #textchannel`: Setup the bot to read messages from that channel

          `-join`: Joins the voice channel you're in
          `-leave`: Leaves voice channel

          `-settings`: Display the current settings
          `-settings help`: Displays list of available settings
          `-set property value`: Sets a setting
          """
        message1 = """
          `-help`: Shows this message
          `-botstats`: Shows various different stats
          `-donate`: Help improve TTS Bot's development and hosting through Patreon
          `-suggest *suggestion*`: Suggests a new feature! (could also DM TTS Bot)
          `-invite`: Sends the instructions to invite TTS Bot!"""

        embed = discord.Embed(title="TTS Bot Help!", url="https://discord.gg/zWPWwQC", description=cleandoc(message), color=0x3498db)
        embed.add_field(name="Universal Commands", value=cleandoc(message1), inline=False)
        embed.set_footer(text="Do you want to get support for TTS Bot or invite it to your own server? https://discord.gg/zWPWwQC")
        await ctx.send(embed=embed)

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command(aliases=["botstats", "stats"])
    async def info(self, ctx):
        channels = int()
        for guild in self.bot.guilds:
            try:
                if guild.voice_client:
                    channels += 1
            except:
                pass

        main_section = cleandoc(f"""
          Currently in:
            :small_blue_diamond: {str(channels)} voice channels
            :small_orange_diamond: {len(self.bot.guilds)} servers
          and can be used by {sum([guild.member_count for guild in self.bot.guilds]):,} people!
        """)

        footer = cleandoc("""
            Support Server: https://discord.gg/zWPWwQC
            Repository: https://github.com/Gnome-py/Discord-TTS-Bot
        """)

        embed = discord.Embed(title=f"{self.bot.user.name}: Now open source!", description=main_section, url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_footer(text=footer)
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))

        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def join(self, ctx):
        if basic.get_value(self.bot.playing, ctx.guild.id) == 3:
            return await ctx.send("Error: Already trying to join your voice channel!")

        if ctx.channel.id != int(await settings.get(ctx.guild, "channel")):
            return await ctx.send("Error: Wrong channel, do -channel get the channel that has been setup.")

        if ctx.author.voice is None:
            return await ctx.send("Error: You need to be in a voice channel to make me join your voice channel!")

        channel = ctx.author.voice.channel
        permissions = channel.permissions_for(ctx.guild.me)

        if not permissions.view_channel:
            return await ctx.send("Error: Missing Permission to view your voice channel!")

        if not permissions.speak or not permissions.use_voice_activation:
            return await ctx.send("Error: I do not have permssion to speak!")

        if ctx.guild.voice_client is not None and ctx.guild.voice_client == channel:
            return await ctx.send("Error: I am already in your voice channel!")

        if ctx.guild.voice_client is not None and ctx.guild.voice_client != channel:
            return await ctx.send("Error: I am already in a voice channel!")

        self.bot.playing[ctx.guild.id] = 3
        await channel.connect()
        self.bot.playing[ctx.guild.id] = 0

        embed = discord.Embed(
            title="Joined your voice channel!",
            description="Just type normally and TTS Bot will say your messages!"
        )
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))
        embed.set_footer(text=pick_random(footer_messages))

        await self.get_tts(ctx.message, f"{ctx.guild.me.display_name} said: Joined your voice channel!", "en-us")
        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.bot_has_permissions(send_messages=True)
    @commands.command()
    async def leave(self, ctx):
        if basic.get_value(self.bot.playing, ctx.guild.id) == 2:
            return await ctx.send("Error: Already trying to leave your voice channel!")

        if ctx.channel.id != int(await settings.get(ctx.guild, "channel")):
            return await ctx.send("Error: Wrong channel, do -channel get the channel that has been setup.")

        if basic.get_value(self.bot.playing, ctx.guild.id) == 3:
            return await ctx.send("Error: Trying to join a voice channel!")

        elif ctx.author.voice is None:
            return await ctx.send("Error: You need to be in a voice channel to make me leave!")

        elif ctx.guild.voice_client is None:
            return await ctx.send("Error: How do I leave a voice channel if I am not in one?")

        elif ctx.author.voice.channel != ctx.guild.voice_client.channel:
            return await ctx.send("Error: You need to be in the same voice channel as me to make me leave!")

        self.bot.playing[ctx.guild.id] = 2
        await ctx.guild.voice_client.disconnect(force=True)
        self.bot.playing[ctx.guild.id] = 0

        await ctx.send("Left voice channel!")

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def channel(self, ctx):
        channel = int(await settings.get(ctx.guild, "channel"))

        if channel == ctx.channel.id:
            await ctx.send("You are in the right channel already!")
        elif channel != 0:
            await ctx.send(f"The current setup channel is: <#{channel}>")
        else:
            await ctx.send("The channel hasn't been setup, do `-setup #textchannel`")

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def tts(self, ctx):
        if ctx.message.content == f"{BOT_PREFIX}tts":
            await ctx.send(f"You don't need to do `{BOT_PREFIX}tts`! {self.bot.user.mention} is made to TTS any message, and ignore messages starting with `{BOT_PREFIX}`!")

class Settings(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def settings(self, ctx, *, help = None):
        if not isinstance(help, NoneType):  help = help.lower()

        if help == "help":
            message = cleandoc("""
              -set channel `#channel`: Sets the text channel to read from
              -set xsaid `true/false`: Enable/disable "person said" before every message
              -set autojoin `true/false`: Auto joins a voice channel when a text is sent
              -set ignorebots `true/false`: Do not read other bot messages
              -set nickname `@person` `new name`: Sets your (or someone else if admin) name for xsaid.
              -set limits: Do `-settings limits help` to check!

              -set voice `language-code`: Changes your voice to a `-voices` code, equivalent to `-voice`""")
            embed = discord.Embed(title="Settings > Help", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="Available properties:", value=message, inline=False)
        elif help == "limits":
            return await self.limits(ctx)
        elif help == "limits help":
            return await self.help(ctx)
        else:
            lang, say, channel, join, bot_ignore, nickname = await asyncio.gather(
                setlangs.get(ctx.author),
                settings.get(ctx.guild, "xsaid"),
                settings.get(ctx.guild, "channel"),
                settings.get(ctx.guild, "auto_join"),
                settings.get(ctx.guild, "bot_ignore"),
                nicknames.get(ctx.guild, ctx.author)
            )

            channel = ctx.guild.get_channel(int(channel))

            if channel is None:
                channel = "has not been setup yet"
            else:
                channel = channel.name

            if nickname == ctx.author.display_name:
                nickname = "has not been set yet"

            # Show settings embed
            message1 = cleandoc(f"""
              :small_orange_diamond: Channel: `#{channel}`
              :small_orange_diamond: XSaid: `{say}`
              :small_orange_diamond: Auto Join: `{join}`
              :small_orange_diamond: Ignore Bots: `{bot_ignore}`
              :star: Limits: Do `-settings limits` to check!""")

            message2 = cleandoc(f"""
              :small_blue_diamond: Language: `{lang}`
              :small_blue_diamond: Nickname: `{nickname}`""")

            embed = discord.Embed(title="Current Settings", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="**Server Wide**", value=message1, inline=False)
            embed.add_field(name="**User Specific**", value=message2, inline=False)

        embed.set_footer(text="Change these settings with -set property value!")
        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.group()
    async def set(self, ctx):
        if ctx.invoked_subcommand is None:
            await ctx.send("Error: Invalid property, do `-settings help` to get a list!")

    @set.command()
    @commands.has_permissions(administrator=True)
    async def xsaid(self, ctx, value: bool):
        await settings.set(ctx.guild, "xsaid", value)
        await ctx.send(f"xsaid is now: {to_enabled[value]}")

    @set.command(aliases=["auto_join"])
    @commands.has_permissions(administrator=True)
    async def autojoin(self, ctx, value: bool):
        await settings.set(ctx.guild, "auto_join", value)
        await ctx.send(f"Auto Join is now: {to_enabled[value]}")

    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    @commands.has_permissions(administrator=True)
    async def botignore(self, ctx, value: bool):
        await settings.set(ctx.guild, "bot_ignore", value)
        await ctx.send(f"Ignoring Bots is now: {to_enabled[value]}")

    @set.command(aliases=["nick_name", "nickname", "name"])
    async def nick(self, ctx, user: Optional[discord.Member] = False, *, nickname):
        if user:
            if nickname:
                if not ctx.channel.permissions_for(ctx.author).administrator:
                    return await ctx.send("Error: You need admin to set other people's nicknames!")
            else:
                nickname = ctx.author.display_name
        else:
            user = ctx.author

        if not nickname:
            raise commands.UserInputError(ctx.message)

        if "<" in nickname and ">" in nickname:
            await ctx.send("Hey! You can't have mentions/emotes in your nickname!")
        elif not re.match(r'^(\w|\s)+$', nickname):
            await ctx.send("Hey! Please keep your nickname to only letters, numbers, and spaces!")
        else:
            await nicknames.set(ctx.guild, user, nickname)
            await ctx.send(embed=discord.Embed(title="Nickname Change", description=f"Changed {user.name}'s nickname to {nickname}"))

    @set.command()
    @commands.has_permissions(administrator=True)
    async def channel(self, ctx, channel: discord.TextChannel):
        await self.setup(ctx, channel)

    @set.command(aliases=("voice", "lang"))
    async def language(self, ctx, voicecode):
        await self.voice(ctx, voicecode)

    @set.group()
    @commands.has_permissions(administrator=True)
    async def limits(self, ctx):
        additional_message = None
        if ctx.invoked_subcommand is not None:  return
        if ctx.message.content != f"{BOT_PREFIX}set limits":    additional_message = "Error: Invalid property!"

        msg_length, repeated_chars = await asyncio.gather(
            settings.get(ctx.guild, "msg_length"),
            settings.get(ctx.guild, "repeated_chars")
        )

        message1 = cleandoc(f"""
            :small_orange_diamond: Max Message Length: `{msg_length} seconds`
            :small_orange_diamond: Max Repeated Characters: `{repeated_chars}`
            """)

        embed = discord.Embed(title="Current Limits", description=message1, url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_footer(text="Change these settings with -set limits property value!")
        await ctx.send(additional_message, embed=embed)

    @limits.command()
    async def help(self, ctx):
        message = cleandoc("""
            -set limits max_length `seconds`: Max seconds for a TTS'd message
            -set limits chars `number`: Max repetion of a character (0 = off)
            """)

        embed = discord.Embed(title="Settings > Limits > Help", url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.add_field(name="Available properties:", value=message, inline=False)
        embed.set_footer(text="Change these settings with -set limits property value!")
        await ctx.send(embed=embed)

    @limits.command(aliases=("length", "max_length", "max_msg_length", "msglength", "maxlength"))
    async def msg_length(self, ctx, length: int):
        if length > 60: return await ctx.send("Hey! You can't set max message length above 60 seconds!")
        if length < 20: return await ctx.send("Hey! You can't set max message length below 20 seconds!")

        await settings.set(ctx.guild, "msg_length", str(length))
        await ctx.send(f"Max message length (in seconds) is now: {length}")

    @limits.command(aliases=("repeated_characters", "repeated_letters", "chars"))
    async def repeated_chars(self, ctx, chars: int):
        if chars > 100: return await ctx.send("Hey! You can't set max repeated chars above 100!")
        if chars < 5: return await ctx.send("Hey! You can't set max repeated chars below 5!")

        await settings.set(ctx.guild, "repeated_chars", str(chars))
        await ctx.send(f"Max repeated characters is now: {chars}")

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.has_permissions(administrator=True)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def setup(self, ctx, channel: discord.TextChannel):
        await settings.set(ctx.guild, "channel", str(channel.id))

        embed = discord.Embed(
            title="TTS Bot has been setup!",
            description=cleandoc(f"""
                TTS Bot will now accept commands and read from {channel.mention}.
                Just do `-join` and start talking!
                """)
            )
        embed.set_footer(text=pick_random(footer_messages))
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))

        await ctx.send(embed=embed)

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def voice(self, ctx, lang: str):
        if lang in tts_langs:
            await setlangs.set(ctx.author, lang)
            await ctx.send(f"Changed your voice to: {tts_langs[lang]}")
        else:
            await ctx.send("Invalid voice, do `-voices`")

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    async def voices(self, ctx, lang: str = None):
        if lang in tts_langs:
            try:  return await self.voice(ctx, lang)
            except: return

        lang = await setlangs.get(ctx.author)
        langs_string = basic.remove_chars(list(tts_langs.keys()), "[", "]")

        embed = discord.Embed(title="TTS Bot Languages")
        embed.set_footer(text=pick_random(footer_messages))
        embed.add_field(name="Currently Supported Languages", value=langs_string)
        embed.add_field(name="Current Language used", value=f"{tts_langs[lang]} | {lang}")
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))

        await ctx.send(embed=embed)
#//////////////////////////////////////////////////////

bot.add_cog(Main(bot))
bot.add_cog(Settings(bot))
bot.run(t)
