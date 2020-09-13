import asyncio
import json
import os
import re
import shutil
import time
import traceback
from typing import Union
from configparser import ConfigParser
from inspect import cleandoc
from subprocess import call

import discord
import gtts as gTTS
import natsort
from discord.ext import commands, tasks
from mutagen.mp3 import MP3

#//////////////////////////////////////////////////////
config = ConfigParser()
config.read("config.ini")
t = config["Main"]["Token"]

#//////////////////////////////////////////////////////
before = time.monotonic()
OPUS_LIBS = ['libopus-0.x86.dll', 'libopus-0.x64.dll', 'libopus-0.dll', 'libopus.so.0', 'libopus.0.dylib']

def load_opus_lib(opus_libs=OPUS_LIBS):
    if opus.is_loaded():
        return True

    for opus_lib in opus_libs:
        try:
            opus.load_opus(opus_lib)
            return
        except OSError:
            pass

        raise RuntimeError('Could not load an opus lib. Tried %s' % (', '.join(opus_libs)))

def listtostring1(s):
    str1 = ""
    for ele in s:
        str1 = f'{str1}"{ele}", '
    return str1
def emojitoword(text):
    emojiAniRegex = re.compile(r'<a\:.+:\d+>')
    emojiRegex = re.compile(r'<:.+:\d+\d+>')
    words = text.split(' ')
    output = []

    for x in words:

        if emojiAniRegex.match(x):
            output.append(f"animated emoji {x.split(':')[1]}")
        elif emojiRegex.match(x):
            output.append(f"emoji {x.split(':')[1].replace('<','')}")
        else:
            output.append(x)

    return ' '.join([str(x) for x in output])

#//////////////////////////////////////////////////////
BOT_PREFIX = "-"
bot = commands.Bot(command_prefix=BOT_PREFIX, case_insensitive=True)
bot.load_extension("cogs.common")
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

#//////////////////////////////////////////////////////
    @tasks.loop(seconds=60.0)
    async def avoid_file_crashes(self):
        try:
            with open("settings.json", "w") as f, open("setlangs.json", "w") as f1, open("blocked_users.json", "w") as f2:
                json.dump(self.bot.settings, f)
                json.dump(self.bot.setlangs, f1)
                json.dump(self.bot.blocked_users, f2)
        except Exception as e:
            error = getattr(e, 'original', e)

            temp = f"```{''.join(traceback.format_exception(type(error), error, error.__traceback__))}```"
            if len(temp) >= 1900:
                with open("temp.txt", "w") as f:  f.write(temp)
                await self.bot.channels["errors"].send(file=discord.File("temp.txt"))
            else:
                await self.bot.channels["errors"].send(temp)

    @avoid_file_crashes.before_loop
    async def before_printer(self):
        print('waiting...')
        await self.bot.wait_until_ready()

#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    @commands.is_owner()
    async def end(self, ctx):
        self.avoid_file_crashes.cancel()

        with open("settings.json", "w") as f, open("setlangs.json", "w") as f1, open("blocked_users.json", "w") as f2:
            json.dump(self.bot.settings, f)
            json.dump(self.bot.setlangs, f1)
            json.dump(self.bot.blocked_users, f2)

        await self.bot.close()

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
        ready = False
        self.bot.playing = dict()
        self.bot.settings = dict()
        self.bot.setlangs = dict()
        self.bot.channels = dict()
        self.bot.trusted = config["Main"]["trusted_ids"].replace("[", "").replace("]", "").replace("'", "").split(", ")
        self.bot.supportserver = self.bot.get_guild(int(config["Main"]["main_server"]))
        config_channel = config["Channels"]

        for channel_name in config_channel:
            channel_id = int(config_channel[channel_name])
            channel_object = self.bot.supportserver.get_channel(channel_id)
            self.bot.channels[channel_name] = channel_object

        possible_settings = {
          "channel": 0,
          "xsaid": True,
          "auto_join": False,
          "bot_ignore": True
        }

        print(f"Starting as {self.bot.user.name}!")
        starting_message = await self.bot.channels["logs"].send(f"Starting {self.bot.user.mention}")

        servers = os.listdir("servers")

        if not os.path.exists("blocked_users.json"):
            with open("blocked_users.json", "x") as f:
                json.dump(list(), f)

        # Load some files
        with open("settings.json") as f, open('setlangs.json') as f1, open("blocked_users.json") as f2, open("activity.txt") as f3, open("activitytype.txt") as f4, open("status.txt") as f5:
            self.bot.settings = json.load(f)
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
            str_guildid = str(guild.id)
            changed = False

            if str_guildid not in self.bot.settings:
                changed = True
                self.bot.settings[str_guildid] = possible_settings
            else:
                for setting, default_value in possible_settings.items():
                    if setting not in self.bot.settings[str_guildid]:
                        changed = True
                        self.bot.settings[str_guildid][setting] = default_value

            directory = f"servers/{str_guildid}"
            shutil.rmtree(directory, ignore_errors=True)
            os.mkdir(directory)

        if changed:
            with open("settings.json", "w") as f:
                json.dump(self.bot.settings, f)

        ready = True
        ping = str(time.monotonic() - before).split(".")[0]
        self.avoid_file_crashes.start()
        await starting_message.edit(content=f"Started and ready! Took `{ping} seconds`")

    @commands.Cog.listener()
    async def on_message(self, message):
        if message.embeds and message.channel.id == 749971061843558440 and str(message.author) == "GitHub#0000":
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
                        self.bot.reload_extension("cogs.common")
                    except Exception as e:
                        await self.bot.channels['logs'].send(f'**`ERROR:`** {type(e).__name__} - {e}')
                    else:
                        await self.bot.channels['logs'].send('**`SUCCESS`**')

        elif message.guild is not None:
            saythis = message.clean_content.lower()

            # return if webhook (webhooks are discord.User even though they are in a guild)
            if message.author.discriminator == "0000":
                return

            # Get autojoin setting, return if settings aren't loaded (fully or at all)
            try:    autojoin = self.bot.settings[str(message.guild.id)]["auto_join"]
            except KeyError:    return  

            # if author is a bot and bot ignore is on
            if self.bot.settings[str(message.guild.id)]["bot_ignore"] and message.author.bot:
                return

            # if author is not a bot, and is not in a voice channel, and doesn't start with -tts
            if not message.author.bot and message.author.voice is None and saythis.startswith("-tts") is False:
                return
            # if bot **not** in voice channel and autojoin **is off**, return
            if message.guild.voice_client is None and autojoin is False:
                return

            # If message is **not** empty **or** there is an attachment
            if int(len(saythis)) != 0 or message.attachments:

                # Ignore messages starting with - that are probably commands (also advertised as a feature when it is wrong lol)
                if saythis.startswith("-") is False or saythis.startswith("-tts"):

                    # This line :( | if autojoin is True **or** message starts with -tts **or** author in same voice channel as bot
                    if autojoin or saythis.startswith("-tts ") or message.author.bot or message.author.voice.channel == message.guild.voice_client.channel:

                        # Check if a setup channel
                        if message.channel.id == self.bot.settings[str(message.guild.id)]["channel"]:

                            #Auto Join
                            if message.guild.voice_client is None and autojoin:
                                try:  channel = message.author.voice.channel
                                except AttributeError: return

                                self.bot.playing[message.guild.id] = 0
                                await channel.connect()

                            # Emoji filter
                            saythis = emojitoword(saythis)

                            # Acronyms and removing -tts
                            acronyms = {
                              " @": " at ",
                              " irl": "in real life",
                              "gtg": " got to go ",
                              "iirc": "if I recall correctly",
                              "™️": " tm",
                              "-tts": "",
                            }
                            for toreplace, replacewith in acronyms.items():
                                saythis = saythis.replace(toreplace, replacewith)

                            # Spoiler filter
                            saythis = re.sub(r"\|\|.*?\|\|", ". spoiler avoided.", saythis)

                            # Url filter
                            saythisbefore = saythis
                            saythis = re.sub(r"(https?:\/\/)(\s)*(www\.)?(\s)*((\w|\s)+\.)*([\w\-\s]+\/)*([\w\-]+)((\?)?[\w\s]*=\s*[\w\%&]*)*", "", str(saythis))
                            if saythisbefore != saythis:
                                saythis = saythis + ". This message contained a link"

                            # Toggleable X said and attachment detection
                            if self.bot.settings[str(message.guild.id)]["xsaid"]:
                                if message.attachments:
                                    if len(message.clean_content.lower()) == 0:
                                        saythis = f"{message.author.display_name} sent an image."
                                    else:
                                        saythis = f"{message.author.display_name} sent an image and said {saythis}"
                                else:
                                    saythis = f"{message.author.display_name} said: {saythis}"

                            if saythis.replace(" ", "").replace("?", "").replace(".", ".") == "":
                                return

                            # Read language file
                            if str(message.author.id) in self.bot.setlangs:
                                lang = self.bot.setlangs[str(message.author.id)]
                            else:
                                lang = "en-us"

                            path = f"servers/{message.guild.id}"
                            saveto = f"{path}/{str(message.id)}.mp3"

                            try:  gTTS.gTTS(text = saythis, lang = lang, slow = False).save(saveto)
                            except AssertionError:
                                try:  os.remove(saveto)
                                except FileNotFoundError: pass
                                return

                            # Discard if over 30 seconds
                            if int(MP3(saveto).info.length) >= 30:
                                return os.remove(saveto)

                            # Queue, please don't touch this, it works somehow
                            while self.bot.playing[message.guild.id] != 0:
                                if self.bot.playing[message.guild.id] == 2: return
                                await asyncio.sleep(0.5)

                            self.bot.playing[message.guild.id] = 1

                            # Select file and play
                            while True:
                                firstmp3 = natsort.natsorted(os.listdir(path),reverse=False)[0]
                                if firstmp3.endswith(".mp3"): break
                                else: os.remove(f"{path}/{firstmp3}")

                            vc = message.guild.voice_client
                            if vc is not None:
                                vc.play(discord.FFmpegPCMAudio(f"{path}/{firstmp3}", options='-loglevel "quiet"'))

                                while vc.is_playing():
                                    await asyncio.sleep(0.5)

                            self.bot.playing[message.guild.id] = 0

                            try:  os.remove(f"{path}/{firstmp3}")
                            except FileNotFoundError: pass

        elif message.author.bot is False:
            pins = await message.author.pins()
            say = False

            for pinned_message in pins:
                if pinned_message.embeds and pinned_message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!":
                    say = True

            if say:
                if message.author.id not in self.bot.blocked_users:
                    webhook = await self.bot.channels["dm_logs"].create_webhook(name=str(message.author))

                    if message.attachments:
                        files = [await attachment.to_file() for attachment in message.attachments]
                    else: files = None

                    await webhook.send(message.content, avatar_url=message.author.avatar_url, files=files)
                    await webhook.delete()

            else:
                embed_message = cleandoc("""
                    **All messages after this will be sent to a private channel on the support server (-invite) where we can assist you.**
                    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day, repeat your message.
                    There are some basic rules if you want to get help though:
                    `1.` Ask your question, don't just ask for help
                    `2.` Don't spam, troll, or send random stuff (including server invites)
                    `3.` Many stuff is answered in `-help`, try that first (also the prefix is `-`)
                """)   

                embed = discord.Embed(title=f"Welcome to {self.bot.user.name} Support DMs!", description=embed_message)
                dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)

                await self.bot.channels["logs"].send(f"{str(message.author)} just got the 'Welcome to Support DMs' message")
                await dm_message.pin()

    @commands.Cog.listener()
    async def on_command_error(self, ctx, error):
        if hasattr(ctx.command, 'on_error'):
            return

        error = getattr(error, 'original', error)
        if isinstance(error, commands.CommandNotFound) or isinstance(error, commands.NotOwner):
            return

        elif isinstance(error, commands.BadArgument) or isinstance(error, commands.MissingRequiredArgument) or isinstance(error, commands.UnexpectedQuoteError) or isinstance(error, commands.ExpectedClosingQuoteError):
            return await ctx.send(f"Did you type the command right, {ctx.author.mention}? Try doing -help!")
        elif isinstance(error, commands.MissingPermissions):
            if not ctx.guild or ctx.author.discriminator == "0000":
                return await ctx.send("**Error:** This command cannot be used in DMs!")

            return await ctx.send(f"You are missing {error.missing_perms} to run this command.")
        elif isinstance(error, commands.BotMissingPermissions):
            if "send_messages" in str(error.missing_perms):
                return await ctx.author.send("Sorry I could not complete this command as I don't have send messages permissions.")
            else:
                return await ctx.send(f'I am missing the permissions: {str(error.missing_perms).replace("[", "").replace("]", "")}')
        elif isinstance(error, commands.NoPrivateMessage):
            return await ctx.author.send("**Error:** This command cannot be used in private messages.")

        first_part = f"{str(ctx.author)} caused an error with the message: {ctx.message.clean_content}"
        second_part = ''.join(traceback.format_exception(type(error), error, error.__traceback__))
        temp = f"{first_part}\n```{second_part}```"

        for oh_no_oh_fuck in ["concurrent.futures._base.TimeoutError", "asyncio.exceptions.TimeoutError"]:
            if oh_no_oh_fuck in temp:
                await self.bot .send(f"**Timeout Error!** caused by {str(ctx.author)}: Message = '{ctx.message.content}' Guild = {ctx.guild.name} | {ctx.guild.id}")
                return await ctx.send("**Timeout Error!** Do I have perms to see the channel you are in? (if yes, join https://discord.gg/zWPWwQC and ping Gnome!#6669)")

        if "discord.errors.Forbidden" in temp:
            await self.bot.channels["errors"].send(f"```discord.errors.Forbidden``` in {str(ctx.guild)} caused by {str(ctx.message.content)} sent by {str(ctx.author)}")
            return await ctx.author.send("Unknown Permission Error, please give TTS Bot the required permissions. If you want this bug fixed, please do `-suggest *what command you just run*`")

        if len(temp) >= 1900:
            with open("temp.txt", "w") as f:
                f.write(temp)
            await self.bot.channels["errors"].send(file=discord.File("temp.txt"))
        else:
            await self.bot.channels["errors"].send(temp)

    @commands.Cog.listener()
    async def on_guild_join(self, guild):
        role = self.bot.supportserver.get_role(738009431052386304)
        owner = guild.owner
        mypath = f"servers/{str(guild.id)}"

        if os.path.exists(mypath) is False:
            os.mkdir(mypath)

        if owner.id in [member.id for member in self.bot.supportserver.members]:
            await self.bot.supportserver.get_member(owner.id).add_roles(role)

            embed = discord.Embed(description=f"*Role Added:** {role.mention} to {owner.mention}\n**Reason:** Owner of {guild.name}")
            embed.set_author(name=f"{str(owner)} (ID {owner.id})", icon_url=owner.avatar_url)

            await self.bot.channels["servers"].send(embed=embed)

        self.bot.settings[str(guild.id)] = {
          "channel": 0,
          "xsaid": True,
          "auto_join": False,
          "bot_ignore": True
        }

        await self.bot.channels["servers"].send(f"Just joined {guild.name} (owned by {str(owner)})! I am now in {str(len(self.bot.guilds))} different servers!".replace("@", "@ "))
        await owner.send(f"Hello, I am TTS Bot and I have just joined your server {guild.name}\nIf you want me to start working do -setup #textchannel and everything will work in there\nIf you want to get support for TTS Bot, join the support server!\nhttps://discord.gg/zWPWwQC")


    @commands.Cog.listener()
    async def on_guild_remove(self, guild):
        self.bot.settings.pop(str(guild.id), None)
        await self.bot.channels["servers"].send(f"Just left/got kicked from {str(guild.name)} (owned by {str(guild.owner)}). I am now in {str(len(self.bot.guilds))} servers".replace("@", "@ "))
#//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    @commands.command()
    async def uptime(self, ctx):
        ping = str((time.monotonic() - before) / 60).split(".")[0]
        await ctx.send(f"{self.bot.user.mention} has been up for {ping} minutes")

    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command(aliases=["commands"])
    async def help(self, ctx):
        message = f"""
          `-setup #textchannel`: Setup the bot to read messages from that channel

          `-join`: Joins the voice channel you're in
          `-leave`: Leaves voice channel

          `-settings`: Display the current settings
          `-settings help`: Displays list of available settings
          `-set property value`: Sets a setting
          """
        message1 = f"""
          `-help`: Shows this message
          `-botstats`: Shows various different stats 
          `-donate`: Help improve TTS Bot's development and hosting through Patreon
          `-suggest *suggestion*`: Suggests a new feature! (could also DM TTS Bot)
          `-invite`: Sends the instructions to invite TTS Bot!"""

        embed=discord.Embed(title="TTS Bot Help!", url="https://discord.gg/zWPWwQC", description=cleandoc(message), color=0x3498db)
        embed.add_field(name="Universal Commands", value=cleandoc(message1), inline=False)
        embed.set_footer(text=f"Do you want to get support for TTS Bot or invite it to your own server? https://discord.gg/zWPWwQC")
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

        embed=discord.Embed(title="TTS Bot Info", url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_thumbnail(url="https://cdn.discordapp.com/avatars/513423712582762502/760ae3b79b2ca0fcd91dc9d89c6984c5.png")
        
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
        
        embed.add_field(name=f"**{self.bot.user.name}: Now open source!**", value=main_section, inline=False)
        embed.set_footer(text=footer)

        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.bot_has_guild_permissions(speak=True, connect=True, use_voice_activation=True)
    @commands.command()
    async def join(self, ctx):
        if ctx.channel.id != self.bot.settings[str(ctx.guild.id)]["channel"]:
            return await ctx.send("Error: Wrong channel, do -channel get the channel that has been setup.")

        if ctx.author.voice is None:
            return await ctx.send("Error: You need to be in a voice channel to make me join your voice channel!")

        channel = ctx.author.voice.channel
        permissions = channel.permissions_for(ctx.guild.me)

        if not permissions.view_channel:
            return await ctx.send("Error: Missing Permission to view your voice channel!")

        if not permissions.speak:
            return await ctx.send("Error: I do not have permssion to speak!")

        if ctx.guild.voice_client is not None and ctx.guild.voice_client == channel:
            return await ctx.send("Error: I am already in your voice channel!")

        if ctx.guild.voice_client is not None and ctx.guild.voice_client != channel:
            return await ctx.send("Error: I am already in a voice channel!")

        self.bot.playing[ctx.guild.id] = 0

        await channel.connect()
        await ctx.send("Joined your voice channel!")

    @commands.guild_only()
    @commands.bot_has_permissions(send_messages=True)
    @commands.command()
    async def leave(self, ctx):
        if ctx.channel.id != self.bot.settings[str(ctx.guild.id)]["channel"]:
            return await ctx.send("Error: Wrong channel, do -channel get the channel that has been setup.")

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

        directory = f"servers/{str(ctx.guild.id)}"
        shutil.rmtree(directory, ignore_errors=True)
        os.mkdir(directory)

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def channel(self, ctx):
        channel = self.bot.settings[str(ctx.guild.id)]["channel"]

        if channel == ctx.channel.id:
            await ctx.send("You are in the right channel already!")
        elif channel != 0:
            await ctx.send(f"The current setup channel is: <#{channel}>")
        else:
            await ctx.send("The channel hasn't been setup, do `-setup #textchannel`")

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def tts(self, ctx, totts = None):
        if totts is None:
            await ctx.send(f"You don't need to do `-tts`! {self.bot.user.mention} is made to TTS any message, and ignore messages starting with `-`!")

class Settings(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.index = 0
        self._last_member = None

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def settings(self, ctx, help = None):
        if help == "help":
            message = cleandoc("""
              -set channel `#channel`: Sets the text channel to read from
              -set xsaid `true/false`: Enable/disable "person said" before every message
              -set autojoin `true/false`: Auto joins a voice channel when a text is sent
              -set ignorebots `true/false`: Do not read other bot messages""")
            embed=discord.Embed(title="Settings > Help", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="Available properties:", value=message, inline=False)

        else:
            channel = ctx.guild.get_channel(self.bot.settings[str(ctx.guild.id)]["channel"])
            say = self.bot.settings[str(ctx.guild.id)]["xsaid"]
            join = self.bot.settings[str(ctx.guild.id)]["auto_join"]
            bot_ignore = self.bot.settings[str(ctx.guild.id)]["bot_ignore"]

            if channel is None: channel = "has not been setup yet"
            else: channel = channel.name

            if str(ctx.author.id) in self.bot.setlangs: lang = self.bot.setlangs[str(ctx.author.id)]
            else: lang = "en-us"

            # Show settings embed
            message1 = cleandoc(f"""
              :small_orange_diamond: Channel: `#{channel}`
              :small_orange_diamond: XSaid: `{say}`
              :small_orange_diamond: Auto Join: `{join}`
              :small_orange_diamond: Ignore Bots: `{bot_ignore}`""")

            message2 = f":small_blue_diamond:Language: `{lang}`"

            embed=discord.Embed(title="Current Settings", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="**Server Wide**", value=message1, inline=False)
            embed.add_field(name="**User Specific**", value=message2, inline=False)

        embed.set_footer(text="Change these settings with -set property value!")
        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.command()
    async def set(self, ctx, key, value):
        if not ctx.guild or ctx.author.discriminator == "0000":
            return await ctx.send("**Error:** This command cannot be used in DMs!")

        key = key.lower()
        needs_admin_to_edit = ("xsaid", "auto_join", "autojoin", "botignore", "bot_ignore", "ignore_bots", "ignorebots")
        no_admin_needed = ("channel", "language")

        to_bool = {
          0: False,
          1: True,
          "0": False,
          "1": True,
          "false": False,
          "true": True,
          False: False,
          True: True,
        }
        to_enabled = {
          True: "Enabled",
          False: "Disabled"
        }

        if key in needs_admin_to_edit:
            if not ctx.author.guild_permissions.administrator and not await self.bot.is_owner(ctx.author):
                return await ctx.send(f"Error: You need administrator permission to edit {key}!")

            if key == "xsaid":
                try:
                    value = to_bool[value.lower()]
                except:
                    return await ctx.send("Error: Invalid value")
                self.bot.settings[str(ctx.guild.id)]["xsaid"] = value
                return await ctx.send(f"xsaid is now: {to_enabled[value]}")

            elif key in ("auto_join", "autojoin"):
                try:
                    value = to_bool[value.lower()]
                except:
                    return await ctx.send("Error: Invalid value")
                self.bot.settings[str(ctx.guild.id)]["auto_join"] = value
                return await ctx.send(f"Auto Join is now: {to_enabled[value]}")

            elif key in ("botignore", "bot_ignore", "ignore_bots", "ignorebots"):
                try:
                    value = to_bool[value.lower()]
                except:
                    return await ctx.send("Error: Invalid value")
                self.bot.settings[str(ctx.guild.id)]["bot_ignore"] = value
                return await ctx.send(f"Ignoring Bots is now: {to_enabled[value]}")

        elif key in no_admin_needed:
            if key == "channel":
                value = await commands.TextChannelConverter().convert(ctx, value)
                await self.setup(ctx, value)

            else:
                await self.voice(ctx, value)

        else:
            await ctx.send("Error: Invalid property, do `-settings` to get a list!")

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.has_permissions(administrator=True)
    @commands.command()
    async def setup(self, ctx, channel: discord.TextChannel):
        self.bot.settings[str(ctx.guild.id)]["channel"] = channel.id
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

        await ctx.send(f"My currently supported language codes are: \n{listtostring1(langs)}\nAnd you are using: {lang}")
#//////////////////////////////////////////////////////

bot.add_cog(Main(bot))
bot.add_cog(Settings(bot))
bot.run(t)
