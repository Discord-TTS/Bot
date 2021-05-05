from asyncio import Event
from typing import Optional, Tuple, Union

import asyncpg
import discord


default_settings = {"channel": 0, "msg_length": 30, "repeated_chars": 0, "xsaid": True, "auto_join": False, "bot_ignore": True, "prefix": "-"}

def setup(bot):
    bot.settings = GeneralSettings(bot.pool)
    bot.userinfo = UserInfoHandler(bot.pool)
    bot.nicknames = NicknameHandler(bot.pool)


class handles_db:
    def __init__(self, pool):
        self.pool = pool
        self._cache = dict()

    async def fetchrow(self, query: str, id: Union[int, Tuple[int]], *args) -> asyncpg.Record:
        if id not in self._cache:
            if "nicknames" in query:
                self._cache[id] = await self.pool.fetchrow(query, *id)
            else:
                self._cache[id] = await self.pool.fetchrow(query, id, *args)

        return self._cache[id]


class GeneralSettings(handles_db):
    async def remove(self, guild: discord.Guild):
        await self.pool.execute(f"DELETE FROM guilds WHERE guild_id = $1;", guild.id)

    async def get(self, guild: discord.Guild, setting: Optional[str] = None, settings: Optional[list] = None):
        row = await self.fetchrow("SELECT * from guilds WHERE guild_id = $1", guild.id)

        if setting:
            # Could be cleaned up with `settings=[]` in func define then
            # just settings.append(setting) however that causes weirdness
            settings = settings + setting if settings else [setting]

        rets = [row[current_setting] for current_setting in settings] if row else \
               [default_settings[setting] for setting in settings]

        return rets[0] if len(rets) == 1 else rets

    async def set(self, guild: discord.Guild, setting: str, value):
        self._cache.pop(guild.id, None)
        await self.pool.execute(f"""
            INSERT INTO guilds(guild_id, {setting}) VALUES($1, $2)

            ON CONFLICT (guild_id)
            DO UPDATE SET {setting} = EXCLUDED.{setting};""",
            guild.id, value
        )

class UserInfoHandler(handles_db):
    async def get(self, value: str, user: discord.User, default: Union[str, bool] = "en") -> Union[str, bool]:
        row = await self.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", user.id)
        return row[value] if row else default

    async def set(self, setting: str, user: discord.User, value: Union[str, bool]) -> None:
        if isinstance(value, str):
            value = value.lower().split("-")[0]

        await self.pool.execute(f"""
            INSERT INTO userinfo(user_id, {setting})
            VALUES($1, $2)

            ON CONFLICT (user_id)
            DO UPDATE SET {setting} = EXCLUDED.{setting};""",
            user.id, value
        )

    async def block(self, user: discord.User) -> None:
        await self.set("blocked", user, True)

    async def unblock(self, user: discord.User) -> None:
        await self.set("blocked", user, False)

class NicknameHandler(handles_db):
    async def get(self, guild: discord.Guild, user: discord.User) -> str:
        row = await self.fetchrow("SELECT * FROM nicknames WHERE guild_id = $1 AND user_id = $2", (guild.id, user.id))
        return row["name"] if row else user.display_name

    async def set(self, guild: discord.Guild, user: discord.User, nickname: str) -> None:
        self._cache.pop((guild.id, user.id), None)

        try:
            await self.pool.execute(f"""
                INSERT INTO nicknames(guild_id, user_id, name)
                VALUES($1, $2, $3)

                ON CONFLICT (guild_id, user_id)
                DO UPDATE SET name = EXCLUDED.name;""",
                guild.id, user.id, nickname
            )
        except asyncpg.ForeignKeyViolationError:
            # Fixes non-existing userinfo and retries inserting into nicknames.
            # Avoids recursion due to user_id being a pkey, so userinfo insert will error if the foreign key error happens twice.
            await self.pool.execute("INSERT INTO userinfo(user_id) VALUES($1);", user.id)
            return await self.set(guild, user, nickname)
