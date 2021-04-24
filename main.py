import asyncio
from configparser import ConfigParser
from functools import wraps
from os import listdir
from time import monotonic

import aiohttp
import asyncgTTS
import asyncpg
import discord
from discord.ext import commands

from utils import basic, cache, settings

print("Starting TTS Bot!")
start_time = monotonic()

# Read config file
config = ConfigParser()
config.read("config.ini")

# Get cache mp3 decryption key
cache_key_str = config["Main"]["key"][2:-1]
cache_key_bytes = cache_key_str.encode()

# Setup activity and intents for logging in
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True)
status = getattr(discord.Status, config["Activity"]["status"])

# Custom prefix support
async def prefix(bot: commands.AutoShardedBot, message: discord.Message) -> str:
    "Gets the prefix for a guild based on the passed message object"
    return await bot.settings.get(message.guild, "prefix") if message.guild else "-"

class TTSBot(commands.AutoShardedBot):
    def __init__(self, config, *args, **kwargs):
        self.config = config
        super().__init__(*args, **kwargs)

    @property
    def support_server(self):
        return self.get_guild(self.config["Main"]["main_server"])

    def load_extensions(self, exts):
        filered_exts = filter(lambda e: e.endswith(".py"), exts)
        for ext in filered_exts:
            self.load_extension(f"cogs.{ext[:-3]}")

    async def start(self, session, *args, **kwargs):
        config = self.config
        self.session = session

        self.gtts, pool = await asyncio.gather(
            asyncgTTS.setup(
                premium=False,
                session=self.session
            ),
            asyncpg.create_pool(
                host=config["PostgreSQL Info"]["ip"],
                user=config["PostgreSQL Info"]["name"],
                database=config["PostgreSQL Info"]["db"],
                password=config["PostgreSQL Info"]["pass"]
            )
        )

        # Setup all bot.vars in one place
        self.settings = settings.settings_class(pool)
        self.setlangs = settings.setlangs_class(pool)
        self.nicknames = settings.nickname_class(pool)
        self.cache = cache.cache(cache_key_bytes, pool)
        self.blocked_users = settings.blocked_users_class(pool)
        self.trusted = basic.remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")
        self.queue = self.channels = self.should_return = self.message_locks = self.currently_playing = dict()

        for channel_name, webhook_url in config["Channels"].items():
            self.channels[channel_name] = discord.Webhook.from_url(
                url=webhook_url,
                adapter=discord.AsyncWebhookAdapter(session=self.session)
            )

        self.load_extensions(listdir("cogs"))
        await self.channels["logs"].send("Starting TTS Bot!")
        await super().start(*args, **kwargs)

def wrap_session(func):
    @wraps(func)
    async def wrapper():
        async with aiohttp.ClientSession() as session:
            return await func(session)

    return wrapper

async def run_bot(bot, session, token):
    try:
        await bot.start(session, token)
    except Exception as e:
        print(f"{repr(e)}\nWhile starting bot, force killing to prevent deadlock.")
        bot.loop.stop()

@wrap_session
async def main(session):
    bot = TTSBot(
        config=config,
        status=status,
        intents=intents,
        help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
        activity=activity,
        command_prefix=prefix,
        case_insensitive=True,
        chunk_guilds_at_startup=False,
        allowed_mentions=discord.AllowedMentions(everyone=False, roles=False)
    )

    try:
        print("\nLogging into Discord...")
        bot_task = asyncio.create_task(run_bot(bot, session, config["Main"]["Token"]))
        await bot.wait_until_ready()

        print(f"Logged in as {bot.user} and ready!")
        await bot.channels["logs"].send(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")
        await bot_task
    except Exception as e:
        print(repr(e))
    finally:
        await bot.channels["logs"].send(f"{bot.user.mention} is shutting down.")
        await bot.session.close()
        await bot.close()

try:
    asyncio.run(main())
except (KeyboardInterrupt, RuntimeError) as e:
    print(f"Shutdown forcefully: {type(e).__name__}: {e}")
