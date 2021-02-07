# TTS Bot!

Text to speech Discord Bot using gTTS and discord.py!

## Setup Guide:
### Easy:
- Invite the bot with [this invite](https://discordapp.com/api/oauth2/authorize?client_id=513423712582762502&permissions=36719617&scope=bot)
- Run -setup #text_channel_to_read_from
- Run -join in that text channel, while being in a voice channel
- Type normally in the setup text channel!

### Hard (Self Host):
- Make sure you have python 3.5 or above installed (tested with 3.8, should work with 3.5) and a postgresql database ready
- Make a bot account with [the Discord Developer Portal](https://discord.com/developers/applications/) and note down the token
- Toggle the `Server Members Intent` toggle in the portal
- Make sure you have a Discord server ready to be setup as a hub for TTS Bot
- Run `git clone --recurse-submodules https://github.com/Gnome-py/Discord-TTS-Bot.git`
- Run `python -m pip install -r requirements.txt`
- Run `python setup.py` and follow the instructions
- Run `python main.py`, and you should have your own instance of TTS Bot running!

## Variable Explaination:

### `bot.queue[guild_id]`:
- Dictionary of message_id: bytes objects of gTTS output

### `bot.trusted`:
- List of trusted people, stored in the config.ini["Main"]["trusted_ids"]

### `bot.channels`:
- Stores commonly used channels, dictionary of channel_name: channel_object

### `bot.supportserver`:
- Cached guild object for the support server, should contain the bot.channels
