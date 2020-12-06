import asyncio
import re
from configparser import ConfigParser
from os import listdir
from random import choice as pick_random
from time import monotonic

import asyncpg
import discord
from cryptography.fernet import Fernet
from discord.ext import commands

from patched_FFmpegPCM import FFmpegPCMAudio
from utils import basic, cache, settings

#//////////////////////////////////////////////////////
start_time = monotonic()
config = ConfigParser()
config.read("config.ini")
t = config["Main"]["Token"]
config_channels = config["Channels"]

cache_key_str = config["Main"]["key"][2:-1]
cache_key_bytes = cache_key_str.encode()

# Define bot and remove overwritten commands
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True)
status = getattr(discord.Status, config["Activity"]["status"])

bot = commands.AutoShardedBot(
    status=status,
    intents=intents,
    activity=activity,
    command_prefix="t-",
    case_insensitive=True,
    chunk_guilds_at_startup=False,
)

pool = bot.loop.run_until_complete(
        asyncpg.create_pool(
            host=config["PostgreSQL Info"]["ip"],
            user=config["PostgreSQL Info"]["name"],
            database=config["PostgreSQL Info"]["db"],
            password=config["PostgreSQL Info"]["pass"]
        )
    )

bot.queue = dict()
bot.playing = dict()
bot.channels = dict()
bot.chunk_queue = list()
bot.remove_command("help")
bot.settings = settings.settings_class(pool)
bot.setlangs = settings.setlangs_class(pool)
bot.nicknames = settings.nickname_class(pool)
bot.cache = cache.cache(cache_key_bytes, pool)
bot.blocked_users = settings.blocked_users_class(pool)
bot.trusted = basic.remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")

for cog in listdir("cogs"):
    if cog.endswith(".py"):
        bot.load_extension(f"cogs.{cog[:-3]}")

@bot.event
async def on_ready():
    await bot.wait_until_ready()

    support_server_id = int(config["Main"]["main_server"])
    bot.supportserver = bot.get_guild(support_server_id)

    while bot.supportserver is None:
        print("Waiting 5 seconds")
        await asyncio.sleep(5)
        bot.supportserver = bot.get_guild(support_server_id)

    for channel_name in config_channels:
        channel_id = int(config_channels[channel_name])
        channel_object = bot.supportserver.get_channel(channel_id)
        bot.channels[channel_name] = channel_object

    try:
        await bot.starting_message.edit(content=f"~~{bot.starting_message.content}~~")
        bot.starting_message = await bot.channels["logs"].send(f"Restarted as {bot.user.name}!")
    except AttributeError:
        print(f"Starting as {bot.user.name}")

        for guild in bot.guilds:
            bot.playing[guild.id] = 0
            bot.queue[guild.id] = dict()

        bot.starting_message = await bot.channels["logs"].send(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")

bot.run(t)
