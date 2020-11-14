import json
from os import rename
from os.path import exists

from cryptography.fernet import Fernet


class cache():
    def __init__(self, key):
        self.key = key
        with open("cache.json", "rb") as cache_file:
            cache_db_enc = cache_file.read()
            self.cache_db = json.loads(Fernet(self.key).decrypt(cache_db_enc).decode())

    def save(self):
        with open("cache.json", "wb") as cache_file:
            cache_db_bytes = str.encode(json.dumps(self.cache_db))
            cache_db_enc = Fernet(self.key).encrypt(cache_db_bytes)
            cache_file.write(cache_db_enc)

    def get(self, text, lang, message_id):
        search_for = str([text, lang])
        if search_for in self.cache_db:
            filename = f"cache/{self.cache_db[search_for]}.mp3.enc"
            if not exists(filename):
                self.remove(message_id)
                return False

            with open(filename, "rb") as mp3:
                decrypted_mp3 = Fernet(self.key).decrypt(mp3.read())

            rename(filename, f"cache/{message_id}.mp3.enc")
            self.cache_db[search_for] = message_id
            return decrypted_mp3
        else:
            return False

    def set(self, text, lang, message_id, file_bytes):
        with open(f"cache/{message_id}.mp3.enc", "wb") as mp3:
            mp3.write(Fernet(self.key).encrypt(file_bytes))
        search_for = str([text, lang])
        self.cache_db[search_for] = message_id

    def remove(self, message_id):
        for key, value in dict(self.cache_db).items():
            if value == message_id:
                del self.cache_db[key]
