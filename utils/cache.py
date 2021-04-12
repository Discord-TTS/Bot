from hashlib import sha256
from os.path import exists

from cryptography.fernet import Fernet


class cache():
    def __init__(self, key, pool):
        self.key = key
        self.pool = pool
        self.fernet = Fernet(self.key)

    def get_hash(self, to_hash: bytes) -> bytes:
        to_hash = sha256(to_hash)
        for _ in range(9):
            to_hash = sha256(to_hash.digest() + self.key)

        return to_hash.digest()

    async def get(self, text, lang, message_id):
        message_id = str(message_id)
        search_for = self.get_hash(str([text, lang]).encode())

        async with self.pool.acquire() as conn:
            row = await conn.fetchrow("SELECT * FROM cache_lookup WHERE message = $1", search_for)

            if row is not None and dict(row)["message_id"] is not None:
                og_message_id = dict(row)["message_id"]
                filename = f"cache/{og_message_id}.mp3.enc"

                if not exists(filename):
                    await self.remove(og_message_id)
                else:
                    with open(filename, "rb") as mp3:
                        decrypted_mp3 = self.fernet.decrypt(mp3.read())

                    return decrypted_mp3

    async def set(self, text, lang, message_id, file_bytes):
        message_id = str(message_id)
        with open(f"cache/{message_id}.mp3.enc", "wb") as mp3:
            mp3.write(self.fernet.encrypt(file_bytes))

        search_for = self.get_hash(str([text, lang]).encode())
        async with self.pool.acquire() as conn:
            row = await conn.fetchrow("SELECT * FROM cache_lookup WHERE message = $1", search_for)
            if row is None or dict(row)["message_id"] is None:
                await conn.execute("""
                    INSERT INTO cache_lookup(message, message_id)
                    VALUES ($1, $2);
                    """, search_for, message_id,
                                   )
            else:
                await conn.execute("""
                    UPDATE cache_lookup
                    SET message_id = $1
                    WHERE message = $2;
                    """, message_id, search_for
                                   )

    async def remove(self, message_id):
        message_id = str(message_id)
        async with self.pool.acquire() as conn:
            await conn.execute("DELETE FROM cache_lookup WHERE message_id = $1;", message_id)

    async def bulk_remove(self, message_ids):
        async with self.pool.acquire() as conn:
            for message in message_ids:
                await conn.execute("DELETE FROM cache_lookup WHERE message_id = $1;", str(message))
