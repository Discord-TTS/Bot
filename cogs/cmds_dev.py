from __future__ import annotations

from typing import TYPE_CHECKING

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(cmds_dev(bot))

class cmds_dev(utils.CommonCog, command_attrs={"hidden": True}):
    """TTS Bot hidden commands for development
    New commands added and removed often from this cog."""

    @commands.command()
    @commands.is_owner()
    async def end(self, _):
        await self.bot.close()

    @commands.command()
    async def debug(self, ctx: commands.Context):
        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=f"Voice Client: {ctx.guild.voice_client!r}"
        )

        await ctx.send(embed=embed)
