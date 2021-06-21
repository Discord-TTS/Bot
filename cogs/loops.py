from __future__ import annotations

import time
from os import getpid, listdir, remove
from typing import Generator, TYPE_CHECKING

import psutil
from discord.ext import tasks

import utils


if TYPE_CHECKING:
    from main import TTSBot


CURRENT_PID = getpid()
def setup(bot: TTSBot):
    bot.add_cog(loops(bot))

class loops(utils.CommonCog):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.cache_cleanup.start()

    def cog_unload(self):
        self.cache_cleanup.cancel()

    @tasks.loop(seconds=60)
    @utils.decos.handle_errors
    async def cache_cleanup(self):
        "Deletes old cache once past a certain size"
        cache_size = utils.get_size("cache")
        if cache_size >= 2000000000:
            cache_folder = listdir("cache")
            cache_folder.sort(reverse=False, key=lambda x: int(''.join(filter(str.isdigit, x))))
            cache_folder = cache_folder[:1000]

            for file in cache_folder:
                remove(f"cache/{file}")

            await self.bot.cache.bulk_remove(int(cache_file[:-8]) for cache_file in cache_folder)


    @cache_cleanup.before_loop # type: ignore
    async def before_loops(self):
        await self.bot.wait_until_ready()
