import asyncio
import configparser
import logging
from typing import Dict, List, Sequence, Tuple, Union

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

    def webhook_send(self, severity: int, *messages: str):
        severity_name = logging.getLevelName(severity)
        webhook = self.error_logs if severity >= ERROR else self.normal_logs

        message = ""
        for line in messages:
            if severity >= WARNING:
                line = f"**{line}**"

            message += f"{self.prefix}{line}\n"

        return webhook.send(
            content=message,
            username=f"TTS-Webhook [{severity_name}]",
            avatar_url=avatars.get(severity, unknown_avatar_url),
        )

class SyncWebhookHandler(WebhookHandler):
    def __init__(self, *args, **kwargs):
        adapters = discord.RequestsWebhookAdapter(), discord.RequestsWebhookAdapter()
        super().__init__(*args, adapters=adapters, **kwargs)

    def emit(self, record: logging.LogRecord):
        try:
            self.webhook_send(record.levelno, self.format(record))
        except RecursionError:
            raise
        except Exception:
            self.handleError(record)

class AsyncWebhookHandler(WebhookHandler):
    def __init__(self, *args, session: aiohttp.ClientSession, **kwargs):
        adapters = discord.AsyncWebhookAdapter(session), discord.AsyncWebhookAdapter(session)
        super().__init__(*args, adapters=adapters, **kwargs)

        self.to_be_sent: Dict[int, List[str]] = {}
        self.loop = asyncio.get_running_loop()

    @tasks.loop(seconds=1)
    async def sender_loop(self):
        for severity in self.to_be_sent.copy().keys():
            msgs = self.to_be_sent.pop(severity)
            try:
                await self.webhook_send(severity, *msgs)
            except RuntimeError:
                return self.sender_loop.stop()

    def _emit(self, record: logging.LogRecord) -> None:
        msg = self.format(record)
        if record.levelno not in self.to_be_sent:
            self.to_be_sent[record.levelno] = [msg]
        else:
            self.to_be_sent[record.levelno].append(msg)

        if not self.sender_loop.is_running():
            self.sender_loop.start()

    def emit(self, record: logging.LogRecord) -> None:
        self.loop.call_soon_threadsafe(self._emit, record)

class CacheFixedLogger(logging.Logger):
    _cache: Dict[int, bool]
    def setLevel(self, level: Union[int, str]) -> None:
        self.level = logging.getLevelName(level)
        self._cache.clear()

def setup(aio: bool, level: str, *args, **kwargs) -> CacheFixedLogger:
    logger = CacheFixedLogger("TTS Bot")
    logger.setLevel(level.upper())

    handler = AsyncWebhookHandler if aio else SyncWebhookHandler
    logger.addHandler(handler(*args, **kwargs))
    return logger
