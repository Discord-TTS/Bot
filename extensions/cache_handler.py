import asyncio
from hashlib import sha256
from os import rename
from os.path import exists
from typing import List

from cryptography.fernet import Fernet

from utils.decos import wrap_with, run_in_executor


def setup(bot):
    bot.cache = cache(bot)

class cache:
    def __init__(self, bot):
        self.bot = bot
        self.pool = bot.pool

        self.get = wrap_with(self.pool.acquire, True)(self.get)
        self.key = bot.config["Main"]["key"][2:-1].encode()
        self.fernet = Fernet(self.key)


    def get_hash(self, to_hash: bytes) -> bytes:
        to_hash = sha256(to_hash)
        for _ in range(9):
            to_hash = sha256(to_hash.digest() + self.key)

        return to_hash.digest()

    @run_in_executor
    def read_from_cache(self, old_filename: str, new_filename: str) -> bytes:
        rename(old_filename, new_filename)
        with open(new_filename, "rb") as mp3:
            return self.fernet.decrypt(mp3.read())

    @run_in_executor
    def write_to_cache(self, filename: str, data: bytes) -> bytes:
        with open(filename, "wb") as mp3:
            mp3.write(self.fernet.encrypt(data))


    async def get(self, conn, text: str, lang: str, message_id: int):
        search_for = self.get_hash(str([text, lang]).encode())
        row = await conn.fetchrow("SELECT * FROM cache_lookup WHERE message = $1", search_for)

        if row is None:
            return

        old_message_id = row["message_id"]
        old_filename = f"cache/{old_message_id}.mp3.enc"
        new_filename = f"cache/{message_id}.mp3.enc"

        if not exists(old_filename):
            return await self.remove(old_message_id)

        read_cache_fut = self.read_from_cache(old_filename, new_filename)
        await conn.execute("UPDATE cache_lookup SET message_id = $1 WHERE message_id = $2", message_id, old_message_id)
        return await read_cache_fut

    async def set(self, text, lang, message_id, file_bytes):
        search_for = self.get_hash(str([text, lang]).encode())
        await asyncio.gather(
            self.write_to_cache(f"cache/{message_id}.mp3.enc", file_bytes),
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

    async def bulk_remove(self, message_ids: List[int]) -> None:
        async with self.pool.acquire() as conn:
            for message in message_ids:
                await conn.execute("DELETE FROM cache_lookup WHERE message_id = $1;", message)
