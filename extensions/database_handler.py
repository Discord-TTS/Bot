from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any, Dict, List, Optional, Tuple, Union

import asyncpg


if TYPE_CHECKING:
    from discord import Guild
    from discord.abc import User
    from typing_extensions import TypeVar

    from main import TTSBot

    Return = TypeVar("Return")


def setup(bot: TTSBot):
    bot.settings = GeneralSettings(bot)
    bot.userinfo = UserInfoHandler(bot)
    bot.nicknames = NicknameHandler(bot)


_DK = Union[int, Tuple[int, ...]]
class CacheWriter:
    def __init__(self, db_handler: HandlesDB, *cache_id: _DK, broadcast: bool) -> None:
        self.handler = db_handler
        self.broadcast = broadcast
        self.websocket = db_handler.bot.websocket
        self.cache_id = " ".join(str(id) for id in cache_id)


    async def __aenter__(self):
        await self.handler._cache_lock.wait()
        self.handler._cache_lock.clear()

    async def __aexit__(self, etype, error, tb):
        self.handler._cache_lock.set()
        if error is not None or self.websocket is None or not self.broadcast:
            return

        await self.websocket.send(f"BROADCAST invalidate_cache {self.cache_id}")

class HandlesDB:
    def __init__(self, bot: TTSBot):
        self.bot = bot
        self.pool = bot.pool
        bot.add_listener(self.on_invalidate_cache)

        self._cache: Dict[_DK, Optional[asyncpg.Record]] = {}
        self._cache_lock = asyncio.Event()
        self._cache_lock.set()


    async def fetchrow(self, query: str, id: _DK, *args: Any) -> Optional[asyncpg.Record]:
        await self._cache_lock.wait()
        if id not in self._cache:
            if isinstance(id, tuple):
                self._cache[id] = await self.pool.fetchrow(query, *id)
            else:
                self._cache[id] = await self.pool.fetchrow(query, id, *args)

        return self._cache[id]

    async def on_invalidate_cache(self, *to_invalidate: str):
        if len(to_invalidate) > 1:
            invalidate_id = tuple(int(id) for id in to_invalidate)
        else:
            invalidate_id = int(to_invalidate[0])

        self._cache.pop(invalidate_id, None)

class GeneralSettings(HandlesDB):
    DEFAULT_SETTINGS: asyncio.Task[asyncpg.Record]
    def __init__(self, bot: TTSBot, *args: Any, **kwargs: Any):
        super().__init__(bot, *args, **kwargs)
        self.DEFAULT_SETTINGS = asyncio.create_task( # type: ignore
            bot.pool.fetchrow("SELECT * FROM guilds WHERE guild_id = 0;")
        )


    async def remove(self, guild: Guild):
        await self.pool.execute("DELETE FROM guilds WHERE guild_id = $1;", guild.id)

    async def get(self, guild: Guild, settings: List[str]) -> List[Any]:
        row = await self.fetchrow("SELECT * from guilds WHERE guild_id = $1", guild.id)

        if row:
            return [row[current_setting] for current_setting in settings]

        defaults = await self.DEFAULT_SETTINGS
        return [defaults[setting] for setting in settings]

    async def set(self, guild: Guild, setting: str, value):
        async with CacheWriter(self, guild.id, broadcast=False):
            await self.pool.execute(f"""
                INSERT INTO guilds(guild_id, {setting}) VALUES($1, $2)

                ON CONFLICT (guild_id)
                DO UPDATE SET {setting} = EXCLUDED.{setting};""",
                guild.id, value
            )

class UserInfoHandler(HandlesDB):
    async def get(self, value: str, user: User, default: Return) -> Return:
        row = await self.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", user.id)
        try:
            return row[value] or default # type: ignore
        except (KeyError, TypeError):
            return default

    async def set(self, setting: str, user: User, value: Union[str, bool]) -> None:
        if isinstance(value, str):
            value = value.lower().split("-")[0]

        async with CacheWriter(self, user.id, broadcast=True):
            await self.pool.execute(f"""
                INSERT INTO userinfo(user_id, {setting})
                VALUES($1, $2)

                ON CONFLICT (user_id)
                DO UPDATE SET {setting} = EXCLUDED.{setting};""",
                user.id, value
            )

    async def block(self, user: User) -> None:
        await self.set("blocked", user, True)

    async def unblock(self, user: User) -> None:
        await self.set("blocked", user, False)

class NicknameHandler(HandlesDB):
    async def get(self, guild: Guild, user: User) -> str:
        row = await self.fetchrow("SELECT * FROM nicknames WHERE guild_id = $1 AND user_id = $2", (guild.id, user.id))
        return row["name"] if row else user.display_name

    async def set(self, guild: Guild, user: User, nickname: str) -> None:
        try:
            async with CacheWriter(self, guild.id, user.id, broadcast=False):
                await self.pool.execute("""
                    INSERT INTO nicknames(guild_id, user_id, name)
                    VALUES($1, $2, $3)

                    ON CONFLICT (guild_id, user_id)
                    DO UPDATE SET name = EXCLUDED.name;""",
                    guild.id, user.id, nickname
                )
        except asyncpg.exceptions.ForeignKeyViolationError:
            # Fixes non-existing userinfo and retries inserting into nicknames.
            # Avoids recursion due to user_id being a pkey, so userinfo insert will error if the foreign key error happens twice.
            await self.pool.execute("INSERT INTO userinfo(user_id) VALUES($1);", user.id)
            return await self.set(guild, user, nickname)
