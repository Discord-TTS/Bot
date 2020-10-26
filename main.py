import asyncio # imports the asyncio module
import json # imports the json module
import re # imports the re module
import shutil # imports the shutil module
from asyncio.exceptions import TimeoutError as asyncio_TimeoutError # imports the TimeoutError module from asyncio.exceptions, as asyncio_TimeoutError
from concurrent.futures._base import TimeoutError as concurrent_TimeoutError # imports the TimeoutError module from concurrent.futures.base as concurrent_TimeoutError
from configparser import ConfigParser # imports the ConfigParser module from configparser
from inspect import cleandoc # imports the cleandoc module from inspect
from io import BytesIO # imports the BytesIO module from io
from os import remove # imports the remove module from os
from os.path import exists # imports the exists module from os.path
from subprocess import call # imports the call module from subprocess
from sys import exc_info # imports the exc_info module from sys
from time import monotonic # imports the monotonic module from time
from traceback import format_exception # imports the format_exception module from traceback
from typing import Optional, Union # imports the Optional and Union modules from typing
 # a blank line
import discord # imports the discord module
import gtts as gTTS # imports the gtts module as gTTS
from discord.ext import commands, tasks # imports the commands and tasks modules from discord.ext
from mutagen.mp3 import MP3 # imports the MP3 module from mutagen.mp3
 # a blank line
from patched_FFmpegPCM import FFmpegPCMAudio # imports the FFmpegPCMAudio module from patched_FFmpegPCM
from utils import basic # imports the basic module from utils
from utils.settings import blocked_users_class as blocked_users # imports the blocked_users_class module from utils.settings as blocked_users
from utils.settings import setlangs_class as setlangs # imports the setlangs_class module from utils.settings as setlangs
from utils.settings import settings_class as settings # imports the settings_class module from utils.settings as settings
 # a blank line
#//////////////////////////////////////////////////////
config = ConfigParser() # creates an alias for the ConfigParser() function as config
config.read("config.ini") # uses the ConfigParser() function to read the config.ini file
t = config["Main"]["Token"] # sets the "t" variable to the Main and Token variables inside config.ini
config_channels = config["Channels"] # sets the config_channels variable to the Channels variable inside config.ini
 # a blank line
# Define random variables
BOT_PREFIX = "-" # sets the bot's prefix to "-"
before = monotonic() # creates an alias for the monotonic() function as before
NoneType = type(None) # sets the NoneType variable to type(None)
settings_loaded = False # sets a variable to indicate the settings haven't been loaded
tts_langs = gTTS.lang.tts_langs(tld='co.uk') # sets the tts_langs variable to the output of gTTS.lang.tts_langs, with the option "tld='co.uk'"
to_enabled = {True: "Enabled", False: "Disabled"} # defines a function to replace boolean values True and False with Enabled and Disabled
 # a blank line
if exists("activity.txt"): # checks for the existence of the activity.txt file
    with open("activity.txt") as f2, open("activitytype.txt") as f3, open("status.txt") as f4: # if it does exist, set the f2 variable to it, the f3 variable to activitytype.txt, and the f4 variable to status.txt
        activity = f2.read() # sets the activity variable to the contents of f2
        activitytype = f3.read() # sets the activitytype variable to the contents of f3
        status = f4.read() # sets the status variable to the contents of f4
 # a blank line
    config["Activity"] = {"name": activity, "type": activitytype, "status": status} # sets up Activity within Config to contain the value of activity as "name", the value of activitytype as "type", and the value of status as "status".
 # a blank line
    with open("config.ini", "w") as configfile: config.write(configfile) # writes the config settings to the config.ini file
    remove("activitytype.txt") # deletes the activitytype.txt file
    remove("activity.txt") # deletes the activity.txt file
    remove("status.txt") # deletes the status.txt file
 # a blank line
async def require_chunk(ctx): # defines the require_chunk function, with a ctx parameter
    if ctx.guild and not ctx.guild.chunked: # checks if a guild is being used and if it isn't chunked
        try:    chunk_guilds.start() # if the check passes, attempts to chunk the guild
        except RuntimeError: pass # gives up if it doesn't work
 # a blank line
        if ctx.guild.id not in bot.chunk_queue: # checks if the guild id isn't queued to be chunked
            bot.chunk_queue.append(ctx.guild.id) # adds the guild id to the queue of servers to be chunked
 # a blank line
    return True # returns True
 # a blank line
@tasks.loop(seconds=1) # uhhh i don't know what @ means
async def chunk_guilds(): # defines the chunk_guilds function
    chunk_queue = bot.chunk_queue # sets the chunk_queue variable to the contents of bot.chunk_queue
 # a blank line
    for guild in chunk_queue: # runs the following for every guild in the chunk queue
        guild = bot.get_guild(guild) # sets the guild variable to the guild currently being processed
 # a blank line
        if not guild.chunked: # runs the following if the guild isn't chunked
            await guild.chunk(cache=True) # waits for the guild to be chunked
            await last_cached_message.edit(content=f"Just chunked: {guild.name} | {guild.id}") # edits the last log message to mention the guild it just chunked
 # a blank line
        bot.chunk_queue.remove(guild.id) # removes the now-chunked guild from the chunk queue
 # a blank line
# Define bot and remove overwritten commands # adds a comment to explain what this code does
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"])) # sets the activity variable to a combination of the discord activity variable, the attribute of its activitytype, and the Activity and type variables from the config
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True) # sets the intents variable to the output of the discord.Intents function, where voice_states is True, messages is True, guilds is True, and members is True
status = getattr(discord.Status, config["Activity"]["status"]) # sets the status variable to a combination of the discord status variable, and the Activity and status options in config
 # a blank line
bot = commands.AutoShardedBot( # adds the following to the bot variable
    status=status, # sets status to status
    intents=intents, # sets intents to intents
    activity=activity, # sets activity to activity
    case_insensitive=True, # sets case_insensitive to True
    command_prefix=BOT_PREFIX, # sets command_prefix to the previously defined bot prefix
    chunk_guilds_at_startup=False, # sets chunk_guilds_at_startup to False
) # ends the loop
 # a blank line
bot.queue = dict() # sets the type of bot.queue to a dict
bot.playing = dict() # sets the type of bot.playing to a dict
bot.channels = dict() # sets the type of bot.channels to a dict
bot.chunk_queue = list() # sets the type of bot.chunk_queue to a list
bot.trusted = basic.remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ") # populates the list of trusted users
 # a blank line
if exists("cogs/common_user.py"): # checks for the existence of the common_user.py file in the cogs folder
    bot.load_extension("cogs.common_owner") # loads the extension cogs.common_owner
    bot.load_extension("cogs.common_trusted") # loads the extension cogs.common_trusted
    bot.load_extension("cogs.common_user") # loads the extension cogs.common_user
elif exists("cogs/common.py"): # if the previous if statement failed, checks for the common.py file in the cogs folder
    bot.load_extension("cogs.common") # loads the extension cogs.common
else: # if none of the two statements passed
    print("Error: Cannot find cogs to load? Did you do 'git clone --recurse-submodules'?") # prints an error message
    raise SystemExit # exits the bot
 # a blank line
for overwriten_command in ("help", "end", "botstats"): # checks for duplicate help, end and botstats commands
    bot.remove_command(overwriten_command) # removes duplicate commands
#////////////////////////////////////////////////////// # a comment to separate parts of the code
class Main(commands.Cog): # defines the Main class
    def __init__(self, bot): # defines the __init__ function of the Main class
        self.bot = bot # sets the self.bot variable to the bot variable
 # a blank line
    def cog_unload(self): # defines the cog_unload function of the Main class
        self.avoid_file_crashes.cancel() # runs the self.avoid_file_crashes.cancel() function
 # a blank line
    def is_trusted(ctx): # defines the is_trusted function of the Main class
        if str(ctx.author.id) in bot.trusted: return True # if the contents of ctx.author.id are contained in bot.trusted, return True
        else: raise commands.errors.NotOwner # if not, print an error
 # a blank line
    @tasks.loop(seconds=60.0) # what the fuck does @ mean
    async def avoid_file_crashes(self): # defines avoid_file_crashes
        try: # attempts the following
            settings.save() # saves settings
            setlangs.save() # saves setlangs
            blocked_users.save() # saves blocked_users
        except Exception as e: # if an error happens
            error = getattr(e, 'original', e) # sets the error variable to the error that occurred
 # a blank line
            temp = f"```{''.join(format_exception(type(error), error, error.__traceback__))}```" # sets the temp variable to the error that happened
            if len(temp) >= 1900: # checks if the length of the temp variable is more than 1900
                with open("temp.txt", "w") as f:  f.write(temp) # writes the temp variable to the file temp.txt
                await self.bot.channels["errors"].send(file=discord.File("temp.txt")) # sends the temp.txt file to the errors channel
            else: # if the previous if statement was false
                await self.bot.channels["errors"].send(temp) # sends the contents of the temp variable to the errors channel
 # a blank line
    @avoid_file_crashes.before_loop # what the fuck does @ mean
    async def before_file_saving_loop(self): # defines the before_file_saving_loop function
        await self.bot.wait_until_ready() # waits until the bot is ready
 # a blank line
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command() # what the fuck does @ mean
    @commands.is_owner() # what the fuck does @ mean
    async def end(self, ctx): # defines the end command
        self.avoid_file_crashes.cancel() # runs the avoid_file_crashes.cancel function
        settings.save() # saves settings
        setlangs.save() # saves setlangs
        blocked_users.save() # saves blocked users

        await self.bot.close() # waits for the bot to close

    @commands.command() # what the fuck does @ mean
    @commands.is_owner() # what the fuck does @ mean
    async def leave_unused_guilds(self, ctx, sure: bool = False): # defines the leave_unused_guilds command
        guilds_to_leave = [] # creates the guilds_to_leave variable
        with open("settings.json") as f: # sets the f variable to the contents of the file settings.json
            temp_settings = f.read() # sets the temp_settings variable to the contents of the f variable

        for guild in self.bot.guilds: # runs this code for every guild the bot is in
            guild_id = str(guild.id) # sets guild_id to the current guild id
            if guild_id not in temp_settings: # checks if the guild is not in temp_settings
                guilds_to_leave.append(guild) # adds the guild id to the list of guilds to leave

        if not sure: # checks if sure is false
            await ctx.send(f"Are you sure you want me to leave {len(guilds_to_leave)} guilds?") # asks if the user is sure they want the bot to leave guilds
        else: # if the above if statement isn't true
            for guild in guilds_to_leave: # runs the following for every guild in guilds_to_leave
                try:    await guild.owner.send("Hey! TTS Bot has not been setup on your server so I have left! If you want to reinvite me, join https://discord.gg/zWPWwQC and look in #invites-and-rules.") # tries to dm the owner of a guild being left about leaving the guild
                except: pass # if the above fails, give up
                await guild.leave() # waits until the guild has been left

            await self.bot.channels["logs"].send(f"Just left {len(guilds_to_leave)} guilds due to no setup, requested by {ctx.author.name}") # adds a log message that a guild was left

    @commands.command() # what the fuck does @ mean
    @commands.is_owner() # what the fuck does @ mean
    async def channellist(self, ctx): # defines the channellist command
        channellist = str() # defines the channellist variable as a string
        for guild1 in self.bot.guilds: # runs the following for all guilds the bot is in
            try:  channellist = f"{channellist} \n{str(guild1.voice_client.channel)} in {guild1.name}" # attempts to find a connected voice channel in the current guild
            except: pass # if the above fails, give up

        tempplaying = dict() # defines the tempplaying variable as a dict
        for key in self.bot.playing: # runs the following for every instance of key in self.bot.playing
            if self.bot.playing[key] != 0: # checks if self.bot.playing is a non-zero value
                tempplaying[key] = self.bot.playing[key] # sets the tempplaying variable to the contents of self.bot.playing
        await ctx.send(f"TTS Bot Voice Channels:\n{channellist}\nAnd just incase {str(tempplaying)}") # sends the list of channels the bot is in

    @commands.command() # what the fuck does @ mean
    @commands.check(is_trusted) # what the fuck does @ mean
    async def save_files(self, ctx): # defines the save_files command
        settings.save() # saves settings
        setlangs.save() # saves setlangs
        blocked_users.save() # saves blocked_users
        await ctx.send("Saved all files!") # sends a message indicating that files were saved

    @commands.command() # what the fuck does @ mean
    @commands.check(is_trusted) # what the fuck does @ mean
    async def cleanup(self, ctx): # defines the cleanup command
        guild_id_list = [str(guild.id) for guild in self.bot.guilds] # sets the guild_id_list variable to a list of guilds the bot is in

        user_id_list = list() # defines the user_id_list variable as a list
        [[user_id_list.append(str(member.id)) for member in guild.members] for guild in bot.guilds] # appends a member's user id to the user_id_list for every member in every server the bot is in

        settings.cleanup(guild_id_list) # performs the settings.cleanup function
        setlangs.cleanup(user_id_list) # performs the setlands.cleanup function

        if exists("servers"): # checks if the servers folder exists
            shutil.rmtree("servers", ignore_errors=True) # if it does, delete it and ignore errors

        await ctx.send("Done!") # send "Done!"
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.Cog.listener()
    async def on_ready(self):
        global last_cached_message
        self.bot.supportserver = self.bot.get_guild(int(config["Main"]["main_server"]))

        try:
            await last_cached_message.edit(f"~~{last_cached_message.content}~~")
            await starting_message.edit(content=f"~~{starting_message.content}~~")
            starting_message = await self.bot.channels["logs"].send(f"Restarting as {self.bot.user.name}!")
            print(f":wagu: Restarting as {self.bot.user.name}!")

        except NameError:
            print(f"Starting as {self.bot.user.name}")

            try:    self.avoid_file_crashes.start()
            except RuntimeError:    pass

            for channel_name in config_channels:
                channel_id = int(config_channels[channel_name])
                channel_object = self.bot.supportserver.get_channel(channel_id)
                self.bot.channels[channel_name] = channel_object

            starting_message = await self.bot.channels["logs"].send(f"Starting as {self.bot.user.name}!")

        for guild in self.bot.guilds:
            self.bot.playing[guild.id] = 0
            self.bot.queue[guild.id] = dict()

        await starting_message.edit(content=f"Started and ready! Took `{int(monotonic() - before)} seconds`")
        last_cached_message = await self.bot.channels["logs"].send("Waiting to chunk a guild!")

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
            autojoin = settings.get(message.guild, "auto_join")
            bot_ignore = settings.get(message.guild, "bot_ignore")

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
            if message.channel.id != settings.get(message.guild, "channel"):
                return

            # If message is **not** empty **or** there is an attachment
            if int(len(saythis)) != 0 or message.attachments:

                # Ignore messages starting with - that are probably commands (also advertised as a feature when it is wrong lol)
                if saythis.startswith(BOT_PREFIX) is False or starts_with_tts:

                    # This line :( | if autojoin is True **or** message starts with -tts **or** author in same voice channel as bot
                    if autojoin or starts_with_tts or message.author.bot or message.author.voice.channel == message.guild.voice_client.channel:

                        # Fixing playing value if not loaded
                        if basic.get_value(self.bot.playing, message.guild.id) is None:
                            self.bot.playing[message.guild.id] = 0

                        # Auto Join
                        if message.guild.voice_client is None and autojoin and self.bot.playing[message.guild.id] in (0, 1):
                            try:  channel = message.author.voice.channel
                            except AttributeError: return

                            self.bot.playing[message.guild.id] = 3
                            await channel.connect()
                            self.bot.playing[message.guild.id] = 0

                        # Sometimes bot.guilds is wrong, because intents
                        if message.guild.id not in self.bot.queue:
                            self.bot.queue[message.guild.id] = dict()

                        # Emoji filter
                        saythis = basic.emojitoword(saythis)

                        # Acronyms and removing -tts
                        saythis = f" {saythis} "
                        acronyms = {
                            "@": " at ",
                            "irl": "in real life",
                            "gtg": " got to go ",
                            "iirc": "if I recall correctly",
                            "™️": "tm",
                            "rn": "right now",
                            "wdym": "what do you mean",
                            "imo": "in my opinion",
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
                        changed = False
                        for word in saythis.split(" "):
                            if word.startswith("https://") or word.startswith("http://") or word.startswith("www."):
                                saythis = saythis.replace(word, "")
                                changed = True

                        if changed:
                            saythis += ". This message contained a link"

                        # Toggleable X said and attachment detection
                        xsaid = settings.get(message.guild, "xsaid")
                        if xsaid:
                            try:
                                last_message = await message.channel.history(limit=2).flatten()
                                last_message = last_message[1]
                                if message.author.id == last_message.author.id: xsaid = False
                            except discord.errors.Forbidden: pass

                        if xsaid:
                            said_name = settings.nickname.get(message.guild, message.author)
                            format = basic.exts_to_format(message.attachments)

                            if message.attachments:
                                if len(saythis) == 0:
                                    saythis = f"{said_name} sent {format}."
                                else:
                                    saythis = f"{said_name} sent {format} and said {saythis}"
                            else:
                                saythis = f"{said_name} said: {saythis}"

                        if basic.remove_chars(saythis, " ", "?", ".", ")", "'", '"') == "":
                            return

                        # Read language file
                        lang = setlangs.get(message.author)

                        temp_store_for_mp3 = BytesIO()
                        try:  gTTS.gTTS(text=saythis, lang=lang).write_to_fp(temp_store_for_mp3)
                        except AssertionError:  return
                        except (gTTS.tts.gTTSError, ValueError):
                            try:    return await message.add_reaction("🚫")
                            except  discord.errors.Forbidden: return

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
                                try:    vc.play(FFmpegPCMAudio(selected.read(), pipe=True, options='-loglevel "quiet"'))
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

                elif not blocked_users.check(message.author):
                    files = [await attachment.to_file() for attachment in message.attachments]
                    webhook = await basic.ensure_webhook(self.bot.channels["dm_logs"], name="TTS-DM-LOGS")

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
    async def on_voice_state_update(self, member, before, after):
        guild = member.guild
        vc = guild.voice_client
        playing = basic.get_value(self.bot.playing, guild.id)

        if member.id == self.bot.user.id:   return # someone other than bot left vc
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

        if event == "on_message":
            if args[0].author.id == bot.user.id:    return

            message = await args[0].channel.fetch_message(args[0].id)
            if isinstance(errors[1], discord.errors.Forbidden):
                try:    return await message.author.send("Unknown Permission Error, please give TTS Bot the required permissions!")
                except discord.errors.Forbidden:    return

            part1 = f"""{message.author} caused an error with the message: {message.content}"""

        try:    error_message = f"{part1}\n```{''.join(format_exception(errors[0], errors[1], errors[2]))}```"
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

        if len(temp) >= 1900:
            with open("temp.txt", "w") as f:    f.write(temp)
            await self.bot.channels["errors"].send(file=discord.File("temp.txt"))
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
        settings.remove(guild)

        if guild.id in self.bot.queue:  self.bot.queue.pop(guild.id, None)
        if guild.id in self.bot.playing:  self.bot.playing.pop(guild.id, None)
        await self.bot.channels["servers"].send(f"Just left/got kicked from {str(guild.name)}. I am now in {str(len(self.bot.guilds))} servers".replace("@", "@ "))
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    async def uptime(self, ctx):
        await ctx.send(f"{self.bot.user.mention} has been up for {int((monotonic() - before) // 60)} minutes")

    @commands.command()
    async def debug(self, ctx):
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

        embed=discord.Embed(title="TTS Bot Help!", url="https://discord.gg/zWPWwQC", description=cleandoc(message), color=0x3498db)
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

        embed=discord.Embed(title=f"{self.bot.user.name}: Now open source!", description=main_section, url="https://discord.gg/zWPWwQC", color=0x3498db)
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

        if ctx.channel.id != settings.get(ctx.guild, "channel"):
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
    @commands.check(require_chunk)
    @commands.bot_has_permissions(send_messages=True)
    @commands.command()
    async def leave(self, ctx):
        if basic.get_value(self.bot.playing, ctx.guild.id) == 2:
            return await ctx.send("Error: Already trying to leave your voice channel!")

        if ctx.channel.id != settings.get(ctx.guild, "channel"):
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
        channel = settings.get(ctx.guild, "channel")

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
        if ctx.message.content != f"{BOT_PREFIX}tts":
            await ctx.send(f"You don't need to do `-tts`! {self.bot.user.mention} is made to TTS any message, and ignore messages starting with `-`!")

class Settings(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.guild_only()
    @commands.check(require_chunk)
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
            channel = ctx.guild.get_channel(settings.get(ctx.guild, "channel"))
            say = settings.get(ctx.guild, "xsaid")
            join = settings.get(ctx.guild, "auto_join")
            bot_ignore = settings.get(ctx.guild, "bot_ignore")
            nickname = settings.nickname.get(ctx.guild, ctx.author)


            if channel is None: channel = "has not been setup yet"
            else: channel = channel.name

            lang = setlangs.get(ctx.author)

            if nickname == ctx.author.display_name: nickname = "has not been set yet"

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

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.group()
    async def set(self, ctx):
        if ctx.invoked_subcommand is None:
            await ctx.send("Error: Invalid property, do `-settings help` to get a list!")

    @commands.has_permissions(administrator=True)
    @set.command()
    async def xsaid(self, ctx, value: bool):
        settings.set(ctx.guild, "xsaid", value)
        await ctx.send(f"xsaid is now: {to_enabled[value]}")

    @commands.has_permissions(administrator=True)
    @set.command(aliases=["auto_join"])
    async def autojoin(self, ctx, value: bool):
        settings.set(ctx.guild, "auto_join", value)
        await ctx.send(f"Auto Join is now: {to_enabled[value]}")

    @commands.has_permissions(administrator=True)
    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    async def botignore(self, ctx, value: bool):
        settings.set(ctx.guild, "bot_ignore", value)
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
            settings.nickname.set(ctx.guild, user, nickname)
            await ctx.send(embed=discord.Embed(title="Nickname Change", description=f"Changed {user.name}'s nickname to {nickname}"))

    @commands.has_permissions(administrator=True)
    @set.command()
    async def channel(self, ctx, channel: discord.TextChannel):
        await self.setup(ctx, channel)

    @set.command(aliases=("voice", "lang"))
    async def language(self, ctx, voicecode):
        await self.voice(ctx, voicecode)

    @commands.guild_only()
    @commands.check(require_chunk)
    @commands.has_permissions(administrator=True)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def setup(self, ctx, channel: discord.TextChannel):
        settings.set(ctx.guild, "channel", channel.id)
        await ctx.send(f"Setup complete, {channel.mention} will now accept -join and -leave!")

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def voice(self, ctx, lang: str):
        if lang in tts_langs:
            setlangs.set(ctx.author, lang)
            await ctx.send(f"Changed your voice to: {tts_langs[setlangs.get(ctx.author)]}")
        else:
            await ctx.send("Invalid voice, do -voices")

    @commands.check(require_chunk)
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    async def voices(self, ctx, lang: str = None):
        if lang in tts_langs:
            try:  return await self.voice(ctx, lang)
            except: return

        lang = setlangs.get(ctx.author)
        langs_string = basic.remove_chars(list(tts_langs.keys()), "[", "]")

        await ctx.send(f"My currently supported language codes are: \n{langs_string}\nAnd you are using: {tts_langs[lang]} | {lang}")
#//////////////////////////////////////////////////////

bot.add_cog(Main(bot))
bot.add_cog(Settings(bot))
try:    bot.run(t)
except RuntimeError: pass
