from __future__ import annotations

import asyncio
import sys
import traceback
from configparser import ConfigParser
from os import listdir
from signal import SIGHUP, SIGINT, SIGTERM
from time import monotonic
from typing import (TYPE_CHECKING, Any, Awaitable, Dict, List, Optional, Tuple,
                    TypeVar, Union, cast)

import aiohttp
import aioredis
import asyncgTTS
import asyncpg
import discord
import websockets
from discord.ext import commands as _commands

import automatic_update
import utils

print("Starting TTS Bot!")
start_time = monotonic()

# Read config file
config = ConfigParser()
config.read("config.ini")

# Setup activity and intents for logging in
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True)
status = getattr(discord.Status, config["Activity"]["status"])

allowed_mentions = discord.AllowedMentions(everyone=False, roles=False)
cache_flags = discord.MemberCacheFlags(joined=False)

# Custom prefix support

if TYPE_CHECKING:
    _T = TypeVar("_T")
    Pool = asyncpg.Pool[asyncpg.Record]
else:
    Pool = asyncpg.Pool

class TTSBot(_commands.AutoShardedBot):
    if TYPE_CHECKING:
        from extensions import cache_handler, database_handler
        from player import TTSVoicePlayer

        cache: cache_handler.CacheHandler
        settings: database_handler.TableHandler[int]
        userinfo: database_handler.TableHandler[int]
        nicknames: database_handler.TableHandler[Tuple[int, int]]

        analytics_buffer: utils.SafeDict
        cache_db: aioredis.Redis
        gtts: asyncgTTS.easygTTS
        status_code: int
        blocked: bool # Handles if to be on gtts or espeak
        pool: Pool

        voice_clients: List[TTSVoicePlayer]
        user: discord.ClientUser

        conn: asyncpg.pool.PoolConnectionProxy[asyncpg.Record]
        del cache_handler, database_handler, TTSVoicePlayer

    def __init__(self,
        config: ConfigParser,
        session: aiohttp.ClientSession,
        cluster_id: Optional[int] = None,
    *args: Any, **kwargs: Any):

        self.config = config
        self.websocket = None
        self.session = session
        self.sent_fallback = False
        self.cluster_id = cluster_id
        self.channels: Dict[str, discord.Webhook] = {}
        self.tasks: asyncio.Queue[Awaitable[Any]] = asyncio.Queue()

        self.status_code = utils.RESTART_CLUSTER
        self.trusted = config["Main"]["trusted_ids"].strip("[]'").split(", ")

        kwargs["command_prefix"] = self.command_prefix
        super().__init__(*args, **kwargs) # type: ignore


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


    async def get_invite_channel(self) -> Optional[discord.TextChannel]:
        channel_id = 694127922801410119
        support_server = self.get_support_server()
        if support_server is None:
            return await self.fetch_channel(channel_id) # type: ignore
        else:
            return support_server.get_channel(channel_id) # type: ignore

    async def check_gtts(self) -> Union[bool, Exception]:
        try:
            await self.gtts.get(text="RL Test", lang="en")
            return True
        except asyncgTTS.RatelimitException:
            return False
        except Exception as e:
            return e

    async def user_from_dm(self, dm_name: str) -> Optional[discord.User]:
        match = utils.ID_IN_BRACKETS_REGEX.search(dm_name)
        if not match:
            return

        real_user_id = int(match.group(1))
        try:
            return await self.fetch_user(real_user_id)
        except _commands.UserNotFound:
            return


    def add_check(self: _T, *args: Any, **kwargs: Any) -> _T:
        super().add_check(*args, **kwargs)
        return self

    async def wait_until_ready(self, *_: Any, **__: Any) -> None:
        return await super().wait_until_ready()

    @staticmethod
    async def command_prefix(bot: TTSBot, message: discord.Message) -> str:
        if message.guild:
            return (await bot.settings.get(message.guild.id))["prefix"]

        return "-"

    async def get_context(self,
        message: discord.Message
    ) -> Union[utils.TypedContext, utils.TypedGuildContext]:
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

        self.cache_db = aioredis.from_url(**cache_info)
        self.pool, self.gtts = await asyncio.gather(
            cast(Awaitable[Pool], asyncpg.create_pool(**db_info)),
            asyncgTTS.setup(premium=False, session=self.session),
        )

        # Fill up bot.channels, as a load of webhooks
        for channel_name, webhook_url in self.config["Webhook URLs"].items():
            self.channels[channel_name] = discord.Webhook.from_url(
                url=webhook_url,
                session=self.session,
                bot_token=self.http.token
            )

        # Load all of /cogs and /extensions
        self.load_extensions("cogs")
        self.load_extensions("extensions")

        # Send starting message and actually start the bot
        if self.shard_ids is not None:
            prefix = f"`[Cluster] [ID {self.cluster_id}] [Shards {len(self.shard_ids)}]`: "
            self.websocket = await self.create_websocket()
        else:
            self.websocket = None
            prefix = ""

        self.logger = utils.setup_logging(config["Main"]["log_level"], prefix, self.session)
        self.logger.info("Starting TTS Bot!")

        await automatic_update.do_normal_updates(self)
        await super().start(token, **kwargs)


def get_error_string(e: BaseException) -> str:
    return f"{type(e).__name__}: {e}"

async def only_avaliable(ctx: utils.TypedContext):
    return not ctx.guild.unavailable if ctx.guild else True


async def on_ready(bot: TTSBot):
    await bot.wait_until_ready()
    bot.logger.info(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")

async def main(*args: Any, **kwargs: Any) -> int:
    async with aiohttp.ClientSession() as session:
        return await _real_main(session, *args, **kwargs)

async def _real_main(
    session: aiohttp.ClientSession,
    cluster_id: Optional[int] = None,
    total_shard_count: Optional[int] = None,
    shards_to_handle: Optional[List[int]] = None,
) -> int:
    bot = TTSBot(
        config=config,
        status=status,
        intents=intents,
        session=session,
        max_messages=None,
        help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
        activity=activity,
        cluster_id=cluster_id,
        case_insensitive=True,
        shard_ids=shards_to_handle,
        shard_count=total_shard_count,
        chunk_guilds_at_startup=False,
        member_cache_flags=cache_flags,
        allowed_mentions=allowed_mentions,
    ).add_check(only_avaliable)

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
