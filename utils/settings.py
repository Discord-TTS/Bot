import json

from utils.basic import get_value

with open("settings.json") as f:    settings = json.load(f)
with open("setlangs.json") as f:    setlangs = json.load(f)
with open("blocked_users.json") as f:    blocked_users = json.load(f)

default_settings = {"channel": 0, "xsaid": True, "auto_join": False, "bot_ignore": True, "nicknames": dict()}

class settings_class():
    def __init__():
        with open("settings.json") as f:    settings = json.load(f)

    def save():
        with open("settings.json", "w") as f:    json.dump(settings, f)

    def remove(guild):
        settings.pop(str(guild.id), None)

    def cleanup(guild_id_list):
        for guild_id in settings.copy():
            if guild_id not in guild_id_list:
                del settings[guild_id]
                continue

            for key, value in settings[guild_id].copy().items():
                if key not in default_settings or value == default_settings[key]:
                    del settings[guild_id][key]

            if settings[guild_id] == dict():
                del settings[guild_id]

    def get(guild, setting):
        return get_value(settings, str(guild.id), setting, default_value=default_settings[setting])

    def set(guild, setting, value):
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
        def get(guild, user):
            all_nicknames = settings_class.get(guild, "nicknames")
            nickname = get_value(all_nicknames, str(user.id), default_value=user.display_name)

            return nickname

        def set(guild, user, nickname):
            nicknames = settings_class.get(guild, "nicknames")

            user_id = str(user.id)

            if nickname != "" and nickname != user.display_name:
                nicknames[user_id] = nickname
            elif user_id in nicknames:
                del nicknames[user_id]
            else:
                return

            settings_class.set(guild, "nicknames", nicknames)

class setlangs_class():
    def save():
        with open("setlangs.json", "w") as f:    json.dump(setlangs, f)

    def cleanup(user_id_list):
        for user_id in setlangs.copy():
            if user_id not in user_id_list:
                del setlangs[user_id]

    def get(user):
        return get_value(setlangs, str(user.id), default_value="en-us")

    def set(user, value):
        user = str(user.id)
        value = value.lower()

        if value == "en-us" and user in setlangs:
            del setlangs[user]
        else:
            setlangs[user] = value

class blocked_users_class():
    def save():
        with open("blocked_users.json", "w") as f:    json.dump(blocked_users, f)

    def check(user):
        return user.id in blocked_users

    def add(user):
        blocked_users.append(user.id)

    def remove(user):
        blocked_users.remove(user.id)
