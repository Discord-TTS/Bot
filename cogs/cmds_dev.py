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
            self.bot.currently_playing[ctx.guild.id] = asyncio.Event()

            return await ctx.send("All internal guild values reset, try doing whatever you were doing again!")

        currently_playing = self.bot.currently_playing.get(message.guild.id, False)
        lock = self.bot.message_locks.get(ctx.guild.id, False)
        if currently_playing:
            currently_playing = not event.is_set()
        if lock:
            lock = lock.locked()

        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=cleandoc(f"""
                Reading messages is currently locked: {lock}
                Currently speaking a message: {currently_playing}
                Shouldn't read messages: {self.bot.should_return.get(ctx.guild.id)}
                Queue has {len(self.bot.queue.get(ctx.guild.id, ()))} message(s) in it
            """)
        )

        await ctx.send(embed=embed)
