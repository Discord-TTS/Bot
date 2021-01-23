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
            message    BYTEA PRIMARY KEY,
            message_id text  UNIQUE
        );""")

    botcategory = await guild.create_category("TTS Bot")
    overwrites = {guild.default_role: discord.PermissionOverwrite(read_messages=False, send_messages=False)}

    errors = await guild.create_text_channel("errors", category=botcategory, overwrites=overwrites)
    dm_logs = await guild.create_text_channel("dm-logs", category=botcategory, overwrites=overwrites)
    servers = await guild.create_text_channel("servers", category=botcategory, overwrites=overwrites)
    suggestions = await guild.create_text_channel("suggestions", category=botcategory, overwrites=overwrites)
    logs = await guild.create_text_channel("logs", category=botcategory, overwrites=overwrites)

    config["Channels"] = {
        "errors": errors.id,
        "dm_logs": dm_logs.id,
        "servers": servers.id,
        "suggestions": suggestions.id,
        "logs": logs.id
    }

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
