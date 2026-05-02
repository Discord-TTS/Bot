# TTS Bot - Rust Rewrite

Text to speech Discord Bot using Serenity, Songbird, and Poise

## Setup Guide:
### Easy (Public Bot):
- Invite the bot with [this invite](https://bit.ly/TTSBotSlash)
- Run /setup channel:(the text channel you wish the bot to read messages from)
- Run /join in that text channel, while being in a voice channel
- Type normally in the setup text channel! and the bot will read your messages aloud with your currently set voice

### Normal (Docker):
- Make sure docker, docker-compose, and git are installed
- Run `git clone https://github.com/Discord-TTS/Bot.git`
- Rename `config-docker.toml` to `config.toml` and fill it out
- Rename `docker-compose-example.yml` to `docker-compose.yml` and fill it out
- Rename `Dockerfile-prod` OR `Dockerfile-dev` to `Dockerfile`
(prod takes longer to build, dev is less efficient to run)

- Build and run the docker containers with `docker-compose up --build -d`
- Check the terminal output with `docker-compose logs bot`
- Now the bot is running in the container, and you can use it!

### Hard (Self Host):
- Make sure rust nightly, cargo, git, postgresql, and ffmpeg are installed
- Run `git clone https://github.com/Discord-TTS/Bot.git`
- Rename `config-selfhost.toml` to `config.toml` and fill it out

- Run `cargo build --release`
- Run the produced exe file in the `/target/release` folder
- Now the bot is running in your terminal, and you can use it!
