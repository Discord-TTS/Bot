import json

from cryptography.fernet import Fernet

class cache():
    def __init__(self, key):
        self.key = key
        with open("cache.json", "rb") as cache_file:
            cache_db_enc = cache_file.read()
            self.cache_db = json.loads(Fernet(self.key).decrypt(cache_db_enc))

    def save(self):
        with open("cache.json", "wb") as cache_file:
            cache_db_bytes = str.encode(str(self.cache_db))
            cache_db_enc = Fernet(self.key).encrypt(cache_db_bytes)
            cache_file.write(cache_db_enc)

    def get(self, text, lang):
        search_for = str([text, lang])
        if search_for in self.cache_db:
            filename = self.cache_db[search_for]
            with open(f"cache/{filename}.mp3.enc", "rb") as mp3:
                decrypted_mp3 = Fernet(self.key).decrypt(mp3.read())
            return decrypted_mp3
        else:
            return False

    def set(self, text, lang, message_id, file_bytes):
        with open(f"cache/{message_id}.mp3.enc", "wb") as mp3:
            mp3.write(Fernet(self.key).encrypt(file_bytes))
        search_for = str([text, lang])
        self.cache_db[search_for] = message_id
