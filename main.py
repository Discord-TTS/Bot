import asyncio
from configparser import ConfigParser
from os import listdir
from time import monotonic

import asyncgTTS
import asyncpg
import discord
from aiohttp import ClientSession
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

bot = commands.AutoShardedBot(
    status=status,
    intents=intents,
    help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
    activity=activity,
    command_prefix=prefix,
    case_insensitive=True,
    chunk_guilds_at_startup=False,
    allowed_mentions=discord.AllowedMentions(everyone=False, roles=False)
)

async def main(bot):
    # Setup async objects, such as aiohttp session and database pool
    bot.session = ClientSession()
    bot.gtts, pool = await asyncio.gather(
        asyncgTTS.setup(
            premium=False,
            session=bot.session
        ),
        asyncpg.create_pool(
            host=config["PostgreSQL Info"]["ip"],
            user=config["PostgreSQL Info"]["name"],
            database=config["PostgreSQL Info"]["db"],
            password=config["PostgreSQL Info"]["pass"]
        )
    )

    # Setup all bot.vars in one place
    bot.queue = dict()
    bot.channels = dict()
    bot.should_return = dict()
    bot.message_locks = dict()
    bot.currently_playing = dict()
    bot.settings = settings.settings_class(pool)
    bot.setlangs = settings.setlangs_class(pool)
    bot.nicknames = settings.nickname_class(pool)
    bot.cache = cache.cache(cache_key_bytes, pool)
    bot.blocked_users = settings.blocked_users_class(pool)
    bot.trusted = basic.remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")

    # Load all the cogs, now bot.vars are ready
    for cog in listdir("cogs"):
        if cog.endswith(".py"):
            bot.load_extension(f"cogs.{cog[:-3]}")
            print(f"Successfully loaded: {cog}")

    # Setup bot.channels, as partial webhooks detatched from bot object
    for channel_name, webhook_url in config["Channels"].items():
        bot.channels[channel_name] = discord.Webhook.from_url(
            url=webhook_url,
            adapter=discord.AsyncWebhookAdapter(session=bot.session)
        )

    async def run_bot():
        # Background task to run bot
        print("\nLogging into Discord...")

        await bot.start(config["Main"]["Token"])
        if not bot.is_closed():
            await bot.close()

        # Cleanup before asyncio loop shutdown
        await bot.channels["logs"].send(f"{bot.user.mention} is shutting down.")
        await bot.session.close()

    # Queue bot to start in background, then wait for bot to start.
    bot_runner = bot.loop.create_task(run_bot())
    bot_runner.add_done_callback(lambda fut: bot.loop.stop())
    await bot.wait_until_ready()

    # on_ready but only firing once, get bot.supportserver then return
    print(f"Logged in as {bot.user} and ready!")
    await bot.channels["logs"].send(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")

    support_server_id = int(config["Main"]["main_server"])
    bot.supportserver = bot.get_guild(support_server_id)

    while bot.supportserver is None:
        print("Waiting 5 seconds")
        await asyncio.sleep(5)
        bot.supportserver = bot.get_guild(support_server_id)

try:
    bot.loop.run_until_complete(main(bot))
    bot.loop.run_forever()
except KeyboardInterrupt:
    print("KeyboardInterrupt: Killing bot")
