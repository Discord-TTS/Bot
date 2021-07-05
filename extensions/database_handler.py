from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any, Dict, List, Optional, Tuple, Union

import asyncpg


if TYPE_CHECKING:
    from discord import Guild
    from discord.abc import User
    from typing_extensions import TypeVar

    from main import TTSBotPremium

    Return = TypeVar("Return")


def setup(bot: TTSBotPremium):
    bot.settings = GeneralSettings(bot.pool)
    bot.userinfo = UserInfoHandler(bot.pool)
    bot.nicknames = NicknameHandler(bot.pool)


_DK = Union[int, Tuple[int, ...]]
class handles_db:
    def __init__(self, pool: asyncpg.Pool[asyncpg.Record]):
        self.pool = pool
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


class GeneralSettings(handles_db):
    DEFAULT_SETTINGS: asyncio.Task[asyncpg.Record]
    def __init__(self, pool: asyncpg.Pool[asyncpg.Record], *args: Any, **kwargs: Any):
        super().__init__(pool, *args, **kwargs)
        self.DEFAULT_SETTINGS = asyncio.create_task( # type: ignore
            pool.fetchrow("SELECT * FROM guilds WHERE guild_id = 0;")
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
        await self._cache_lock.wait()
        self._cache_lock.clear()

        try:
            self._cache.pop(guild.id, None)
            await self.pool.execute(f"""
                INSERT INTO guilds(guild_id, {setting}) VALUES($1, $2)

                ON CONFLICT (guild_id)
                DO UPDATE SET {setting} = EXCLUDED.{setting};""",
                guild.id, value
            )
        finally:
            self._cache_lock.set()

class UserInfoHandler(handles_db):
    async def get(self, value: str, user: User, default: Return) -> Return:
        row = await self.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", user.id)
        return row.get(value, default) if row else default # type: ignore

    async def set(self, setting: str, user: User, value: Union[str, bool]) -> None:
        await self._cache_lock.wait()
        self._cache_lock.clear()

        try:
            self._cache.pop(user.id, None)
            await self.pool.execute(f"""
                INSERT INTO userinfo(user_id, {setting})
                VALUES($1, $2)

                ON CONFLICT (user_id)
                DO UPDATE SET {setting} = EXCLUDED.{setting};""",
                user.id, value
            )
        finally:
            self._cache_lock.set()

    async def block(self, user: User) -> None:
        await self.set("blocked", user, True)

    async def unblock(self, user: User) -> None:
        await self.set("blocked", user, False)

class NicknameHandler(handles_db):
    async def get(self, guild: Guild, user: User) -> str:
        row = await self.fetchrow("SELECT * FROM nicknames WHERE guild_id = $1 AND user_id = $2", (guild.id, user.id))
        return row["name"] if row else user.display_name

    async def set(self, guild: Guild, user: User, nickname: str) -> None:
        await self._cache_lock.wait()
        self._cache_lock.clear()

        try:
            self._cache.pop((guild.id, user.id), None)
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
            self._cache_lock.set()

            return await self.set(guild, user, nickname)
        finally:
            self._cache_lock.set()
