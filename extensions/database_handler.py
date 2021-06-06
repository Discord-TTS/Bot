from __future__ import annotations

from asyncio import Event
from typing import TYPE_CHECKING, Any, Dict, List, Tuple, Union, overload

import asyncpg
import discord
from typing_extensions import TypeVar


if TYPE_CHECKING:
    from main import TTSBotPremium


Return = TypeVar("Return")
DEFAULT_SETTINGS: Dict[str, Union[int, bool, str]] = {
    "channel": 0, "msg_length": 30, "repeated_chars": 0,
    "xsaid": True, "auto_join": False, "bot_ignore": True,
    "prefix": "-", "default_lang": "en"
}

def setup(bot: TTSBotPremium):
    bot.settings = GeneralSettings(bot.pool)
    bot.userinfo = UserInfoHandler(bot.pool)
    bot.nicknames = NicknameHandler(bot.pool)


class handles_db:
    def __init__(self, pool: asyncpg.Pool):
        self.pool = pool
        self._cache = dict()

        self._cache_lock = Event()
        self._cache_lock.set()

    async def fetchrow(self, query: str, id: Union[int, Tuple[int, ...]], *args) -> asyncpg.Record:
        await self._cache_lock.wait()
        if id not in self._cache:
            if isinstance(id, tuple):
                self._cache[id] = await self.pool.fetchrow(query, *id)
            else:
                self._cache[id] = await self.pool.fetchrow(query, id, *args)

        return self._cache[id]


class GeneralSettings(handles_db):
    async def remove(self, guild: discord.Guild):
        await self.pool.execute("DELETE FROM guilds WHERE guild_id = $1;", guild.id)

    @overload
    async def get(self, guild: discord.Guild, setting: str) -> Any: ...

    @overload
    async def get(self, guild: discord.Guild, settings: List[str]) -> List[Any]: ...

    @overload
    async def get(self, guild: discord.Guild, setting: str, settings: List[str]) -> List[Any]: ...

    async def get(self, guild: discord.Guild, setting=None, settings=None): # type: ignore
        row = await self.fetchrow("SELECT * from guilds WHERE guild_id = $1", guild.id)

        if setting:
            # Could be cleaned up with `settings=[]` in func define then
            # just settings.append(setting) however that causes weirdness
            settings = settings + [setting] if settings else [setting]

        rets = [row[current_setting] for current_setting in settings] if row else \
               [DEFAULT_SETTINGS[setting] for setting in settings]

        return rets[0] if len(rets) == 1 else rets

    async def set(self, guild: discord.Guild, setting: str, value):
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
    async def get(self, value: str, user: discord.abc.User, default: Return) -> Return:
        row = await self.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", user.id)
        return row.get(value, default) if row else default # type: ignore

    async def set(self, setting: str, user: discord.abc.User, value: Union[str, bool]) -> None:
        await self._cache_lock.wait()
        self._cache_lock.clear()
        if isinstance(value, str):
            value = value.lower().split("-")[0]

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

    async def block(self, user: discord.abc.User) -> None:
        await self.set("blocked", user, True)

    async def unblock(self, user: discord.abc.User) -> None:
        await self.set("blocked", user, False)

class NicknameHandler(handles_db):
    async def get(self, guild: discord.Guild, user: discord.abc.User) -> str:
        row = await self.fetchrow("SELECT * FROM nicknames WHERE guild_id = $1 AND user_id = $2", (guild.id, user.id))
        return row["name"] if row else user.display_name

    async def set(self, guild: discord.Guild, user: discord.abc.User, nickname: str) -> None:
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
