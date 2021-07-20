from __future__ import annotations
import asyncio
import os

from typing import TYPE_CHECKING, Union

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

    @commands.group(aliases=("end",))
    @commands.is_owner()
    async def close(self, ctx: utils.TypedContext):
        if not ctx.invoked_subcommand:
            return await ctx.send("Unknown close type!")

    @close.command()
    async def all(self, _: utils.TypedContext):
        self.bot.status_code = utils.KILL_EVERYTHING
        return await self.bot.close()

    @close.command()
    async def cluster(self, ctx: utils.TypedContext, cluster_id: int):
        if self.bot.websocket is None:
            return await ctx.send("Manager websocket is None!")

        if cluster_id == self.bot.cluster_id:
            self.bot.status_code = utils.RESTART_CLUSTER
            await self.bot.close()
        else:
            await self.bot.websocket.send(f"SEND {cluster_id} CLOSE")
            await ctx.send(f"Told cluster {cluster_id} to die.")

    @commands.command()
    @commands.guild_only()
    async def debug(self, ctx: utils.TypedGuildContext):
        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=f"Voice Client: {ctx.guild.voice_client!r}"
        )

        await ctx.send(embed=embed)
