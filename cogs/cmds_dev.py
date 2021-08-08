from __future__ import annotations

from inspect import cleandoc
from typing import TYPE_CHECKING, Optional

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(DevCommands(bot))

class DevCommands(utils.CommonCog, name="Development Commands"):
    """TTS Bot hidden commands for development
    New commands added and removed often from this cog."""

    @commands.group(aliases=("end", "restart"), hidden=True)
    @commands.is_owner()
    async def close(self, ctx: utils.TypedContext):
        if not ctx.invoked_subcommand:
            return await ctx.send("Unknown close type!")

    @close.command()
    async def all(self, _: utils.TypedContext):
        return await self.bot.close(utils.KILL_EVERYTHING)

    @close.command()
    async def cluster(self, ctx: utils.TypedContext, cluster_id: int):
        if self.bot.websocket is None:
            return await ctx.send("Manager websocket is None!")

        if cluster_id == self.bot.cluster_id:
            await self.bot.close(utils.RESTART_CLUSTER)
        else:
            wsjson = utils.data_to_ws_json("SEND", target=cluster_id, **{
                "c": "restart",
                "a": {},
            })

            await self.bot.websocket.send(wsjson)
            await ctx.send(f"Told cluster {cluster_id} to restart.")


    @commands.group()
    async def debug(self, ctx: utils.TypedContext):
        "Shows info, for debug usage"
        if not ctx.invoked_subcommand:
            await self.info(ctx)

    @debug.command()
    @commands.guild_only()
    async def info(self, ctx: utils.TypedGuildContext):
        "Shows info, for debug usage"
        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=cleandoc(f"""
                Cluster ID: {self.bot.cluster_id}
                Blocked by Google: {self.bot.blocked}
                Voice Client: {ctx.guild.voice_client!r}
            """)
        )

        await ctx.send(embed=embed)

    @debug.command()
    async def invoke(self, ctx: utils.TypedContext, command: str, *, args: Optional[str] = ""):
        "Manually invokes command, for debug usage."
        prefix = await self.bot.command_prefix(self.bot, ctx.message)
        ctx.message.content = f"{prefix}{command} {args}"

        ctx = await self.bot.get_context(ctx.message)
        await ctx.command.invoke(ctx)
