import asyncio
import json
from configparser import ConfigParser
from os import rename

import asyncpg

config = ConfigParser()
config.read("config.ini")

with open("settings.json") as f:
    settings_json = json.load(f)
with open("setlangs.json") as f:
    setlangs_json = json.load(f)

async def run():
    conn = await asyncpg.connect(
        user=config["PostgreSQL Info"]["name"],
        password=config["PostgreSQL Info"]["pass"],
        host=config["PostgreSQL Info"]["ip"],
        )

    async with conn.transaction():
        await conn.execute("""
            CREATE TABLE guilds (
                guild_id       text PRIMARY KEY,
                channel        text DEFAULT 0,
                xsaid          bool DEFAULT True,
                bot_ignore     bool DEFAULT True,
                auto_join      bool DEFAULT False,
                msg_length     text DEFAULT 30,
                repeated_chars text DEFAULT 0
            );
            CREATE TABLE nicknames (
                guild_id text,
                user_id  text,
                name     text
            );
            CREATE TABLE userinfo (
                user_id  text PRIMARY KEY,
                lang     text DEFAULT 'en-us',
                blocked  bool DEFAULT False
            );
            CREATE TABLE cache_lookup (
                message       BYTEA PRIMARY KEY,
                message_id    text  UNIQUE
            );""")

        for guild, settings in settings_json.items():
            await conn.execute("INSERT INTO guilds(guild_id) VALUES($1);", guild)
            for setting, value in settings.items():
                if setting == "channel":
                    await conn.execute("UPDATE guilds SET channel = $1 WHERE guild_id = $2;", str(value), guild)
                elif setting in ("xsaid", "auto_join", "bot_ignore"):
                    await conn.execute(f"UPDATE guilds SET {setting} = $1 WHERE guild_id = $2;", value, guild)
                elif setting == "limits":
                    for limit, limit_value in value.items():
                        await conn.execute(f"UPDATE guilds SET {limit} = $1 WHERE guild_id = $2;", limit_value, guild)
                elif setting == "nicknames":
                    for id, nickname in value.items():
                        await conn.execute("INSERT INTO nicknames(guild_id, user_id, name) VALUES($1, $2, $3)", guild, id, nickname)

                else:
                    print(f"wtf is {setting}: {value}")
        for user_id, lang in setlangs_json.items():
            await conn.execute("INSERT INTO userinfo(user_id, lang) VALUES($1, $2)", user_id, lang)

    for f in ("settings.json", "setlangs.json"):
        rename(f, f"{f}.bak")
    print("Done!")
loop = asyncio.get_event_loop()
loop.run_until_complete(run())
