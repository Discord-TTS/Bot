from __future__ import annotations

from hashlib import sha256
from typing import TYPE_CHECKING, Optional

from cryptography.fernet import Fernet


if TYPE_CHECKING:
    from main import TTSBot


def hash_args(func):
    def wrapper(self: CacheHandler, text: str, lang: str, *args, **kwargs):
        return func(self, self.get_hash(text, lang), *args, **kwargs)
    return wrapper


def setup(bot: TTSBot):
    bot.cache = CacheHandler(bot)

class CacheHandler:
    def __init__(self, bot: TTSBot):
        self.key = bot.config["Main"]["key"][2:-1].encode()
        self.fernet = Fernet(self.key)
        self.cache_db = bot.cache_db


    def get_hash(self, *args: str) -> str:
        hashed = sha256(" | ".join(args).encode())
        for _ in range(9):
            hashed = sha256(hashed.digest() + self.key)

        return hashed.hexdigest()

    @hash_args
    async def get(self, search_for: str) -> Optional[bytes]:
        encrypted_mp3: Optional[bytes] = await self.cache_db.get(search_for)
        return self.fernet.decrypt(encrypted_mp3) if encrypted_mp3 else None

    @hash_args
    async def set(self, search_for: str, file: bytes) -> None:
        await self.cache_db.set(search_for, self.fernet.encrypt(file))
