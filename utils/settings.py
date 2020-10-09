import asyncio
import json

from utils.basic import get_value

with open("settings.json") as f:    settings = json.load(f)
default_settings = {"channel": 0, "xsaid": True, "auto_join": False, "bot_ignore": True, "nicknames": dict()}

def save():
    with open("settings.json", "w") as f:    json.dump(settings, f)

def remove(guild):
    settings.pop(str(guild.id), None)

def cleanup():
    for guild_id in settings.copy():
        if guild_id not in guild_id_list:
            del settings[guild_id]
            continue

        for key, value in settings[guild_id].copy().items():
            if key not in default_settings or value == default_settings[key]:
                del settings[guild_id][key]

        if settings[guild_id] == dict():
            del settings[guild_id]

async def get(guild, setting):
    return get_value(settings, str(guild.id), setting, default_value=default_settings[setting])

async def set(guild, setting, value):
    guild = str(guild.id)

    if guild in settings:
        if value == default_settings[setting] and setting in settings[guild]:
            del settings[guild][setting]
            return

        if settings[guild] == dict():
            del settings[guild]
            return
    else:
        settings[guild] = dict()

    settings[guild][setting] = value

class nickname():
    async def get(guild, user):
        all_nicknames = await get(guild, "nicknames")
        nickname = get_value(all_nicknames, str(user.id), default_value=user.display_name)

        return nickname

    async def set(guild, user, nickname):
        nicknames = await get(guild, "nicknames")

        user_id = str(user.id)

        if nickname != "" and nickname != user.display_name:
            nicknames[user_id] = nickname
        elif user_id in nicknames:
            del nicknames[user_id]
        else:
            return

        await set(guild, "nicknames", nicknames)
