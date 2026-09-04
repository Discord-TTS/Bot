# TTS Bot - Rust Rewrite

Text to speech Discord Bot using Serenity, Songbird, and Poise

## Setup Guide:
### Easy (Public Bot):
- Invite the bot with [this invite](https://bit.ly/TTSBotSlash)
- Run -setup #text_channel_to_read_from
- Run -join in that text channel, while being in a voice channel
- Type normally in the setup text channel!

### Normal (Docker):
- Make sure docker and git are installed
- Run `git clone https://github.com/Discord-TTS/Bot.git`
- Rename `config-docker.toml` to `config.toml` and fill it out
- Rename `docker-compose-example.yml` to `docker-compose.yml` and fill it out
- Rename `.env.example` to `.env` and fill it out (Polly voices, which are currently required, need `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and `AWS_REGION`)
- Place your [GCP credentials file](https://docs.cloud.google.com/iam/docs/keys-create-delete#creating) in the project root as `gcp.json` (gCloud voices currently require this)
> If you do not supply the required credentials, the bot will not start
- Run the containers with `docker compose up -d`
- Check the terminal output with `docker compose logs bot`
- Now the bot is running in the container, and you can use it!

### Hard (Self Host):
- Make sure rust nightly, cargo, git, postgresql, and ffmpeg are installed
- Run `git clone https://github.com/Discord-TTS/Bot.git`
- Rename `config-selfhost.toml` to `config.toml` and fill it out
- Run `cargo build --release`
- Run the produced exe file in the `/target/release` folder
- Now the bot is running in your terminal, and you can use it!