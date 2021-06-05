from __future__ import annotations

import asyncio
from hashlib import sha256
from os import rename
from os.path import exists
from typing import TYPE_CHECKING, Iterable, Optional

from cryptography.fernet import Fernet

from utils.decos import run_in_executor


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.cache = cache(bot)

class cache:
    def __init__(self, bot: TTSBot):
        self.bot = bot
        self.pool = bot.pool

        self.key = bot.config["Main"]["key"][2:-1].encode()
        self.fernet = Fernet(self.key)


    def get_hash(self, to_hash: bytes) -> bytes:
        hashed = sha256(to_hash)
        for _ in range(9):
            hashed = sha256(hashed.digest() + self.key)

        return hashed.digest()

    @run_in_executor
    def read_from_cache(self, old_filename: str, new_filename: str) -> bytes:
        rename(old_filename, new_filename)
        with open(new_filename, "rb") as mp3:
            return self.fernet.decrypt(mp3.read())

    @run_in_executor
    def write_to_cache(self, filename: str, data: bytes):
        with open(filename, "wb") as mp3:
            mp3.write(self.fernet.encrypt(data))

    async def get(self, text: str, lang: str, message_id: int) -> Optional[bytes]:
        search_for = self.get_hash(str([text, lang]).encode())
        row = await self.pool.fetchrow("SELECT * FROM cache_lookup WHERE message = $1", search_for)

        if row is None:
            return

        old_message_id = row["message_id"]
        old_filename = f"cache/{old_message_id}.mp3.enc"
        new_filename = f"cache/{message_id}.mp3.enc"

        if not exists(old_filename):
            return await self.remove(old_message_id)

        read_cache_fut = self.read_from_cache(old_filename, new_filename)
        await self.pool.execute("UPDATE cache_lookup SET message_id = $1 WHERE message_id = $2", message_id, old_message_id)
        return await read_cache_fut

    async def set(self, text: str, lang: str, message_id: int, file: bytes) -> None:
        search_for = self.get_hash(str([text, lang]).encode())
        await asyncio.gather(
            self.write_to_cache(f"cache/{message_id}.mp3.enc", file),
            self.pool.execute("""
                INSERT INTO cache_lookup(message, message_id)
                VALUES ($1, $2)

                ON CONFLICT (message)
                DO UPDATE SET message_id = EXCLUDED.message_id;""",
                search_for, message_id
            )
        )

    async def remove(self, message_id: int) -> None:
        await self.pool.execute("DELETE FROM cache_lookup WHERE message_id = $1;", message_id)

    async def bulk_remove(self, message_ids: Iterable[int]) -> None:
        async with self.pool.acquire() as conn:
            for message in message_ids:
                await conn.execute("DELETE FROM cache_lookup WHERE message_id = $1;", message)
