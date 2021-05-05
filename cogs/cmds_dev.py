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
            return await ctx.send("Not currently implemented.")

        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=f"Voice Client: {ctx.guild.voice_client!r}"
        )

        await ctx.send(embed=embed)
