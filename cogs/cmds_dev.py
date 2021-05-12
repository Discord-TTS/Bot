from typing import Union

import discord
from discord.ext import commands

import utils


def setup(bot):
    bot.add_cog(cmds_dev(bot))

class cmds_dev(utils.CommonCog, command_attrs={"hidden": True}):
    """TTS Bot hidden commands for development
    New commands added and removed often from this cog."""

    @commands.command()
    @commands.is_owner()
    async def end(self, ctx):
        await self.bot.close()

    @commands.command()
    async def debug(self, ctx):
        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=f"Voice Client: {ctx.guild.voice_client!r}"
        )

        await ctx.send(embed=embed)
