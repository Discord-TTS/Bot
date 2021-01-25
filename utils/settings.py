from asyncio import Lock

default_settings = {"channel": 0, "msg_length": 30, "repeated_chars": 0, "xsaid": True, "auto_join": False, "bot_ignore": True, "prefix": "l-"}


class settings_class():
    def __init__(self, pool):
        self.pool = pool
        self._cache = dict()
        self._cache_lock = Lock()

    async def remove(self, guild):
        async with self.pool.acquire() as conn:
            await conn.execute(f"""
                DELETE FROM guilds WHERE guild_id = '{guild.id}';
                DELETE FROM nicknames WHERE guild_id = '{guild.id}';
                """)

    async def get(self, guild, setting=None, settings=None):
        if setting == "prefix":
            return "l-"

        async with self._cache_lock:
            row = self._cache.get(guild.id)

            if not row:
                async with self.pool.acquire() as conn:
                    row = await conn.fetchrow("SELECT * FROM guilds WHERE guild_id = $1", str(guild.id))

                self._cache[guild.id] = row

        if not settings:
            if row is None or dict(row)[setting] is None:
                return default_settings[setting]

            return dict(row)[setting]

        if row is None:
            return [default_settings[setting] for setting in settings]

        row_dict = dict(row)
        settings_values = list()

        for setting in settings:
            if not setting:
                setting = default_settings[setting]
            if setting == "prefix":
                row_dict[setting] = "l-"

            settings_values.append(row_dict[setting])

        return settings_values

    async def set(self, guild, setting, value):
        guild_id = str(guild.id)
        async with self._cache_lock:
            row = self._cache.pop(guild.id, None)
            async with self.pool.acquire() as conn:
                if row is not None:
                    if value == default_settings[setting] and setting in dict(row):
                        return await conn.execute(f"""
                            UPDATE guilds
                            SET {setting} = $1
                            WHERE guild_id = $2;""",
                            default_settings[setting], guild_id
                            )

                    if dict(row) == dict():
                        return await conn.execute(
                            "DELETE * FROM guilds WHERE guild_id = $1;",
                            guild_id
                        )

                    await conn.execute(f"""
                            UPDATE guilds
                            SET {setting} = $1
                            WHERE guild_id = $2;""",
                            value, guild_id
                                        )
                else:
                    await conn.execute(f"""
                        INSERT INTO guilds(guild_id, {setting})
                        VALUES ($1, $2);
                        """, guild_id, value)

class nickname_class():
    def __init__(self, pool):
        self.pool = pool

    async def get(self, guild, user):
        async with self.pool.acquire() as conn:
            row = await conn.fetchrow("SELECT * FROM nicknames WHERE guild_id = $1 AND user_id = $2", str(guild.id), str(user.id))

        if row is None or dict(row)["name"] is None:
            return user.display_name

        return dict(row)["name"]

    async def set(self, guild, user, nickname):
        guild = str(guild.id)
        user_id = str(user.id)

        async with self.pool.acquire() as conn:
            existing = await conn.fetchrow("""
                SELECT * FROM nicknames
                WHERE guild_id = $1 AND user_id = $2""",
                guild, user_id
                ) is not None

            if not nickname or nickname == user.display_name:
                await conn.execute("""
                    DELETE FROM nicknames
                    WHERE guild_id = $1 AND user_id = $2;
                    """, guild, user_id
                                   )
            elif existing:
                await conn.execute("""
                    UPDATE nicknames
                    SET name = $1
                    WHERE guild_id = $2 AND user_id = $3;
                    """, nickname, guild, user_id
                                   )
            else:
                await conn.execute("""
                    INSERT INTO nicknames(guild_id, user_id, name)
                    VALUES ($1, $2, $3);
                    """, guild, user_id, nickname
                                   )


class setlangs_class():
    def __init__(self, pool):
        self.pool = pool

    async def get(self, user):
        async with self.pool.acquire() as conn:
            row = await conn.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", str(user.id))

        if row is None or dict(row)["lang"] is None:
            return "en-us"

        return dict(row)["lang"]

    async def set(self, user, lang):
        user = str(user.id)
        lang = lang.lower()
        async with self.pool.acquire() as conn:
            userinfo = await conn.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", user)
            existing = userinfo is not None

            if lang == "en-us" and existing and not dict(userinfo)["blocked"]:
                await conn.execute("""
                    DELETE FROM userinfo
                    WHERE user_id = $1;
                    """, user)
            elif existing:
                await conn.execute("""
                    UPDATE userinfo
                    SET lang = $1
                    WHERE user_id = $2;
                    """, lang, user)
            else:
                await conn.execute("""
                    INSERT INTO userinfo(user_id, lang)
                    VALUES ($1, $2);
                    """, user, lang)


class blocked_users_class():
    def __init__(self, pool):
        self.pool = pool

    async def change(self, user, value):
        user = str(user.id)
        async with self.pool.acquire() as conn:
            row = await conn.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", user)

            if row is None:
                await conn.execute("""
                    INSERT INTO userinfo(user_id, blocked)
                    VALUES ($1, $2);
                    """, user, value)
            else:
                await conn.execute("""
                    UPDATE userinfo
                    SET blocked = $1
                    WHERE user_id = $2
                    """, value, user)

    async def check(self, user):
        async with self.pool.acquire() as conn:
            row = await conn.fetchrow("SELECT * FROM userinfo WHERE user_id = $1", str(user.id))

        if row is None:
            return False

        return dict(row)["blocked"]

    async def add(self, user):
        await self.change(user, True)

    async def remove(self, user):
        await self.change(user, False)
