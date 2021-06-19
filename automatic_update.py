"""Automatic Updater
This file is to prevent manual work whenever updating the bot.
If you add a breaking change (new database table/column, new files, etc)
please add a corrisponding function and decorate it.

do_early_updates() is called before bot does any setup (database, etc)
do_normal_updates() is called just before the bot logs in"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Awaitable, Callable, List, Literal


if TYPE_CHECKING:
    from main import TTSBot
    UpdateFunction = Callable[[TTSBot], Awaitable[bool]]


def add_to_updates(type: Literal["early", "normal"]) -> Callable[[UpdateFunction], UpdateFunction]:
    def deco(func: UpdateFunction) -> UpdateFunction:
        global early_updates, normal_updates
        if type == "early":
            early_updates.append(func)
        elif type == "normal":
            normal_updates.append(func)
        else:
            raise TypeError("Invalid update add type!")

        return func
    return deco


early_updates: List[UpdateFunction] = []
normal_updates: List[UpdateFunction] = []

async def do_early_updates(bot: TTSBot):
    for func in early_updates:
        if await func(bot):
            print(f"Completed update: {func.__name__}")

async def do_normal_updates(bot: TTSBot):
    for func in normal_updates:
        if await func(bot):
            print(f"Completed update: {func.__name__}")


# All updates added from here
@add_to_updates("normal")
async def add_default_column(bot: TTSBot) -> bool:
    result = await bot.settings.DEFAULT_SETTINGS
    if result is not None:
        # Default column already created
        return False

    await bot.pool.execute("INSERT INTO guilds(guild_id) VALUES(0)")
    bot.settings.DEFAULT_SETTINGS = asyncio.create_task(
        bot.pool.fetchrow("SELECT * FROM guilds WHERE guild_id = 0;")
    )

    return True
