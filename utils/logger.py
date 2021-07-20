import asyncio
import configparser
import logging
from typing import Sequence

import aiohttp
import discord
from discord.ext import tasks

from utils.decos import handle_errors

from logging import WARNING, ERROR

config = configparser.ConfigParser()
config.read("config.ini")

default_avatar_url = "https://cdn.discordapp.com/embed/avatars/{}.png"
unknown_avatar_url = default_avatar_url.format(5)
avatars = {
    logging.INFO: default_avatar_url.format(0),
    logging.DEBUG: default_avatar_url.format(1),
    logging.ERROR: default_avatar_url.format(4),
    logging.WARNING: default_avatar_url.format(3),
}


class WebhookHandler(logging.StreamHandler):
    webhook: discord.Webhook
    def __init__(self, *args, prefix: str, adapters: Sequence[discord.WebhookAdapter], **kwargs):
        super().__init__(*args, **kwargs)
        self.prefix = prefix

        self.normal_logs = discord.Webhook.from_url(url=config["Webhook URLs"]["logs"], adapter=adapters[0])
        self.error_logs = discord.Webhook.from_url(url=config["Webhook URLs"]["errors"], adapter=adapters[1])

    def emit(self, *args, **kwargs):
        raise NotImplementedError

    def webhook_send(self, record: logging.LogRecord):
        msg = self.format(record)

        severity = record.levelno
        if severity >= WARNING:
            msg = f"**{msg}**"

        webhook = self.error_logs if severity >= ERROR else self.normal_logs

        return webhook.send(
            self.prefix + msg,
            username=f"TTS-Webhook [{record.levelname}]",
            avatar_url=avatars.get(record.levelno, unknown_avatar_url),
        )

class SyncWebhookHandler(WebhookHandler):
    def __init__(self, *args, **kwargs):
        adapters = discord.RequestsWebhookAdapter(), discord.RequestsWebhookAdapter()
        super().__init__(*args, adapters=adapters, **kwargs)

    def emit(self, record: logging.LogRecord):
        try:
            self.webhook_send(record)
        except RecursionError:
            raise
        except Exception:
            self.handleError(record)

class AsyncWebhookHandler(WebhookHandler):
    def __init__(self, *args, session: aiohttp.ClientSession, **kwargs):
        adapters = discord.AsyncWebhookAdapter(session), discord.AsyncWebhookAdapter(session)
        super().__init__(*args, adapters=adapters, **kwargs)

        self.loop = asyncio.get_running_loop()
        self.to_be_sent: asyncio.Queue[logging.LogRecord] = asyncio.Queue()

    @tasks.loop()
    @handle_errors
    async def sender_loop(self):
        record = await self.to_be_sent.get()
        await self.webhook_send(record)

    def _emit(self, record: logging.LogRecord) -> None:
        self.to_be_sent.put_nowait(record)
        if not self.sender_loop.is_running():
            self.sender_loop.start()

    def emit(self, record: logging.LogRecord) -> None:
        self.loop.call_soon_threadsafe(self._emit, record)

class CacheDisabledLogger(logging.Logger):
    def is_enabled_for(self, level: int) -> bool:
        return level >= self.getEffectiveLevel()


def setup(aio: bool, level: str, *args, **kwargs) -> CacheDisabledLogger:
    logger = CacheDisabledLogger("TTS Bot")
    logger.setLevel(level.upper())

    handler = AsyncWebhookHandler if aio else SyncWebhookHandler
    logger.addHandler(handler(*args, **kwargs))
    return logger
