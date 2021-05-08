import asyncio
import json
import os
from configparser import ConfigParser
from time import monotonic
from typing import Union

import aiohttp
import asyncgTTS
import asyncpg
import discord
from discord.ext import commands

from utils.basic import remove_chars
from utils.decos import wrap_with

print("Starting TTS Bot!")
start_time = monotonic()

# Read config file
config = ConfigParser()
config.read("config.ini")

# Setup activity and intents for logging in
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True, reactions=True)
status = getattr(discord.Status, config["Activity"]["status"])

# Custom prefix support
async def prefix(bot: commands.AutoShardedBot, message: discord.Message) -> str:
    "Gets the prefix for a guild based on the passed message object"
    return await bot.settings.get(message.guild, "prefix") if message.guild else "p-"

async def premium_check(ctx):
    if not ctx.bot.patreon_role:
        return

    if not ctx.guild:
        return True

    if str(ctx.author.id) in ctx.bot.trusted:
        return True

    if str(ctx.command) in ("donate", "add_premium") or str(ctx.command).startswith(("jishaku"),):
        return True

    premium_user_for_guild = ctx.bot.patreon_json.get(str(ctx.guild.id))
    if premium_user_for_guild in (member.id for member in ctx.bot.patreon_role.members):
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
            embed.set_thumbnail(url=ctx.bot.user.avatar_url)

            await ctx.send(embed=embed)
        else:
            await ctx.send(f"Hey! This server isn't premium! Please purchase TTS Bot Premium via Patreon! (`{ctx.prefix}donate`)\n*If this is an error, please contact Gnome!#6669.*")


class TTSBotPremium(commands.AutoShardedBot):
    def __init__(self, config, session, *args, **kwargs):
        self.channels = {}
        self.config = config
        self.session = session

        self.trusted = remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")

        with open("patreon_users.json") as f:
            self.patreon_json = json.load(f)

        super().__init__(*args, **kwargs)


    @property
    def support_server(self):
        return self.get_guild(int(self.config["Main"]["main_server"]))

    @property
    def patreon_role(self):
        return discord.utils.get(self.support_server.roles, name="Patreon!")

    def add_check(self, *args, **kwargs):
        super().add_check(*args, **kwargs)
        return self


    def load_extensions(self, folder):
        filered_exts = filter(lambda e: e.endswith(".py"), os.listdir(folder))
        for ext in filered_exts:
            self.load_extension(f"{folder}.{ext[:-3]}")

    async def start(self, *args, token, **kwargs):
        # Get everything ready in async env
        db_info = self.config["PostgreSQL Info"]
        self.gtts, self.pool = await asyncio.gather(
            asyncgTTS.setup(
                premium=True,
                session=self.session,
                service_account_json_location=os.getenv("GOOGLE_APPLICATION_CREDENTIALS")
            ),
            asyncpg.create_pool(
                host=db_info["ip"],
                user=db_info["name"],
                database=db_info["db"],
                password=db_info["pass"]
            )
        )

        # Fill up bot.channels, as a load of webhooks
        for channel_name, webhook_url in self.config["Channels"].items():
            self.channels[channel_name] = discord.Webhook.from_url(
                url=webhook_url,
                adapter=discord.AsyncWebhookAdapter(session=self.session)
            )

        # Load all of /cogs and /extensions
        self.load_extensions("cogs")
        self.load_extensions("extensions")

        # Send starting message and actually start the bot
        await self.channels["logs"].send("Starting TTS Bot Premium!")
        await super().start(token, *args, **kwargs)


def get_error_string(e: BaseException) -> str:
    return f"{type(e).__name__}: {e}"

@wrap_with(aiohttp.ClientSession, aenter=True)
async def main(session):
    bot = TTSBotPremium(
        config=config,
        status=status,
        intents=intents,
        session=session,
        help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
        activity=activity,
        command_prefix=prefix,
        case_insensitive=True,
        chunk_guilds_at_startup=False,
        allowed_mentions=discord.AllowedMentions(everyone=False, roles=False)
    ).add_check(premium_check)

    try:
        print("\nLogging into Discord...")
        ready_task = asyncio.create_task(bot.wait_until_ready())
        bot_task = asyncio.create_task(bot.start(token=config["Main"]["Token"]))

        done, pending = await asyncio.wait((bot_task, ready_task), return_when=asyncio.FIRST_COMPLETED)
        if bot_task in done:
            raise RuntimeError(f"Bot Shutdown before ready: {get_error_string(bot_task.exception())}")

        print(f"Logged in as {bot.user} and ready!")
        await bot.channels["logs"].send(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")

        await bot.support_server.chunk(cache=True)
        await bot_task
    except Exception as e:
        print(get_error_string(e))
    finally:
        if not bot.user:
            return

        await bot.channels["logs"].send(f"{bot.user.mention} is shutting down.")
        await bot.close()

try:
    asyncio.run(main())
except (KeyboardInterrupt, RuntimeError) as e:
    print(f"Shutdown forcefully: {get_error_string(e)}")
