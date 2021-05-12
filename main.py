import asyncio
import traceback
from concurrent.futures import ProcessPoolExecutor
from configparser import ConfigParser
from os import listdir
from time import monotonic
from typing import Union

import aiohttp
import asyncgTTS
import asyncpg
import discord
from discord.ext import commands

from utils.basic import remove_chars
from utils.decos import wrap_with

print("Starting TTS Bot!")
start_time = monotonic()

# Read config file
config = ConfigParser()
config.read("config.ini")

# Setup activity and intents for logging in
activity = discord.Activity(name=config["Activity"]["name"], type=getattr(discord.ActivityType, config["Activity"]["type"]))
intents = discord.Intents(voice_states=True, messages=True, guilds=True, members=True)
status = getattr(discord.Status, config["Activity"]["status"])

# Custom prefix support
async def prefix(bot: commands.AutoShardedBot, message: discord.Message) -> str:
    "Gets the prefix for a guild based on the passed message object"
    return await bot.settings.get(message.guild, "prefix") if message.guild else "-"

class TTSBot(commands.AutoShardedBot):
    def __init__(self, config, session, executor, *args, **kwargs):
        self.channels = {}
        self.config = config
        self.session = session
        self.executor = executor
        self.sent_fallback = False

        self.trusted = remove_chars(config["Main"]["trusted_ids"], "[", "]", "'").split(", ")

        super().__init__(*args, **kwargs)

    @property
    def support_server(self):
        return self.get_guild(int(self.config["Main"]["main_server"]))


    async def check_gtts(self) -> Union[bool, Exception]:
        try:
            await self.gtts.get(text="RL Test", lang="en")
            return True
        except asyncgTTS.RatelimitException:
            return False
        except Exception as e:
            return e


    def load_extensions(self, folder):
        filered_exts = filter(lambda e: e.endswith(".py"), listdir(folder))
        for ext in filered_exts:
            self.load_extension(f"{folder}.{ext[:-3]}")

    async def start(self, *args, token, **kwargs):
        # Get everything ready in async env
        db_info = self.config["PostgreSQL Info"]
        self.gtts, self.pool = await asyncio.gather(
            asyncgTTS.setup(
                premium=False,
                session=self.session
            ),
            asyncpg.create_pool(
                host=db_info["ip"],
                user=db_info["name"],
                database=db_info["db"],
                password=db_info["pass"]
            )
        )

        # Fill up bot.channels, as a load of webhooks
        for channel_name, webhook_url in self.config["Channels"].items():
            self.channels[channel_name] = discord.Webhook.from_url(
                url=webhook_url,
                adapter=discord.AsyncWebhookAdapter(session=self.session)
            )

        # Load all of /cogs and /extensions
        self.load_extensions("cogs")
        self.load_extensions("extensions")

        # Send starting message and actually start the bot
        await self.channels["logs"].send("Starting TTS Bot!")
        await super().start(token, *args, **kwargs)


def get_error_string(e: BaseException) -> str:
    return f"{type(e).__name__}: {e}"

@wrap_with(ProcessPoolExecutor,   aenter=False)
@wrap_with(aiohttp.ClientSession, aenter=True)
async def main(session, executor):
    bot = TTSBot(
        config=config,
        status=status,
        intents=intents,
        session=session,
        executor=executor,
        help_command=None, # Replaced by FancyHelpCommand by FancyHelpCommandCog
        activity=activity,
        command_prefix=prefix,
        case_insensitive=True,
        chunk_guilds_at_startup=False,
        allowed_mentions=discord.AllowedMentions(everyone=False, roles=False)
    )

    try:
        print("\nLogging into Discord...")
        ready_task = asyncio.create_task(bot.wait_until_ready())
        bot_task = asyncio.create_task(bot.start(token=config["Main"]["Token"]))

        done, pending = await asyncio.wait((bot_task, ready_task), return_when=asyncio.FIRST_COMPLETED)
        if bot_task in done:
            raise RuntimeError(f"Bot Shutdown before ready: {get_error_string(bot_task.exception())}")

        print(f"Logged in as {bot.user} and ready!")
        await bot.channels["logs"].send(f"Started and ready! Took `{monotonic() - start_time:.2f} seconds`")
        await bot_task
    except Exception as e:
        print(get_error_string(e))
    finally:
        if not bot.user:
            return

        await bot.channels["logs"].send(f"{bot.user.mention} is shutting down.")
        await bot.close()

try:
    asyncio.run(main())
except (KeyboardInterrupt, RuntimeError) as e:
    print(f"Shutdown forcefully: {get_error_string(e)}")
