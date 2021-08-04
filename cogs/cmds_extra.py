from __future__ import annotations

import asyncio
import time
import uuid
from functools import partial
from inspect import cleandoc
from io import BytesIO
from os import getpid
from typing import TYPE_CHECKING, cast

import discord
from discord.ext import commands
from psutil import Process

import utils
from utils.websocket_types import *


if TYPE_CHECKING:
    from main import TTSBotPremium

    from events_errors import ErrorEvents
else:
    ErrorEvents = None


start_time = time.monotonic()
def get_ram_recursive(process: Process) -> int:
    return sum(proc.memory_info().rss for proc in process.children())

def setup(bot: TTSBotPremium):
    bot.add_cog(ExtraCommands(bot))

class ExtraCommands(utils.CommonCog, name="Extra Commands"):
    "TTS Bot extra commands, not required but useful."

    @commands.command()
    async def uptime(self, ctx: utils.TypedContext):
        "Shows how long TTS Bot has been online"
        await ctx.send(f"{self.bot.user.mention} has been up for {(time.monotonic() - start_time) / 60:.2f} minutes")

    @commands.bot_has_permissions(send_messages=True, attach_files=True)
    @commands.command()
    async def tts(self, ctx: utils.TypedContext, *, message: str):
        "Generates TTS and sends it in the current text channel!"
        if (
            not ctx.interaction
            and ctx.guild is not None
            and ctx.guild.voice_client
            and (await self.bot.settings.get(ctx.guild, ["channel"]))[0] == ctx.channel.id
        ):
            return # probably in VC, just let on_message do TTS

        author_name = "".join(filter(str.isalnum, ctx.author.name))
        
        lang = await self.bot.userinfo.get("lang", ctx.author, "en-us")
        variant = await self.bot.userinfo.get("variant", ctx.author, "a")
        voice = await self.bot.get_voice(lang, variant)

        audio, _ = await utils.TTSAudioMaker(self.bot).get_tts(
            text=message,
            voice=voice.tuple,
            max_length=float("inf")
        )

        if audio is None:
            return await ctx.reply("Failed to generate TTS!", ephemeral=True)

        await ctx.reply("Generated some TTS!", file=discord.File(
            fp=BytesIO(audio), filename=(
                f"{author_name}-{ctx.message.id}-premium.ogg"
            )
        ))

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.command(aliases=["info", "stats"])
    async def botstats(self, ctx: utils.TypedContext):
        "Shows various different stats"
        await ctx.trigger_typing()

        start_time = time.perf_counter()
        if self.bot.websocket is None:
            guilds = [guild for guild in self.bot.guilds if not guild.unavailable]
            total_members = sum(guild.member_count for guild in guilds)
            total_voice_clients = len(self.bot.voice_clients)
            total_guild_count = len(guilds)

            raw_ram_usage = Process(getpid()).memory_info().rss
        else:
            ws_uuid = uuid.uuid4()
            to_fetch = ["guild_count", "member_count", "voice_count"]
            wsjson = utils.data_to_ws_json("REQUEST", target="*", info=to_fetch, nonce=ws_uuid)

            await self.bot.websocket.send(wsjson)
            try:
                check = lambda _, nonce: uuid.UUID(nonce) == ws_uuid
                responses, _ = await self.bot.wait_for(timeout=10, check=check, event="response",)
            except asyncio.TimeoutError:
                cog = cast(ErrorEvents, self.bot.get_cog("ErrorEvents"))
                self.bot.logger.error("Timed out fetching botstats!")

                error_msg = "the bot timed out fetching this info"
                return await cog.send_error(ctx, error_msg)

            raw_ram_usage = await utils.to_thread(partial(get_ram_recursive, Process(getpid()).parent()))
            total_voice_clients = sum(resp["voice_count"] for resp in responses)
            total_guild_count = sum(resp["guild_count"] for resp in responses)
            total_members = sum(resp["member_count"] for resp in responses)

        time_to_fetch = (time.perf_counter() - start_time) * 1000
        footer = cleandoc(f"""
            Time to fetch: {time_to_fetch:.2f}ms
            Support Server: https://discord.gg/zWPWwQC
            Repository: https://github.com/Gnome-py/Discord-TTS-Bot
        """)

        sep1, sep2, *_ = utils.OPTION_SEPERATORS
        embed = discord.Embed(
            title=f"{self.bot.user.name} Stats",
            url="https://discord.gg/zWPWwQC",
            colour=utils.NETURAL_COLOUR,
            description=cleandoc(f"""
                Currently in:
                    {sep2} {total_voice_clients} voice channels
                    {sep2} {total_guild_count} servers
                Currently using:
                    {sep1} {self.bot.shard_count} shards
                    {sep1} {raw_ram_usage / 1024 ** 2:.1f}MB of RAM
                and can be used by {total_members:,} people!
            """)
        ).set_footer(text=footer).set_thumbnail(url=self.bot.user.avatar.url)

        await ctx.send(embed=embed)

    @commands.bot_has_permissions(send_messages=True)
    @commands.guild_only()
    @commands.command()
    async def channel(self, ctx: utils.TypedGuildContext):
        "Shows the current setup channel!"
        channel = (await self.bot.settings.get(ctx.guild, ["channel"]))[0]

        if channel == ctx.channel.id:
            await ctx.send("You are in the right channel already!")
        elif channel != 0:
            await ctx.send(f"The current setup channel is: <#{channel}>")
        else:
            await ctx.send(f"The channel hasn't been setup, do `{ctx.prefix}setup #textchannel`")

    @commands.command()
    @commands.bot_has_permissions(send_messages=True)
    async def donate(self, ctx: utils.TypedContext):
        "Shows how you can help support TTS Bot's development and hosting!"
        await ctx.send(
            "To donate to support the development and hosting of "
            f"{self.bot.user.mention} and get access to TTS Bot Premium, "
            "a more stable version of this bot with more and better voices "
            "you can donate via Patreon!\nhttps://www.patreon.com/Gnome_the_Bot_Maker"
        )

    @commands.command(aliases=["lag"])
    @commands.bot_has_permissions(send_messages=True)
    async def ping(self, ctx: utils.TypedContext):
        "Gets current ping to discord!"

        ping_before = time.perf_counter()
        ping_message = await ctx.send("Loading!", return_msg=True)
        ping = (time.perf_counter() - ping_before) * 1000
        await ping_message.edit(content=f"Current Latency: `{ping:.0f}ms`")

    @commands.command()
    @commands.bot_has_permissions(send_messages=True)
    async def suggest(self, ctx: utils.TypedContext, *, suggestion: str):
        "Suggests a new feature!"

        if suggestion.lower().replace("*", "") == "suggestion":
            return await ctx.send("Hey! You are meant to replace `*suggestion*` with your actual suggestion!")

        if not await self.bot.userinfo.get("blocked", ctx.author, default=False):
            files = [await attachment.to_file() for attachment in ctx.message.attachments]

            author_name = str(ctx.author)
            author_id = f" ({ctx.author.id})"
            await self.bot.channels["suggestions"].send(
                files=files,
                content=suggestion,
                avatar_url=ctx.author.avatar.url,
                username=author_name[:32 - len(author_id)] + author_id,
            )

        await ctx.send("Suggestion noted")

    @commands.command()
    @commands.bot_has_permissions(send_messages=True)
    async def invite(self, ctx: utils.TypedContext):
        "Sends the instructions to invite TTS Bot and join the support server!"
        await ctx.send(f"Join https://discord.gg/zWPWwQC and check out the patreon channel to invite me!")
