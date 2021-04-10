import asyncio
import json
import os
from configparser import ConfigParser
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
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True, reactions=True)
status = getattr(discord.Status, config["Activity"]["status"])

# Custom prefix support
async def prefix(bot: commands.AutoShardedBot, message: discord.Message) -> str:
    "Gets the prefix for a guild based on the passed message object"
    return await bot.settings.get(message.guild, "prefix") if message.guild else "p-"

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

@bot.check
async def premium_check(ctx):
    if not getattr(bot, "patreon_role", None):
        return

    if not ctx.guild:
        return True

    if str(ctx.author.id) in bot.trusted:
        return True

    if str(ctx.command) in ("donate", "add_premium"):
        return True

    premium_user_for_guild = bot.patreon_json.get(str(ctx.guild.id))
    if premium_user_for_guild in (member.id for member in bot.patreon_role.members):
        return True

    print(f"{ctx.author} | {ctx.author.id} failed premium check in {ctx.guild.name} | {ctx.guild.id}")

    permissions = ctx.channel.permissions_for(ctx.guild.me)
    if permissions.send_messages:
        if permissions.embed_links:
            embed = discord.Embed(
                title="TTS Bot Premium",
                description=f"Hey! This server isn't premium! Please purchase TTS Bot Premium via Patreon! (`{ctx.prefix}donate`)",
            )
            embed.set_footer(text="If this is an error, please contact Gnome!#6669.")
            embed.set_thumbnail(url=bot.user.avatar_url)

            await ctx.send(embed=embed)
        else:
            await ctx.send(f"Hey! This server isn't premium! Please purchase TTS Bot Premium via Patreon! (`{ctx.prefix}donate`)\n*If this is an error, please contact Gnome!#6669.*")

async def main(bot):
    # Setup async objects, such as aiohttp session and database pool
    bot.session = ClientSession()
    bot.gtts, pool = await asyncio.gather(
        asyncgTTS.setup(
            premium=True,
            session=bot.session,
            service_account_json_location=os.getenv("GOOGLE_APPLICATION_CREDENTIALS")
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

    with open("patreon_users.json") as f:
        bot.patreon_json = json.load(f)

    # Load all the cogs, now bot.vars are ready
    for cog in os.listdir("cogs"):
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

    await bot.supportserver.chunk(cache=True)
    bot.patreon_role = discord.utils.get(bot.supportserver.roles, name="Patreon!")

try:
    bot.loop.run_until_complete(main(bot))
    bot.loop.run_forever()
except KeyboardInterrupt:
    print("KeyboardInterrupt: Killing bot")
