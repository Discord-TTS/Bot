from os import listdir, remove

from discord.ext import commands, tasks

from utils import basic


def setup(bot):
    bot.add_cog(loops(bot))


class loops(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.cache_cleanup.start()

    def cog_unload(self):
        self.cache_cleanup.cancel()

    @tasks.loop(seconds=60)
    async def cache_cleanup(self):
        try:
            cache_size = basic.get_size("cache")
            if cache_size >= 1073741824:
                print("Deleting 100 messages from cache!")
                cache_folder = listdir("cache")
                cache_folder.sort(reverse=False, key=lambda x: int(''.join(filter(str.isdigit, x))))

                for count, cached_message in enumerate(cache_folder):
                    remove(f"cache/{cached_message}")
                    await self.bot.cache.remove(cached_message)

                    if count == 100:
                        break

        except Exception as error:
            await self.bot.on_error("cache_cleanup", error)

    @cache_cleanup.before_loop
    async def before_loops(self):
        await self.bot.wait_until_ready()
