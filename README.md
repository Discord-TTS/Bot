# TTS Bot!

Text to speech Discord Bot using gTTS/voxpopuli and discord.py!

## Setup Guide:
### Easy (Public Bot):
- Invite the bot with [this invite](https://discordapp.com/api/oauth2/authorize?client_id=513423712582762502&permissions=36719617&scope=bot)
- Run -setup #text_channel_to_read_from
- Run -join in that text channel, while being in a voice channel
- Type normally in the setup text channel!

### Normal (Docker):
- Make sure docker, git, and postgresql are installed
- Run `git clone https://github.com/Gnome-py/Discord-TTS-Bot.git`
- Rename `config-example.ini` to `config.ini` and fill it out

- Build the docker container with `docker build -t ttsbot .`
- Run the docker container with `docker run ttsbot` (add `-d` to run in background)
- Now the bot is running in the container, and you can use it!

### Hard (Self Host):
- Make sure python 3.7+, git, postgresql, and ffmpeg are installed
- Run `git clone https://github.com/Gnome-py/Discord-TTS-Bot.git`
- Rename `config-example.ini` to `config.ini` and fill it out

- Run `python3 -m pip install -r requirements.txt` (`python3` may be `py` on windows)
- Run `python3 main.py`
- Now the bot is running in your terminal, and you can use it!
