import configparser
from os import mkdir
from getpass import getpass

import asyncpg
import discord
from cryptography.fernet import Fernet
from discord.ext import commands

bot = commands.Bot(command_prefix="-", intents=discord.Intents(guilds=True, members=True, messages=True))
config = configparser.ConfigParser()

psql_name = input("What is the username to sign into your PostgreSQL DB: ")
psql_pass = getpass("What is the password to sign into your PostgreSQL DB: ")
psql_db = input("Which database do you want to connect to (blank = default): ")
psql_ip = input("What is the IP for the PostgreSQL DB (blank = 127.0.0.1): ")
if psql_ip == "":
    psql_ip = "127.0.0.1"

token = getpass("Input a bot token: ")
main_server = int(input("What is the ID of the main server for your bot? (Suggestions, errors and DMs will be sent here): "))
trusted_ids = input("Input a list of trusted user IDs (allowing for moderation commands such as -(un)block, -dm, -refreshroles, -lookupinfo, and others.): ").split(", ")

mkdir("cache")

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


@bot.event
async def on_ready():
    global config
    global logs
    guild = bot.get_guild(main_server)

    conn = await asyncpg.connect(
        user=psql_name,
        password=psql_pass,
        database=psql_db,
        host=psql_ip
    )

    await conn.execute("""
        CREATE TABLE guilds (
            guild_id       bigint     PRIMARY KEY,
            channel        bigint     DEFAULT 0,
            xsaid          bool       DEFAULT True,
            bot_ignore     bool       DEFAULT True,
            auto_join      bool       DEFAULT False,
            msg_length     smallint   DEFAULT 30,
            repeated_chars smallint   DEFAULT 0,
            prefix         varchar(6) DEFAULT '-'
        );
        CREATE TABLE userinfo (
            user_id  bigint     PRIMARY KEY,
            blocked  bool       DEFAULT False,
            lang     varchar(4) DEFAULT 'en'
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

    botcategory = await guild.create_category("TTS Bot")
    overwrites = {guild.default_role: discord.PermissionOverwrite(read_messages=False, send_messages=False)}

    config["Channels"] = {}
    avatar_bytes = await bot.user.avatar_url.read()

    for channel_name in ("errors", "dm-logs", "servers", "suggestions", "logs"):
        channel = await guild.create_text_channel(channel_name, category=botcategory, overwrites=overwrites)
        webhook = await channel.create_webhook(name=bot.user.name, avatar=avatar_bytes).url

        config["Channels"][channel_name] = webhook

    await logs.send(f"Are you sure you want {[str(bot.get_user(int(trusted_id))) for trusted_id in trusted_ids]} to be trusted? (do -yes to accept)")


@bot.command()
@commands.is_owner()
async def yes(ctx):
    with open("config.ini", "x") as configfile:
        config.write(configfile)

    await logs.send("Finished and written to config.ini, change the names of the channels all you want and now TTS Bot should be startable!")
    await bot.close()


@bot.command()
@commands.is_owner()
async def no(ctx):
    await bot.close()

bot.run(token)
