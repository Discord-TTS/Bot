from __future__ import annotations

import asyncio
import json
import os
import sys
import traceback
from configparser import ConfigParser
from os import listdir
from signal import SIGHUP, SIGINT, SIGTERM
from time import monotonic
from typing import (TYPE_CHECKING, Any, Awaitable, Optional, TypeVar, Union,
                    cast, overload)

import aiohttp
import aioredis
import asyncgTTS
import asyncpg
import discord
import websockets
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

allowed_mentions = discord.AllowedMentions(everyone=False, roles=False)
cache_flags = discord.MemberCacheFlags(joined=False)

async def premium_check(ctx: utils.TypedGuildContext):
    if ctx.guild is None:
        return True

    if str(ctx.author.id) in ctx.bot.trusted:
        return True

    if str(ctx.command) in ("donate", "add_premium") or str(ctx.command).startswith(("jishaku"),):
        return True

    if ctx.bot.patreon_members is None:
        ctx.bot.patreon_members = await ctx.bot.fill_patreon_members()

    premium_user: Optional[int] = ctx.bot.settings[ctx.guild.id]["premium_user"]
    if premium_user is not None and premium_user in ctx.bot.patreon_members:
        return True

    ctx.bot.logger.warning(f"{ctx.author} | {ctx.author.id} failed premium check in {ctx.guild.name} | {ctx.guild.id}")
    permissions: discord.Permissions = ctx.channel.permissions_for(ctx.guild.me)
    if permissions.send_messages:
        main_msg = f"Hey! This server isn't premium! Please purchase TTS Bot Premium via Patreon! (`{ctx.prefix}donate`)"
        footer_msg = "If this is an error, please contact Gnome!#6669."

        if permissions.embed_links:
            embed = discord.Embed(
                title="TTS Bot Premium",
                description=main_msg,
            )
            embed.set_footer(text=footer_msg)
            embed.set_thumbnail(url=ctx.bot.user.display_avatar.url)

            await ctx.send(embed=embed)
        else:
            await ctx.send(f"{main_msg}\n*{footer_msg}*")


if TYPE_CHECKING:
    _T = TypeVar("_T")
    Pool = asyncpg.Pool[asyncpg.Record]
else:
    Pool = asyncpg.Pool

class TTSBotPremium(commands.AutoShardedBot, utils.CommonCog):
    if TYPE_CHECKING:
        from extensions import cache_handler, database_handler
        from player import TTSVoiceClient

        cache: cache_handler.CacheHandler
        settings: database_handler.TableHandler[int]
        userinfo: database_handler.TableHandler[int]
        nicknames: database_handler.TableHandler[tuple[int, int]]

        voice_clients: list[TTSVoiceClient]
        analytics_buffer: utils.SafeDict
        _voice_data: list[utils.Voice]
        gtts: asyncgTTS.asyncgTTS
        cache_db: aioredis.Redis
        bot: TTSBotPremium
        pool: Pool

        voice_clients: list[TTSVoiceClient]
        user: discord.ClientUser

        conn: asyncpg.pool.PoolConnectionProxy[asyncpg.Record]
        del cache_handler, database_handler, TTSVoiceClient

    def __init__(self,
        config: ConfigParser,
        session: aiohttp.ClientSession,
        cluster_id: Optional[int] = None,
    *args: Any, **kwargs: Any):

        self.bot = self
        self.config = config
        self.websocket = None
        self.session = session
        self.cluster_id = cluster_id

        self.status_code = utils.RESTART_CLUSTER
        self.channels: dict[str, discord.Webhook] = {}
        self.patreon_members: Optional[list[int]] = None

        self.trusted = config["Main"]["trusted_ids"].strip("[]'").split(", ")

        return super().__init__(*args, **kwargs,
            command_prefix=self.command_prefix,
            slash_command_guild=(
                int(self.config["Main"]["slash_command_guild"])
                if "slash_command_guild" in self.config["Main"] else None
            ),
        )


    def handle_error(self, task: asyncio.Task):
        try:
            exception = task.exception()
        except (asyncio.CancelledError, RecursionError):
            return

        if exception is not None:
            self.create_task(self.on_error("task", exception))

    def create_task(self, coro: Awaitable[_T], *args: Any, **kwargs: Any) -> asyncio.Task[_T]:
        task = self.loop.create_task(coro, *args, **kwargs)
        task.add_done_callback(self.handle_error)
        return task

    @property
    def invite_channel(self) -> Optional[discord.TextChannel]:
        support_server = self.get_support_server()
        return support_server.get_channel(835224660458864670) if support_server else None # type: ignore


    def log(self, event: str) -> None:
        self.analytics_buffer.add(event)

    def get_support_server(self) -> Optional[discord.Guild]:
        return self.get_guild(int(self.config["Main"]["main_server"]))

    def load_extensions(self, folder: str):
        filered_exts = filter(lambda e: e.endswith(".py"), listdir(folder))
        for ext in filered_exts:
            self.load_extension(f"{folder}.{ext[:-3]}")

    def create_websocket(self) -> Awaitable[websockets.WebSocketClientProtocol]:
        host = self.config["Clustering"].get("websocket_host", "localhost")
        port = self.config["Clustering"].get("websocket_port", "8765")

        uri = f"ws://{host}:{port}/{self.cluster_id}"
        return websockets.connect(uri)

    def get_patreon_role(self) -> Optional[discord.Role]:
        support_server = self.get_support_server()
        if not support_server:
            return

        return discord.utils.get(support_server.roles, name="Patreon!")

    async def get_invite_channel(self) -> Optional[discord.TextChannel]:
        channel_id = 694127922801410119
        support_server = self.get_support_server()
        if support_server is None:
            return await self.fetch_channel(channel_id) # type: ignore
        else:
            return support_server.get_channel(channel_id) # type: ignore

    async def user_from_dm(self, dm_name: str) -> Optional[discord.User]:
        match = utils.ID_IN_BRACKETS_REGEX.search(dm_name)
        if not match:
            return

        real_user_id = int(match.group(1))
        try:
            return await self.fetch_user(real_user_id)
        except commands.UserNotFound:
            return

    @utils.decos.require_voices
    async def get_voice(self, lang: str, variant: Optional[str] = None) -> utils.Voice:
        generator = {
            True:  (voice for voice in self._voice_data if voice.lang == lang and voice.variant == variant),
            False: (voice for voice in self._voice_data if voice.lang == lang)
        }

        voice = next(generator[bool(variant)], None)
        if not voice:
            raise utils.VoiceNotFound(f"Cannot find voice with {lang} {variant}")

        return voice

    async def safe_get_voice(self, ctx: utils.TypedContext, lang: str, variant: Optional[str] = None) -> Union[utils.Voice, discord.Embed]:
        try:
            return await self.get_voice(lang, variant)
        except utils.VoiceNotFound:
            embed = discord.Embed(title=f"Cannot find voice with language `{lang}` and variant `{variant}` combo!")
            embed.set_author(name=str(ctx.author), icon_url=ctx.author.display_avatar.url)
            embed.set_footer(text=f"Try {ctx.prefix}voices for a full list!")
            return embed

    async def fill_patreon_members(self) -> list[int]:
        support_server = self.get_support_server()
        if support_server is None:
            return []

        patreon_role = self.get_patreon_role()
        if patreon_role is None:
            return []

        support_members = await support_server.chunk()
        if support_members is None:
            return []

        support_members = cast(list[discord.Member], support_members)
        self.patreon_members = [
            member.id
            for member in support_members
            if member.get_role(patreon_role.id)
        ]
        return self.patreon_members


    def add_check(self: _T, *args: Any, **kwargs: Any) -> _T:
        super().add_check(*args, **kwargs)
        return self

    async def wait_until_ready(self, *_: Any, **__: Any) -> None:
        return await super().wait_until_ready()

    @staticmethod
    async def command_prefix(bot: TTSBotPremium, message: discord.Message) -> str:
        if message.guild:
            return (await bot.settings.get(message.guild.id))["prefix"]

        return "p-"

    @overload
    async def get_context(self, message: discord.Message, cls: None = None) -> Union[utils.TypedGuildContext, utils.TypedContext]: ...

    @overload
    async def get_context(self, message: discord.Message, cls: type[_T]) -> _T: ...

    async def get_context(self, message: discord.Message, cls=None):
        if cls is None:
            cls = utils.TypedGuildContext if message.guild else utils.TypedContext

        return await super().get_context(message, cls=cls)

    def close(self, status_code: Optional[int] = None) -> Awaitable[None]:
        if status_code is not None:
            self.status_code = status_code
            self.logger.debug(f"Shutting down with status code {status_code}")

        return super().close()

    async def start(self, token: str, **kwargs: Any):
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
            webhook: discord.Webhook = discord.Webhook.from_url(
                url=webhook_url,
                session=self.session,
                bot_token=self.http.token
            )
            webhook._state = self._connection
            self.channels[channel_name] = await webhook.fetch()

        # Create websocket and other clustering specific stuff
        if self.shard_ids is not None:
            prefix = f"`[Cluster] [ID {self.cluster_id}] [Shards {len(self.shard_ids)}]`: "
            self.websocket = await self.create_websocket()
        else:
            self.websocket = None
            prefix = ""

        # Load all of /cogs and /extensions
        self.load_extensions("cogs")
        self.load_extensions("extensions")

        # Setup logging and more migrations
        self.logger = utils.setup_logging(config["Main"]["log_level"], prefix, self.session)
        await automatic_update.do_normal_updates(self)

        # Add all persistent views
        tracebacks = await self.pool.fetch("SELECT * from errors")
        for row in tracebacks:
            view = utils.ShowTracebackView(f"```{row['traceback']}```")
            self.add_view(view, message_id=row["message_id"])

        # Actually start the bot
        self.logger.info("Starting TTS Bot!")
        await super().start(token, **kwargs)


def get_error_string(e: BaseException) -> str:
    return f"{type(e).__name__}: {e}"

async def only_avaliable(ctx: utils.TypedContext):
    return not ctx.guild.unavailable if ctx.guild else True


async def on_ready(bot: TTSBotPremium):
    await bot.wait_until_ready()
    bot.logger.info(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")

async def main(*args: Any, **kwargs: Any) -> int:
    async with aiohttp.ClientSession() as session:
        return await _real_main(session, *args, **kwargs)

async def _real_main(
    session: aiohttp.ClientSession,
    cluster_id: Optional[int] = None,
    total_shard_count: Optional[int] = None,
    shards_to_handle: Optional[list[int]] = None,
) -> int:
    bot = TTSBotPremium(
        config=config,
        status=status,
        intents=intents,
        session=session,
        max_messages=None,
        help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
        activity=activity,
        slash_commands=True,
        cluster_id=cluster_id,
        case_insensitive=True,
        shard_ids=shards_to_handle,
        shard_count=total_shard_count,
        chunk_guilds_at_startup=False,
        member_cache_flags=cache_flags,
        allowed_mentions=allowed_mentions,
    ).add_check(only_avaliable).add_check(premium_check)

    def stop_bot_sync(sig: int):
        bot.status_code = -sig
        bot.logger.warning(f"Recieved signal {sig} and shutting down.")

        bot.create_task(bot.close())

    for sig in (SIGINT, SIGTERM, SIGHUP):
        bot.loop.add_signal_handler(sig, stop_bot_sync, sig)

    await automatic_update.do_early_updates(bot)
    try:
        bot.create_task(on_ready(bot))
        await bot.start(token=config["Main"]["Token"])
        return bot.status_code

    except Exception:
        traceback.print_exception(*sys.exc_info())
        return utils.DO_NOT_RESTART_CLUSTER

    finally:
        if not bot.user:
            return utils.DO_NOT_RESTART_CLUSTER

        closing_coros = [bot.pool.close(), bot.cache_db.close(), bot.close()]
        if bot.websocket is not None:
            closing_coros.append(bot.websocket.close())

        bot.logger.info(f"{bot.user.mention} is shutting down.")
        await asyncio.wait_for(asyncio.gather(*closing_coros), timeout=5)

try:
    import uvloop
    uvloop.install()
except ModuleNotFoundError:
    print("Failed to import uvloop, performance may be reduced")

if __name__ == "__main__":
    asyncio.run(main())
