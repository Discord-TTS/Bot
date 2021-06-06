import asyncio
import configparser
from getpass import getpass
from os import mkdir
from typing import Optional

import asyncpg
import discord
from cryptography.fernet import Fernet


psql_name = input("What is the username to sign into your PostgreSQL DB: ")
psql_pass = getpass("What is the password to sign into your PostgreSQL DB: ")
psql_db = input("Which database do you want to connect to (blank = default): ")
psql_ip = input("What is the IP for the PostgreSQL DB (blank = 127.0.0.1): ")
if psql_ip == "":
    psql_ip = "127.0.0.1"


token = getpass("Input a bot token: ")
owner_id = int(input("What is your Discord User ID: "))
main_server = int(input("What is the ID of the main server for your bot? (Suggestions, errors and DMs will be sent here): "))
trusted_ids = input("Input a list of trusted user IDs (allowing for moderation commands such as -(un)block, -dm, -refreshroles, -lookupinfo, and others.): ").split(", ")

mkdir("cache")

config = configparser.ConfigParser()
cache_key = Fernet.generate_key()
config["Main"] = {
    "token": token,
    "key": cache_key,
    "main_server": main_server,
    "trusted_ids": trusted_ids,
}
config["Activity"] = {
    "name": "my owner set me up!",
    "type": "watching",
    "status": "idle",
}
config["PostgreSQL Info"] = {
    "name": psql_name,
    "pass": psql_pass,
    "ip": psql_ip,
    "db": psql_db
}
with open("patreon_users.json", "w") as f:
    f.write("{}")

def yay_or_nae(message: discord.Message) -> Optional[bool]:
    if message.content == "-no":
        should_close.set()
    else:
        return message.content == "-yes" and message.author.id == owner_id

async def setup_db() -> str:
    conn = await asyncpg.connect(
        user=psql_name,
        password=psql_pass,
        database=psql_db,
        host=psql_ip
    )

    return await conn.execute("""
        CREATE TABLE guilds (
            guild_id       bigint     PRIMARY KEY,
            channel        bigint     DEFAULT 0,
            xsaid          bool       DEFAULT True,
            bot_ignore     bool       DEFAULT True,
            auto_join      bool       DEFAULT False,
            msg_length     smallint   DEFAULT 30,
            repeated_chars smallint   DEFAULT 0,
            prefix         varchar(6) DEFAULT 'p-',
            default_lang   varchar(3)
        );
        CREATE TABLE userinfo (
            user_id  bigint     PRIMARY KEY,
            blocked  bool       DEFAULT False,
            lang     varchar(4),
            variant  varchar(1)
        );
        CREATE TABLE nicknames (
            guild_id bigint,
            user_id  bigint,
            name     text,

            PRIMARY KEY (guild_id, user_id),

            FOREIGN KEY       (guild_id)
            REFERENCES guilds (guild_id)
            ON DELETE CASCADE,

            FOREIGN KEY         (user_id)
            REFERENCES userinfo (user_id)
            ON DELETE CASCADE
        );
        CREATE TABLE cache_lookup (
            message    BYTEA  PRIMARY KEY,
            message_id bigint UNIQUE NOT NULL
        );""")


async def main() -> None:
    global should_close

    intents = discord.Intents(guilds=True, members=True, messages=True)
    client = discord.Client(intents=intents, chunk_guilds_at_startup=False)

    asyncio.create_task(client.start(token))
    await asyncio.gather(client.wait_until_ready(), setup_db())


    logs_channel = None
    guild = client.get_guild(main_server)
    botcategory = await guild.create_category("TTS Bot")
    overwrites = {guild.default_role: discord.PermissionOverwrite(read_messages=False)}

    config["Channels"] = {}
    avatar_bytes = await client.user.avatar_url.read()

    for channel_name in ("errors", "dm-logs", "servers", "suggestions", "logs"):
        channel = await guild.create_text_channel(channel_name, category=botcategory, overwrites=overwrites) # type: ignore
        webhook = (await channel.create_webhook(name=client.user.name, avatar=avatar_bytes)).url

        if channel_name == "logs":
            logs_channel = channel

        config["Channels"][channel_name] = webhook


    trusted_users = ", ".join(str(await client.fetch_user(int(trusted_id))) for trusted_id in trusted_ids)
    await logs_channel.send(f"Are you sure you want {trusted_users} to be trusted? (do -yes to accept)")

    should_close = asyncio.Event()
    await client.wait_for("message", check=yay_or_nae) # type: ignore
    if should_close.is_set():
        return await client.close()

    with open("config.ini", "x") as configfile:
        config.write(configfile)

    await logs_channel.send("Finished and written to config.ini, change the names of the channels all you want and now TTS Bot should be startable!")
    await client.close()

asyncio.run(main())
