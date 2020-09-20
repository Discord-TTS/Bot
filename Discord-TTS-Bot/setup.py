import configparser
import json
import os

import discord
from discord.ext import commands

try:
  os.mkdir("servers")

  with open("activity.txt", "x") as activity, open("activitytype.txt", "x") as activitytype, open("status.txt", "x") as status:
    activitytype.write("watching")
    activity.write("my owner set me up!")
    status.write("idle")

  with open("blocked_users.json", "x") as blocked_users, open("setlangs.json", "x") as setlangs, open("settings.json", "x") as settings:
    json.dump(list(), blocked_users)
    json.dump(dict(), setlangs)
    json.dump(dict(), settings)
except:
  print("Failed making one of the files! If you are resetting to default, delete the servers folder,  all .txt, .json, and the .ini file before running this again!")
  raise SystemExit

bot = commands.Bot(command_prefix="-")
config = configparser.ConfigParser()

token = input("Input a bot token: ")
main_server = int(input("What is the ID of the main server for your bot? (Suggestions, errors and DMs will be sent here): "))
trusted_ids = input("Input a list of trusted user IDs (allowing for moderation commands such as -(un)block, -dm, -refreshroles, -lookupinfo, and others.): ").split(", ")

config["Main"] = {
  "token": token,
  "main_server": main_server,
  "trusted_ids": trusted_ids,
}

@bot.event
async def on_ready():
    global config
    global logs
    
    guild = bot.get_guild(main_server)
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
