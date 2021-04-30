from os import listdir, remove

from discord.ext import commands, tasks

from utils import basic
from utils.decos import handle_errors

def setup(bot):
    bot.add_cog(loops(bot))


class loops(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.cache_cleanup.start()

    def cog_unload(self):
        self.cache_cleanup.cancel()

    @tasks.loop(seconds=60)
    @handle_errors
    async def cache_cleanup(self):
        cache_size = basic.get_size("cache")
        if cache_size >= 2000000000:
            cache_folder = listdir("cache")
            cache_folder.sort(reverse=False, key=lambda x: int(''.join(filter(str.isdigit, x))))
            cache_folder = cache_folder[:1000]

            for file in cache_folder:
                remove(f"cache/{file}")

            await self.bot.cache.bulk_remove(cache_folder)

    @cache_cleanup.before_loop
    async def before_loops(self):
        await self.bot.wait_until_ready()
