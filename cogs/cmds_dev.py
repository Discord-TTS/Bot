from __future__ import annotations

import asyncio
import logging
from inspect import cleandoc
from typing import TYPE_CHECKING

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(DevCommands(bot))

class DevCommands(utils.CommonCog, command_attrs={"hidden": True}):
    """TTS Bot hidden commands for development
    New commands added and removed often from this cog."""

    @commands.command(aliases=("log_level", "logger", "loglevel"))
    @commands.is_owner()
    async def change_log_level(self, ctx: utils.TypedContext, *, level: str):
        with open("config.ini", "w") as config_file:
            self.bot.config["Main"]["log_level"] = level
            self.bot.config.write(config_file)

        if self.bot.websocket is None:
            return self.bot.logger.setLevel(level.upper())

        try:
            wsjson = utils.data_to_ws_json("SEND", target="*", **{
                "c": "change_log_level",
                "a": {"level": level.upper()},
                "t": "*"
            })
            await self.bot.websocket.send(wsjson)
            await self.bot.wait_for("change_log_level", timeout=10)
        except asyncio.TimeoutError:
            await ctx.send(f"Didn't recieve broadcast within 10 seconds!")
        else:
            level = logging.getLevelName(self.bot.logger.level)
            await ctx.send(f"Broadcast complete, log level is now: {level}")


    @commands.group(aliases=("end", "restart"))
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


    @commands.command()
    @commands.guild_only()
    async def debug(self, ctx: utils.TypedGuildContext):
        embed = discord.Embed(
            title="TTS Bot debug info!",
            description=cleandoc(f"""
                Cluster ID: {self.bot.cluster_id}
                Voice Client: {ctx.guild.voice_client!r}
            """)
        )

        await ctx.send(embed=embed)
