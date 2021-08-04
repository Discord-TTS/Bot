import asyncio
import configparser
import logging
from logging import ERROR, WARNING
from typing import Dict, List, Union

import aiohttp
import discord
from discord.ext import tasks

from .constants import DEFAULT_AVATAR_URL

config = configparser.ConfigParser()
config.read("config.ini")

unknown_avatar_url = DEFAULT_AVATAR_URL.format(5)
avatars = {
    logging.INFO: DEFAULT_AVATAR_URL.format(0),
    logging.DEBUG: DEFAULT_AVATAR_URL.format(1),
    logging.ERROR: DEFAULT_AVATAR_URL.format(4),
    logging.WARNING: DEFAULT_AVATAR_URL.format(3),
}

class CacheFixedLogger(logging.Logger):
    _cache: Dict[int, bool]
    def setLevel(self, level: Union[int, str]) -> None:
        self.level = logging._checkLevel(level) # type: ignore
        self._cache.clear()

class WebhookHandler(logging.StreamHandler):
    webhook: discord.Webhook
    def __init__(self, prefix: str, session: aiohttp.ClientSession, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.prefix = prefix

        self.loop = asyncio.get_running_loop()
        self.to_be_sent: Dict[int, List[str]] = {}

        self.normal_logs = discord.Webhook.from_url(url=config["Webhook URLs"]["logs"], session=session)
        self.error_logs = discord.Webhook.from_url(url=config["Webhook URLs"]["errors"], session=session)


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

    @tasks.loop(seconds=1)
    async def sender_loop(self) -> None:
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

    def close(self):
        self.sender_loop.cancel()


def setup(level: str, prefix: str, session: aiohttp.ClientSession) -> CacheFixedLogger:
    logger = CacheFixedLogger("TTS Bot")
    logger.setLevel(level.upper())
    logger.addHandler(WebhookHandler(prefix, session))
    return logger
