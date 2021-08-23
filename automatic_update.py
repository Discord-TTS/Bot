"""Automatic Updater
This file is to prevent manual work whenever updating the bot.
If you add a breaking change (new database table/column, new files, etc)
please add a corrisponding function and decorate it.

do_early_updates() is called before bot does any setup (database, etc)
do_normal_updates() is called just before the bot logs in"""

from __future__ import annotations

import asyncio
from configparser import ConfigParser
from typing import TYPE_CHECKING, Awaitable, Callable, List, Literal, Optional

import utils


if TYPE_CHECKING:
    from asyncpg import Record
    from main import TTSBot

    _UF = Callable[[TTSBot], Awaitable[Optional[bool]]]


def _update_config(config: ConfigParser):
    with open("config.ini", "w") as config_file:
        config.write(config_file)

def _update_defaults(bot: TTSBot) -> asyncio.Task[Record]:
    return bot.loop.create_task( # type: ignore
        bot.conn.fetchrow("SELECT * FROM guilds WHERE guild_id = 0;")
    )


def add_to_updates(type: Literal["early", "normal"]) -> Callable[[_UF], _UF]:
    def deco(func: _UF) -> _UF:
        global early_updates, normal_updates
        if type == "early":
            early_updates.append(func)
        elif type == "normal":
            normal_updates.append(func)
        else:
            raise TypeError("Invalid update add type!")

        return func
    return deco


early_updates: List[_UF] = []
normal_updates: List[_UF] = []

async def do_early_updates(bot: TTSBot):
    if bot.cluster_id not in {0, None}:
        return

    for func in early_updates:
        if await func(bot):
            print(f"Completed update: {func.__name__}")

async def do_normal_updates(bot: TTSBot):
    if bot.cluster_id not in {0, None}:
        return

    async with bot.pool.acquire() as conn:
        bot.conn = conn
        for func in normal_updates:
            if await func(bot):
                print(f"Completed update: {func.__name__}")
    del bot.conn

# All updates added from here
@add_to_updates("normal")
async def add_default_column(bot: TTSBot) -> bool:
    return not (await asyncio.gather(
        bot.pool.execute(
            "INSERT INTO guilds(guild_id) VALUES(0) ON CONFLICT (guild_id) DO NOTHING"
        ), bot.pool.execute(
            "INSERT INTO userinfo(user_id) VALUES(0) ON CONFLICT (user_id) DO NOTHING"
        ), bot.pool.execute(
            "INSERT INTO nicknames(guild_id, user_id) VALUES(0, 0) ON CONFLICT (guild_id, user_id) DO NOTHING"
        ),
    ))

@add_to_updates("normal")
async def add_analytics(bot: TTSBot) -> bool:
    if await bot.conn.fetchval("SELECT to_regclass('public.analytics')"):
        return False

    await bot.conn.execute(utils.ANALYTICS_CREATE)
    return True


@add_to_updates("early")
async def make_voxpopuli_async(_: TTSBot) -> bool:
    import inspect
    import voxpopuli

    if inspect.iscoroutinefunction(voxpopuli.Voice.to_audio):
        return False

    async_branch = "git+https://github.com/hadware/voxpopuli.git@async-support"
    command = ("python3", "-m", "pip", "install", "-U", async_branch)
    process = await asyncio.create_subprocess_exec(*command)
    await process.wait()

    raise Exception("Tried to update voxpopuli, please restart the bot!")

@add_to_updates("early")
async def update_config(bot: TTSBot) -> bool:
    config = bot.config
    if "Webhook URLs" in config:
        return False

    config["Webhook URLs"] = config["Channels"]
    config["Main"].pop("cache_key", None) # old key, to remove
    config["PostgreSQL Info"]["host"] = config["PostgreSQL Info"].pop("ip")
    config["PostgreSQL Info"]["user"] = config["PostgreSQL Info"].pop("name")
    config["PostgreSQL Info"]["database"] = config["PostgreSQL Info"].pop("db")
    config["PostgreSQL Info"]["password"] = config["PostgreSQL Info"].pop("pass")

    del config["Channels"]
    bot.config = config

    _update_config(config)
    return True

@add_to_updates("early")
async def setup_bot(bot: TTSBot) -> bool:
    if "key" in bot.config["Main"]:
        return False

    import asyncpg
    from cryptography.fernet import Fernet

    db_info = bot.config["PostgreSQL Info"]
    bot.config["Main"]["key"] = str(Fernet.generate_key())

    conn = await asyncpg.connect(**db_info)
    await conn.execute(utils.DB_SETUP_QUERY)
    await conn.close()

    _update_config(bot.config)
    return True

@add_to_updates("early")
async def cache_to_redis(bot: TTSBot) -> bool:
    if "Redis Info" in bot.config:
        return False

    bot.config["Redis Info"] = {"url": "redis://cache"}
    _update_config(bot.config)
    return True

@add_to_updates("normal")
async def add_log_level(bot: TTSBot) -> bool:
    if "log_level" in bot.config["Main"]:
        return False

    bot.config["Main"]["log_level"] = "INFO"
    _update_config(bot.config)
    return True
