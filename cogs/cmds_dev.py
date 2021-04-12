from inspect import cleandoc
from asyncio import Lock

import discord
from discord.ext import commands

def setup(bot):
    bot.add_cog(cmds_dev(bot))

class cmds_dev(commands.Cog, command_attrs=dict(hidden=True)):
    """TTS Bot hidden commands for development
    New commands added and removed often from this cog."""

    def __init__(self, bot):
        self.bot = bot

    @commands.command()
    @commands.is_owner()
    async def end(self, ctx):
        await self.bot.close()

    @commands.command()
    async def debug(self, ctx, reset="nope"):
        if reset.lower() == "reset":
            self.bot.queue[ctx.guild.id] = dict()
            self.bot.message_locks[ctx.guild.id] = Lock()
            if self.bot.currently_playing.get(ctx.guild.id) is not None and not self.bot.currently_playing[ctx.guild.id].done():
                self.bot.currently_playing[ctx.guild.id].set_result("done")

            return await ctx.send("All internal guild values reset, try doing whatever you were doing again!")

        lock = self.bot.message_locks.get(ctx.guild.id, False)
        if lock:
            lock = lock.locked()

        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=cleandoc(f"""
                Reading messages is currently locked: {lock}
                Shouldn't read messages: {self.bot.should_return.get(ctx.guild.id)}
                Queue has {len(self.bot.queue.get(ctx.guild.id, ()))} message(s) in it
            """)
        )

        await ctx.send(embed=embed)
