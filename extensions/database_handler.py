from asyncio import Event
from typing import Optional

import discord

default_settings = {"channel": 0, "msg_length": 30, "repeated_chars": 0, "xsaid": True, "auto_join": False, "bot_ignore": True, "prefix": "-"}


def setup(bot):
    bot.settings = GeneralSettings(bot.pool)
    bot.setlangs = LanguageHandler(bot.pool)
    bot.nicknames = NicknameHandler(bot.pool)
    bot.blocked_users = DMBlockHandler(bot.pool)        

class handles_db:
    def __init__(self, pool):
        self.pool = pool

class GeneralSettings(handles_db):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self._cache = dict()

    async def remove(self, guild: discord.Guild):
        await self.pool.execute(f"DELETE FROM guilds WHERE guild_id = $1;", guild.id)

    async def get(self, guild: discord.Guild, setting: Optional[str] = None, settings: Optional[list] = None):
        row = self._cache.get(guild.id)
        if not row:
            row = await self.pool.fetchrow("SELECT * FROM guilds WHERE guild_id = $1", guild.id)
            self._cache[guild.id] = row

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


class NicknameHandler(handles_db):
    async def get(self, guild: discord.Guild, user: discord.User) -> str:
        row = await self.pool.fetchrow("SELECT name FROM nicknames WHERE guild_id = $1 AND user_id = $2", guild.id, user.id)
        return row["name"] if row else user.display_name

    async def set(self, guild: discord.Guild, user: discord.User, nickname: str) -> str:
        await self.pool.execute(f"""
            INSERT INTO nicknames(guild_id, user_id, name)
            VALUES($1, $2, $3)

            ON CONFLICT (guild_id, user_id)
            DO UPDATE SET name = EXCLUDED.name;""",
            guild.id, user.id, nickname
        )


class LanguageHandler(handles_db):
    async def get(self, user: discord.User) -> str:
        row = await self.pool.fetchrow("SELECT lang FROM userinfo WHERE user_id = $1", user.id)
        return row["lang"].split("-")[0] if row else "en"

    async def set(self, user: discord.User, lang: str) -> None:
        lang = lang.lower().split("-")[0]
        await self.pool.execute("""
            INSERT INTO userinfo(user_id, lang)
            VALUES($1, $2)

            ON CONFLICT (user_id)
            DO UPDATE SET lang = EXCLUDED.lang;""",
            user.id, lang
        )


class DMBlockHandler(handles_db):
    async def change(self, user: discord.User, value: bool) -> None:
        await self.pool.execute("""
            INSERT INTO userinfo(user_id, blocked)
            VALUES($1, $2)

            ON CONFLICT (user_id)
            DO UPDATE SET blocked = EXCLUDED.blocked;""",
            user.id, value
        )

    async def check(self, user: discord.User) -> bool:
        row = await self.pool.fetchrow("SELECT blocked FROM userinfo WHERE user_id = $1", user.id)
        return row["blocked"] if row else False

    async def add(self, user: discord.User) -> None:
        await self.change(user, True)

    async def remove(self, user: discord.User) -> None:
        await self.change(user, False)
