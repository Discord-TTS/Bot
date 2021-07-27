from __future__ import annotations

import asyncio
import json
import os
import traceback
from configparser import ConfigParser
from os import listdir
from time import monotonic
from typing import Any, Awaitable, Callable, Coroutine, List, TYPE_CHECKING, Dict, Optional, cast

import aiohttp
import aioredis
import asyncgTTS
import asyncpg
import discord
from discord.ext import commands

import automatic_update
import utils

print("Starting TTS Bot Premium!")
start_time = monotonic()

# Read config file
config = ConfigParser()
config.read("config.ini")

# Setup activity and intents for logging in
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True, reactions=True)
status = getattr(discord.Status, config["Activity"]["status"])

# Custom prefix support
async def prefix(bot: TTSBotPremium, message: discord.Message) -> str:
    "Gets the prefix for a guild based on the passed message object"
    return (await bot.settings.get(message.guild, ["prefix"]))[0] if message.guild else "p-"

async def premium_check(ctx: utils.TypedGuildContext):
    if not ctx.bot.patreon_role:
        return

    if not ctx.guild:
        return True

    if str(ctx.author.id) in ctx.bot.trusted:
        return True

    if str(ctx.command) in ("donate", "add_premium") or str(ctx.command).startswith(("jishaku"),):
        return True

    support_server = ctx.bot.get_support_server()
    if not support_server:
        return False

    if not support_server.chunked:
        await support_server.chunk(cache=True)

    premium_user_for_guild = ctx.bot.donators.get(ctx.guild.id)
    if any(premium_user_for_guild == member.id for member in ctx.bot.patreon_role.members):
        return True

    print(f"{ctx.author} | {ctx.author.id} failed premium check in {ctx.guild.name} | {ctx.guild.id}")

    permissions = ctx.channel.permissions_for(ctx.guild.me) # type: ignore
    if permissions.send_messages:
        main_msg = f"Hey! This server isn't premium! Please purchase TTS Bot Premium via Patreon! (`{ctx.prefix}donate`)"
        footer_msg = "If this is an error, please contact Gnome!#6669."

        if permissions.embed_links:
            embed = discord.Embed(
                title="TTS Bot Premium",
                description=main_msg,
            )
            embed.set_footer(text=footer_msg)
            embed.set_thumbnail(url=str(ctx.bot.user.avatar_url))

            await ctx.send(embed=embed)
        else:
            await ctx.send(f"{main_msg}\n*{footer_msg}*")

Pool = asyncpg.Pool[asyncpg.Record] if TYPE_CHECKING else asyncpg.Pool
class TTSBotPremium(commands.AutoShardedBot):
    if TYPE_CHECKING:
        from extensions import cache_handler, database_handler
        from player import TTSVoicePlayer

        settings: database_handler.GeneralSettings
        userinfo: database_handler.UserInfoHandler
        nicknames: database_handler.NicknameHandler
        cache: cache_handler.CacheHandler

        command_prefix: Callable[[TTSBotPremium, discord.Message], Coroutine[Any, Any, str]]
        voice_clients: List[TTSVoicePlayer]
        patreon_json: Dict[str, int]
        analytics_buffer: utils.SafeDict
        cache_db: aioredis.Redis
        gtts: asyncgTTS.asyncgTTS
        pool: Pool

        del cache_handler, database_handler, TTSVoicePlayer

    def __init__(self, config: ConfigParser, session: aiohttp.ClientSession, *args, **kwargs):
        self.config = config
        self.session = session
        self.channels: Dict[str, discord.Webhook] = {}

        self.trusted = config["Main"]["trusted_ids"].strip("[]'").split(", ")
        self.donators: Dict[int, int] = {}
        self.tied_to_database = asyncio.Event()

        # cyrus01337: delete this block once utils.migrate_json_to_psql
        # performs it's initial run successfully
        with open("patreon_users.json") as f:
            self.patreon_json = json.load(f)

        super().__init__(*args, **kwargs)


    def get_support_server(self) -> Optional[discord.Guild]:
        return self.get_guild(int(self.config["Main"]["main_server"]))

    @property
    def invite_channel(self) -> Optional[discord.TextChannel]:
        support_server = self.get_support_server()
        return support_server.get_channel(835224660458864670) if support_server else None # type: ignore

    @property
    def patreon_role(self) -> Optional[discord.Role]:
        support_server = self.get_support_server()
        if not support_server:
            return

        return discord.utils.get(support_server.roles, name="Patreon!")

    @property
    def avatar_url(self) -> str:
        return str(self.user.avatar_url) if self.user else ""

    def load_extensions(self, folder: str):
        filered_exts = filter(lambda e: e.endswith(".py"), listdir(folder))
        for ext in filered_exts:
            self.load_extension(f"{folder}.{ext[:-3]}")


    def add_check(self, *args, **kwargs):
        super().add_check(*args, **kwargs)
        return self

    async def process_commands(self, message: discord.Message) -> None:
        if message.author.bot:
            return

        ctx_class = utils.TypedGuildContext if message.guild else utils.TypedContext
        ctx = await self.get_context(message=message, cls=ctx_class)

        await self.invoke(ctx)

    async def wait_until_ready(self, *args: Any, **kwargs: Any) -> bool:
        await super().wait_until_ready()
        await self.tied_to_database.wait()

        return True

    async def start(self, token: str, *args: None, **kwargs: bool):
        "Get everything ready in async env"
        cache_info = self.config["Redis Info"]
        db_info = self.config["PostgreSQL Info"]

        gcp_creds = os.getenv("GOOGLE_APPLICATION_CREDENTIALS")
        if not gcp_creds:
            raise asyncgTTS.AuthorizationException("GOOGLE_APPLICATION_CREDENTIALS is not set or empty!")

        self.cache_db = aioredis.from_url(**cache_info)
        self.pool, self.gtts = await asyncio.gather(
            cast(Awaitable[Pool], asyncpg.create_pool(**db_info)),
            asyncgTTS.setup(
                premium=True,
                session=self.session,
                service_account_json_location=gcp_creds
            ),
        )

        # Fill up bot.channels, as a load of webhooks
        for channel_name, webhook_url in self.config["Webhook URLs"].items():
            adapter = discord.AsyncWebhookAdapter(session=self.session)
            self.channels[channel_name] = discord.Webhook.from_url(
                url=webhook_url, adapter=adapter
            )

        # Load all of /cogs
        self.load_extensions("cogs")
        self.load_extensions("extensions")

        # Send starting message and actually start the bot
        await self.channels["logs"].send("Starting TTS Bot!")

        await automatic_update.do_normal_updates(self)
        await super().start(token, *args, **kwargs)


def get_error_string(e: BaseException) -> str:
    return f"{type(e).__name__}: {e}"

async def only_avaliable(ctx: utils.TypedContext):
    return not ctx.guild.unavailable if ctx.guild else True


async def main() -> None:
    async with aiohttp.ClientSession() as session:
        return await _real_main(session)

async def _real_main(session: aiohttp.ClientSession) -> None:
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
    ).add_check(premium_check).add_check(only_avaliable)

    # cyrus01337: testing something new, this might break or just flat
    # out not work but ideally I want no commands to run until the bot
    # is ready/has completed setup
    bot.add_check(bot.wait_until_ready)

    await automatic_update.do_early_updates(bot)
    try:
        print("\nLogging into Discord...")
        ready_task = asyncio.create_task(bot.wait_until_ready())
        bot_task = asyncio.create_task(bot.start(token=config["Main"]["Token"]))

        await automatic_update.do_early_updates(bot)
        done, _ = await asyncio.wait((bot_task, ready_task), return_when=asyncio.FIRST_COMPLETED)
        if bot_task in done:
            error = bot_task.exception()
            print("Bot shutdown before ready!")
            if error:
                traceback.print_exception(type(error), error, error.__traceback__)
            return

        print(f"Logged in as {bot.user} and ready!")
        await bot.channels["logs"].send(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")
        await bot_task
    except Exception as e:
        print(get_error_string(e))
    finally:
        if not bot.user:
            return

        await bot.channels["logs"].send(f"{bot.user.mention} is shutting down.")
        await asyncio.wait_for(asyncio.gather(
            bot.pool.close(), bot.cache_db.close(), bot.close()
        ), timeout=5)

try:
    import uvloop
    uvloop.install()
except ModuleNotFoundError:
    print("Failed to import uvloop, performance may be reduced")

try:
    asyncio.run(main())
except (KeyboardInterrupt, RuntimeError) as e:
    print(f"Shutdown forcefully: {get_error_string(e)}")
