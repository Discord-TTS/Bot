import asyncio
import json
import re
import shutil
import time
from asyncio.exceptions import TimeoutError as asyncio_TimeoutError
from concurrent.futures._base import TimeoutError as concurrent_TimeoutError
from configparser import ConfigParser
from inspect import cleandoc
from io import BytesIO
from os.path import exists
from subprocess import call
from traceback import format_exception
from typing import Optional, Union

import discord
import gtts as gTTS
from discord.ext import commands, tasks
from mutagen.mp3 import MP3

from patched_FFmpegPCM import FFmpegPCMAudio
from utils import settings, basic

#//////////////////////////////////////////////////////
config = ConfigParser()
config.read("config.ini")
t = config["Main"]["Token"]

# Define random variables
settings_loaded = False
before = time.monotonic()
to_enabled = {True: "Enabled", False: "Disabled"}
OPUS_LIBS = ('libopus-0.x86.dll', 'libopus-0.x64.dll', 'libopus-0.dll', 'libopus.so.0', 'libopus.0.dylib')

intents = discord.Intents.none()
intents.voice_states = True
intents.messages = True
intents.guilds = True
intents.members = True

# Define useful functions
def load_opus_lib(opus_libs=OPUS_LIBS):
    if opus.is_loaded():
        return True

    for opus_lib in opus_libs:
        try:    return opus.load_opus(opus_lib)
        except OSError: pass

        raise RuntimeError(f"Could not load an opus lib. Tried {', '.join(opus_libs)}")

def emojitoword(text):
    emojiAniRegex = re.compile(r'<a\:.+:\d+>')
    emojiRegex = re.compile(r'<:.+:\d+\d+>')
    words = text.split(' ')
    output = []

    for x in words:

        if emojiAniRegex.match(x):
            output.append(f"animated emoji {x.split(':')[1]}")
        elif emojiRegex.match(x):
            output.append(f"emoji {x.split(':')[1]}")
        else:
            output.append(x)

    return ' '.join([str(x) for x in output])

# Define bot and remove overwritten commands
BOT_PREFIX = "-"
bot = commands.Bot(command_prefix=BOT_PREFIX, chunk_guilds_at_startup=False, case_insensitive=True, intents=intents)

if exists("cogs/common_user.py"):
    bot.load_extension("cogs.common_owner")
    bot.load_extension("cogs.common_trusted")
    bot.load_extension("cogs.common_user")
elif exists("cogs/common.py"):
    bot.load_extension("cogs.common")
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
        self.avoid_file_crashes.cancel()

    def is_trusted(ctx):
        if str(ctx.author.id) in bot.trusted: return True
        else: raise commands.errors.NotOwner

    async def fill_cache(self):
        await self.bot.supportserver.chunk(cache=True)
        for guild in self.bot.guilds:
            if not guild.chunked:
                await guild.chunk(cache=True)

    @tasks.loop(seconds=60.0)
    async def avoid_file_crashes(self):
        try:    settings.save()
        except Exception as e:
            error = getattr(e, 'original', e)

            temp = f"```{''.join(format_exception(type(error), error, error.__traceback__))}```"
            if len(temp) >= 1900:
                with open("temp.txt", "w") as f:  f.write(temp)
                await self.bot.channels["errors"].send(file=discord.File("temp.txt"))
            else:
                await self.bot.channels["errors"].send(temp)

    @avoid_file_crashes.before_loop
    async def before_file_saving_loop(self):
        await self.bot.wait_until_ready()

#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    @commands.is_owner()
    async def end(self, ctx):
        self.avoid_file_crashes.cancel()
        settings.save()

        with open("setlangs.json", "w") as f1, open("blocked_users.json", "w") as f2:
            json.dump(self.bot.setlangs, f1)
            json.dump(self.bot.blocked_users, f2)

        await self.bot.close()

    @commands.command()
    @commands.check(is_trusted)
    async def debug(self, ctx):
        with open("queue.txt", "w") as f:   f.write(str(self.bot.queue[ctx.guild.id]))
        await ctx.author.send(
            cleandoc(f"""
                Playing is currently set to {str(self.bot.playing[ctx.guild.id])}
                Guild is chunked: {str(ctx.guild.chunked)}
                Queue for {ctx.guild.name} | {ctx.guild.id} is attached:
            """),
            file=discord.File("queue.txt"))

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
    @commands.is_owner()
    async def trust(self, ctx, mode, user: Union[discord.User, str] = ""):
        if mode == "list":
            await ctx.send("\n".join(self.bot.trusted))

        elif isinstance(user, str):
            return

        elif mode == "add":
            self.bot.trusted.append(str(user.id))
            config["Main"]["trusted_ids"] = str(self.bot.trusted)
            with open("config.ini", "w") as configfile:
                config.write(configfile)

            await ctx.send(f"Added {str(user)} | {user.id} to the trusted members")

        elif mode == "del":
            if str(user.id) in self.bot.trusted:
                self.bot.trusted.remove(str(user.id))
                config["Main"]["trusted_ids"] = str(self.bot.trusted)
                with open("config.ini", "w") as configfile:
                    config.write(configfile)

                await ctx.send(f"Removed {str(user)} | {user.id} from the trusted members")

    @commands.command()
    @commands.check(is_trusted)
    async def cleanup(self, ctx):
        guild_id_list = [str(guild.id) for guild in self.bot.guilds]
        settings.cleanup()

        if exists("servers"):
            shutil.rmtree("servers", ignore_errors=True)

        await ctx.send("Done!")

    @commands.command()
    @commands.check(is_trusted)
    async def block(self, ctx, user: discord.User, notify: bool = False):
        if user.id in self.bot.blocked_users:
            return await ctx.send(f"{str(user)} | {user.id} is already blocked!")

        self.bot.blocked_users.append(user.id)

        await ctx.send(f"Blocked {str(user)} | {str(user.id)}")
        if notify:
            await user.send("You have been blocked from support DMs.\nPossible Reasons: ```Sending invite links\nTrolling\nSpam```")

    @commands.command()
    @commands.check(is_trusted)
    async def unblock(self, ctx, user: discord.User, notify: bool = False):
        if user.id not in self.bot.blocked_users:
            return await ctx.send(f"{str(user)} | {user.id} isn't blocked!")

        self.bot.blocked_users.remove(user.id)

        await ctx.send(f"Unblocked {str(user)} | {str(user.id)}")
        if notify:
            await user.send("You have been unblocked from support DMs.")
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.Cog.listener()
    async def on_ready(self):
        global settings_loaded
        if settings_loaded:
            await self.bot.close()

        self.bot.queue = dict()
        self.bot.playing = dict()
        self.bot.setlangs = dict()
        self.bot.channels = dict()
        self.bot.trusted = basic.remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")
        self.bot.supportserver = self.bot.get_guild(int(config["Main"]["main_server"]))
        config_channel = config["Channels"]

        for channel_name in config_channel:
            channel_id = int(config_channel[channel_name])
            channel_object = self.bot.supportserver.get_channel(channel_id)
            self.bot.channels[channel_name] = channel_object

        print(f"Starting as {self.bot.user.name}!")
        starting_message = await self.bot.channels["logs"].send(f"Starting {self.bot.user.mention}")

        # Load some files
        with open("setlangs.json") as f1, open("blocked_users.json") as f2, open("activity.txt") as f3, open("activitytype.txt") as f4, open("status.txt") as f5:
            self.bot.setlangs = json.load(f1)
            self.bot.blocked_users = json.load(f2)
            activity = f3.read()
            activitytype = f4.read()
            status = f5.read()


        activitytype1 = getattr(discord.ActivityType, activitytype)
        status1 = getattr(discord.Status, status)
        await self.bot.change_presence(status=status1, activity=discord.Activity(name=activity, type=activitytype1))

        for guild in self.bot.guilds:
            self.bot.playing[guild.id] = 0
            self.bot.queue[guild.id] = dict()

        self.avoid_file_crashes.start()

        ping = str(time.monotonic() - before).split(".")[0]
        await starting_message.edit(content=f"Started and ready! Took `{ping} seconds`")

        await self.fill_cache()

        ping = str(time.monotonic() - before).split(".")[0]
        await starting_message.edit(content=f"Started, ready, and cache filled! Took `{ping} seconds`")

    @commands.Cog.listener()
    async def on_message(self, message):
        if message.channel.id == 749971061843558440 and message.embeds and str(message.author) == "GitHub#0000":
            print("Message is from a github webhook")
            if " new commit" in message.embeds[0].title:
                print("Message is a commit")
                update_for_main = message.embeds[0].title.startswith("[Discord-TTS-Bot:master]") and self.bot.user.id == 513423712582762502
                update_for_dev = message.embeds[0].title.startswith("[Discord-TTS-Bot:dev]") and self.bot.user.id == 698218518335848538
                cog_update = message.embeds[0].title.startswith("[Common-Cogs:master]")

                print (update_for_main, update_for_dev, cog_update, "\n===============================================")

                if update_for_main or update_for_dev:
                    await self.bot.channels['logs'].send(f"Detected new bot commit! Pulling changes")
                    call(['git', 'pull'])
                    print("===============================================")
                    await self.bot.channels['logs'].send("Restarting bot...")
                    await self.end(message)

                elif cog_update:
                    await self.bot.channels['logs'].send(f"Detected new cog commit! Pulling changes")
                    call(['git', 'submodule', 'update', '--recursive', '--remote'])
                    print("===============================================")
                    await self.bot.channels['logs'].send("Reloading cog...")

                    try:
                        self.bot.reload_extension("cogs.common_user")
                        self.bot.reload_extension("cogs.common_owner")
                        self.bot.reload_extension("cogs.common_trusted")
                    except Exception as e:
                        await self.bot.channels['logs'].send(f'**`ERROR:`** {type(e).__name__} - {e}')
                    else:
                        await self.bot.channels['logs'].send('**`SUCCESS`**')

        elif message.guild is not None:
            saythis = message.clean_content.lower()

            # Get settings
            autojoin = await settings.get(message.guild, "auto_join")
            bot_ignore = await settings.get(message.guild, "bot_ignore")

            starts_with_tts = saythis.startswith("-tts")

            # if author is a bot and bot ignore is on
            if bot_ignore and message.author.bot:
                return

            # if author is not a bot, and is not in a voice channel, and doesn't start with -tts
            if not message.author.bot and message.author.voice is None and starts_with_tts is False:
                return

            # if bot **not** in voice channel and autojoin **is off**, return
            if message.guild.voice_client is None and autojoin is False:
                return

            # Check if a setup channel
            if message.channel.id != await settings.get(message.guild, "channel"):
                return

            # If message is **not** empty **or** there is an attachment
            if int(len(saythis)) != 0 or message.attachments:

                # Ignore messages starting with - that are probably commands (also advertised as a feature when it is wrong lol)
                if saythis.startswith(BOT_PREFIX) is False or starts_with_tts:

                    # This line :( | if autojoin is True **or** message starts with -tts **or** author in same voice channel as bot
                    if autojoin or starts_with_tts or message.author.bot or message.author.voice.channel == message.guild.voice_client.channel:

                        #Auto Join
                        if message.guild.voice_client is None and autojoin:
                            try:  channel = message.author.voice.channel
                            except AttributeError: return

                            self.bot.playing[message.guild.id] = 0
                            await channel.connect()

                        # Sometimes bot.guilds is wrong, because intents
                        if message.guild.id not in self.bot.queue:
                            self.bot.queue[message.guild.id] = dict()

                        # Emoji filter
                        saythis = emojitoword(saythis)

                        # Acronyms and removing -tts
                        saythis = f" {saythis} "
                        acronyms = {
                            "@": " at ",
                            "irl": "in real life",
                            "gtg": " got to go ",
                            "iirc": "if I recall correctly",
                            "™️": "tm",
                            "rn": "right now"
                        }
                        if starts_with_tts: acronyms["-tts"] = ""

                        for toreplace, replacewith in acronyms.items():
                            saythis = saythis.replace(f" {toreplace} ", f" {replacewith} ")
                        saythis = saythis[1:-1]

                        # Spoiler filter
                        saythis = re.sub(r"\|\|.*?\|\|", ". spoiler avoided.", saythis)

                        # Url filter
                        saythisbefore = saythis
                        saythis = re.sub(r"(https?:\/\/)(\s)*(www\.)?(\s)*((\w|\s)+\.)*([\w\-\s]+\/)*([\w\-]+)((\?)?[\w\s]*=\s*[\w\%&]*)*", "", str(saythis))
                        if saythisbefore != saythis:
                            saythis = saythis + ". This message contained a link"

                        # Toggleable X said and attachment detection
                        if await settings.get(message.guild, "xsaid"):
                            said_name = await settings.nickname.get(message.guild, message.author)

                            if message.attachments:
                                if len(message.clean_content.lower()) == 0:
                                    saythis = f"{said_name} sent an image."
                                else:
                                    saythis = f"{said_name} sent an image and said {saythis}"
                            else:
                                saythis = f"{said_name} said: {saythis}"

                        if basic.remove_chars(saythis, " ", "?", ".", ")", "'", '"') == "":
                            return

                        # Read language file
                        if str(message.author.id) in self.bot.setlangs:
                            lang = self.bot.setlangs[str(message.author.id)]
                        else:
                            lang = "en-us"

                        temp_store_for_mp3 = BytesIO()
                        try:  gTTS.gTTS(text=saythis, lang=lang).write_to_fp(temp_store_for_mp3)
                        except AssertionError:  return

                        # Discard if over 30 seconds
                        temp_store_for_mp3.seek(0)
                        if not (int(MP3(temp_store_for_mp3).info.length) >= 30):
                            self.bot.queue[message.guild.id][message.id] = temp_store_for_mp3
                            del temp_store_for_mp3

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
                            selected.seek(0)

                            # Play selected audio
                            vc = message.guild.voice_client
                            if vc is not None:
                                vc.play(FFmpegPCMAudio(selected.read(), pipe=True, options='-loglevel "quiet"'))

                                while vc.is_playing():  await asyncio.sleep(0.5)

                                # Delete said message from queue
                                del self.bot.queue[message.guild.id][message_id_to_read]
                            else:
                                # If not in a voice channel anymore, clear the queue
                                self.bot.queue[message.guild.id] = dict()

                        # Queue should be empty now, let next on_message though
                        self.bot.playing[message.guild.id] = 0

        elif message.author.bot is False:
            pins = await message.author.pins()
            say = False

            for pinned_message in pins:
                if pinned_message.embeds and pinned_message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!":
                    say = True

            if say:
                if message.author.id not in self.bot.blocked_users:
                    if "https://discord.gg/" in message.content.lower():
                        return await message.author.send(f"Join https://discord.gg/zWPWwQC and look in <#694127922801410119> to invite {self.bot.user.mention}!")

                    if message.attachments:
                        files = [await attachment.to_file() for attachment in message.attachments]
                    else:
                        files = None

                    webhooks = await self.bot.channels["dm_logs"].webhooks()
                    if len(webhooks) == 0:
                        webhook = await self.bot.channels["dm_logs"].create_webhook(name="TTS-DM-LOGS")
                    else:
                        webhook = webhooks[0]

                    await webhook.send(message.content, username=str(message.author), avatar_url=message.author.avatar_url, files=files)

            else:
                embed_message = cleandoc("""
                    **All messages after this will be sent to a private channel on the support server (-invite) where we can assist you.**
                    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day, repeat your message.
                    There are some basic rules if you want to get help though:
                    `1.` Ask your question, don't just ask for help
                    `2.` Don't spam, troll, or send random stuff (including server invites)
                    `3.` Many questions are answered in `-help`, try that first (also the prefix is `-`)
                """)

                embed = discord.Embed(title=f"Welcome to {self.bot.user.name} Support DMs!", description=embed_message)
                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await self.bot.channels["logs"].send(f"{str(message.author)} just got the 'Welcome to Support DMs' message")
                await dm_message.pin()

    @commands.Cog.listener()
    async def on_command_error(self, ctx, error):
        if hasattr(ctx.command, 'on_error') or isinstance(error, commands.CommandNotFound) or isinstance(error, commands.NotOwner):
            return

        if ctx.guild is not None and not ctx.guild.chunked:
            message = "**Warning:** The server you are in hasn't been fully loaded yet, this could cause issues!"

            try:  await ctx.send(message)
            except:
                try:    await ctx.author.send(message)
                except: pass

        error = getattr(error, 'original', error)

        for typed_wrong in (commands.BadArgument, commands.MissingRequiredArgument, commands.UnexpectedQuoteError, commands.ExpectedClosingQuoteError):
            if isinstance(error, typed_wrong):
                return await ctx.send(f"Did you type the command right, {ctx.author.mention}? Try doing -help!")

        for Timeout_Error in (concurrent_TimeoutError, asyncio_TimeoutError):
            if isinstance(error, Timeout_Error):
                return await ctx.send("**Timeout Error!** Do I have perms to see the channel you are in? (if yes, join https://discord.gg/zWPWwQC and ping Gnome!#6669)")

        if isinstance(error, commands.NoPrivateMessage):
            return await ctx.author.send("**Error:** This command cannot be used in private messages!")

        elif isinstance(error, commands.MissingPermissions):
            return await ctx.send(f"**Error:** You are missing {error.missing_perms} to run this command!")
        elif isinstance(error, commands.BotMissingPermissions):
            if "send_messages" in str(error.missing_perms):
                return await ctx.author.send("**Error:** I could not complete this command as I don't have send messages permissions!")

            return await ctx.send(f'**Error:** I am missing the permissions: {basic.remove_chars(error.missing_perms, "[", "]")}')
        elif isinstance(error, discord.errors.Forbidden):
            await self.bot.channels["errors"].send(f"```discord.errors.Forbidden``` in {str(ctx.guild)} caused by {str(ctx.message.content)} sent by {str(ctx.author)}")
            return await ctx.author.send("Unknown Permission Error, please give TTS Bot the required permissions. If you want this bug fixed, please do `-suggest *what command you just run*`")

        first_part = f"{str(ctx.author)} caused an error with the message: {ctx.message.clean_content}"
        second_part = ''.join(format_exception(type(error), error, error.__traceback__))
        temp = f"{first_part}\n```{second_part}```"

        if len(temp) >= 1900:
            with open("temp.txt", "w") as f:    f.write(temp)
            await self.bot.channels["errors"].send(file=discord.File("temp.txt"))
        else:
            await self.bot.channels["errors"].send(temp)

    @commands.Cog.listener()
    async def on_guild_join(self, guild):
        owner = guild.owner
        self.bot.queue[guild.id] = dict()

        await self.bot.channels["servers"].send(f"Just joined {guild.name} (owned by {str(owner)})! I am now in {str(len(self.bot.guilds))} different servers!".replace("@", "@ "))

        while not guild.chunked:   await guild.chunk(cache=True)

        try:    await owner.send(f"Hello, I am TTS Bot and I have just joined your server {guild.name}\nIf you want me to start working do -setup #textchannel and everything will work in there\nIf you want to get support for TTS Bot, join the support server!\nhttps://discord.gg/zWPWwQC")
        except:
            if guild.chunked:   pass
            else:   await self.bot.channels["logs"].send(f"Weird, `{guild.name} | {guild.id}` wasn't chunked after trying to chunk?")

        if owner.id in [member.id for member in self.bot.supportserver.members]:
            role = self.bot.supportserver.get_role(738009431052386304)
            await self.bot.supportserver.get_member(owner.id).add_roles(role)

            embed = discord.Embed(description=f"**Role Added:** {role.mention} to {owner.mention}\n**Reason:** Owner of {guild.name}")
            embed.set_author(name=f"{str(owner)} (ID {owner.id})", icon_url=owner.avatar_url)

            await self.bot.channels["logs"].send(embed=embed)


    @commands.Cog.listener()
    async def on_guild_remove(self, guild):
        settings.remove(guild)
        await self.bot.channels["servers"].send(f"Just left/got kicked from {str(guild.name)} (owned by {str(guild.owner)}). I am now in {str(len(self.bot.guilds))} servers".replace("@", "@ "))
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    async def uptime(self, ctx):
        ping = str((time.monotonic() - before) / 60).split(".")[0]
        await ctx.send(f"{self.bot.user.mention} has been up for {ping} minutes")

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

        embed=discord.Embed(title="TTS Bot Help!", url="https://discord.gg/zWPWwQC", description=cleandoc(message), color=0x3498db)
        embed.add_field(name="Universal Commands", value=cleandoc(message1), inline=False)
        embed.set_footer(text="Do you want to get support for TTS Bot or invite it to your own server? https://discord.gg/zWPWwQC")
        await ctx.send(embed=embed)

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

        embed=discord.Embed(title=f"{self.bot.user.name}: Now open source!", description=main_section, url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_footer(text=footer)
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))

        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def join(self, ctx):
        if basic.get_value(self.bot.playing, ctx.guild.id) == 3:
            return await ctx.send("Error: Already trying to join your voice channel!")

        if ctx.channel.id != await settings.get(ctx.guild, "channel"):
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

        await ctx.send("Joined your voice channel!")

    @commands.guild_only()
    @commands.bot_has_permissions(send_messages=True)
    @commands.command()
    async def leave(self, ctx):
        if ctx.channel.id != await settings.get(ctx.guild, "channel"):
            return await ctx.send("Error: Wrong channel, do -channel get the channel that has been setup.")

        if basic.get_value(self.bot.playing, ctx.guild.id) == 3:
            return await ctx.send("Error: Trying to join a voice channel!")

        elif ctx.author.voice is None:
            return await ctx.send("Error: You need to be in a voice channel to make me leave!")

        elif ctx.guild.voice_client is None:
            return await ctx.send("Error: How do I leave a voice channel if I am not in one?")

        elif ctx.author.voice.channel != ctx.guild.voice_client.channel:
            return await ctx.send("Error: You need to be in the same voice channel as me to make me leave!")

        await ctx.guild.voice_client.disconnect(force=True)

        if self.bot.playing[ctx.guild.id] is None:
            self.bot.playing[ctx.guild.id] = 0
        self.bot.playing[ctx.guild.id] = 2

        await ctx.send("Left voice channel!")

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def channel(self, ctx):
        channel = await settings.get(ctx.guild, "channel")

        if channel == ctx.channel.id:
            await ctx.send("You are in the right channel already!")
        elif channel != 0:
            await ctx.send(f"The current setup channel is: <#{channel}>")
        else:
            await ctx.send("The channel hasn't been setup, do `-setup #textchannel`")

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def tts(self, ctx, no_input = True):
        if no_input:    await ctx.send(f"You don't need to do `-tts`! {self.bot.user.mention} is made to TTS any message, and ignore messages starting with `-`!")

class Settings(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def settings(self, ctx, help = None):
        if help == "help":
            message = cleandoc("""
              -set channel `#channel`: Sets the text channel to read from
              -set xsaid `true/false`: Enable/disable "person said" before every message
              -set autojoin `true/false`: Auto joins a voice channel when a text is sent
              -set ignorebots `true/false`: Do not read other bot messages
              -set nickname `@person` `new name`: Sets your (or someone else if admin) name for xsaid.

              -set voice `language-code`: Changes your voice to a `-voices` code, equivalent to `-voice`""")
            embed=discord.Embed(title="Settings > Help", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="Available properties:", value=message, inline=False)

        else:
            channel = ctx.guild.get_channel(await settings.get(ctx.guild, "channel"))
            say = await settings.get(ctx.guild, "xsaid")
            join = await settings.get(ctx.guild, "auto_join")
            bot_ignore = await settings.get(ctx.guild, "bot_ignore")
            nickname = await settings.nickname.get(ctx.guild, ctx.author)


            if channel is None: channel = "has not been setup yet"
            else: channel = channel.name

            if str(ctx.author.id) in self.bot.setlangs: lang = self.bot.setlangs[str(ctx.author.id)]
            else: lang = "en-us"

            if nickname == ctx.author.display_name: nickname = "has not be set yet"

            # Show settings embed
            message1 = cleandoc(f"""
              :small_orange_diamond: Channel: `#{channel}`
              :small_orange_diamond: XSaid: `{say}`
              :small_orange_diamond: Auto Join: `{join}`
              :small_orange_diamond: Ignore Bots: `{bot_ignore}`""")

            message2 = cleandoc(f"""
              :small_blue_diamond:Language: `{lang}`
              :small_blue_diamond:Nickname: `{nickname}`""")

            embed=discord.Embed(title="Current Settings", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="**Server Wide**", value=message1, inline=False)
            embed.add_field(name="**User Specific**", value=message2, inline=False)

        embed.set_footer(text="Change these settings with -set property value!")
        await ctx.send(embed=embed)

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.guild_only()
    @commands.group()
    async def set(self, ctx):
        if ctx.invoked_subcommand is None:
            await ctx.send("Error: Invalid property, do `-settings help` to get a list!")

    @commands.has_permissions(administrator=True)
    @set.command()
    async def xsaid(self, ctx, value: bool):
        await settings.set(ctx.guild, "xsaid", value)
        await ctx.send(f"xsaid is now: {to_enabled[value]}")

    @commands.has_permissions(administrator=True)
    @set.command(aliases=["auto_join"])
    async def autojoin(self, ctx, value: bool):
        await settings.set(ctx.guild, "auto_join", value)
        await ctx.send(f"Auto Join is now: {to_enabled[value]}")

    @commands.has_permissions(administrator=True)
    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    async def botignore(self, ctx, value: bool):
        await settings.set(ctx.guild, "bot_ignore", value)
        await ctx.send(f"Ignoring Bots is now: {to_enabled[value]}")

    @set.command(aliases=["nick_name", "nickname", "name"])
    async def nick(self, ctx, user: Optional[discord.User] = False, *, nickname):

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
            await settings.nickname.set(ctx.guild, user, nickname)
            await ctx.send(embed=discord.Embed(title="Nickname Change", description=f"Changed {user.name}'s nickname to {nickname}"))

    @commands.has_permissions(administrator=True)
    @set.command()
    async def channel(self, ctx, channel: discord.TextChannel):
        await self.setup(ctx, channel)

    @set.command(aliases=("voice", "lang"))
    async def language(self, ctx, voicecode):
        await self.voice(ctx, voicecode)

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.has_permissions(administrator=True)
    @commands.command()
    async def setup(self, ctx, channel: discord.TextChannel):
        await settings.set(ctx.guild, "channel", channel.id)
        await ctx.send(f"Setup complete, {channel.mention} will now accept -join and -leave!")

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def voice(self, ctx, lang : str):
        langs = gTTS.lang.tts_langs(tld='co.uk')

        if lang in langs:
            self.bot.setlangs[str(ctx.author.id)] = lang.lower()
            await ctx.send(f"Changed your voice to: {self.bot.setlangs[str(ctx.author.id)]}")
        else:
            await ctx.send("Invalid voice, do -voices")

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    async def voices(self, ctx, lang: str = None):
        langs = gTTS.lang.tts_langs(tld='co.uk')

        if lang in langs:
            try:  return await self.voice(ctx, lang)
            except: pass


        if str(ctx.author.id) in self.bot.setlangs:
            lang = self.bot.setlangs[str(ctx.author.id)]
        else: lang = "en-us"

        langs_string = basic.remove_chars(list(langs.keys()), "[", "]")
        await ctx.send(f"My currently supported language codes are: \n{langs_string}\nAnd you are using: {lang}")
#//////////////////////////////////////////////////////

bot.add_cog(Main(bot))
bot.add_cog(Settings(bot))
try:    bot.run(t)
except RuntimeError: pass
