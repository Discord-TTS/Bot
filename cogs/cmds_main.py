from __future__ import annotations

import asyncio
from inspect import cleandoc
from random import choice as pick_random
from typing import TYPE_CHECKING, cast

import discord
from discord.ext import commands

import utils
from player import TTSVoicePlayer


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(MainCommands(bot))

class MainCommands(utils.CommonCog, name="Main Commands"):
    "TTS Bot main commands, required for the bot to work."

    async def channel_check(self, ctx: utils.TypedGuildContext) -> bool:
        channel = (await self.bot.settings.get(ctx.guild, ["channel"]))[0]
        if ctx.channel.id != channel:
            await ctx.send(f"Error: Wrong channel, do {ctx.prefix}channel get the channel that has been setup.")
            return False

        return True


    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.guild_only()
    @commands.command()
    async def join(self, ctx: utils.TypedGuildContext):
        "Joins the voice channel you're in!"
        if not await self.channel_check(ctx):
            return

        if not isinstance(ctx.author, discord.Member):
            return

        if not ctx.author.voice:
            return await ctx.send("Error: You need to be in a voice channel to make me join your voice channel!")

        voice_client = ctx.guild.voice_client
        voice_channel = ctx.author.voice.channel
        permissions = voice_channel.permissions_for(ctx.guild.me)

        if not permissions.view_channel:
            return await ctx.send("Error: Missing Permission to view your voice channel!")

        if not permissions.speak:
            return await ctx.send("Error: I do not have permssion to speak!")

        if voice_client:
            if voice_client == voice_channel:
                await ctx.send("Error: I am already in your voice channel!")
            else:
                await ctx.send(f"Error: I am in {voice_client.channel.mention}!")
            return

        join_embed = discord.Embed(
            title="Joined your voice channel!",
            description="Just type normally and TTS Bot will say your messages!"
        )
        join_embed.set_thumbnail(url=self.bot.user.avatar.url)
        join_embed.set_author(name=ctx.author.display_name, icon_url=ctx.author.avatar.url)
        join_embed.set_footer(text=pick_random(utils.FOOTER_MSGS))

        try:
            await voice_channel.connect(cls=TTSVoicePlayer)
        except asyncio.TimeoutError:
            return await ctx.send("Error: Timed out when trying to join your voice channel!")

        await ctx.send(embed=join_embed)

        if self.bot.blocked:
            blocked_embed = discord.Embed(title="TTS Bot is currently blocked by Google")
            blocked_embed.description = cleandoc(f"""
                During this temporary block, voice has been swapped to a worse quality voice.
                If you want to avoid this, consider TTS Bot Premium, which you can get by donating via Patreon: `{ctx.prefix}donate`
                """)
            blocked_embed.set_footer(text="You can join the support server for more info: discord.gg/zWPWwQC")

            await ctx.send(embed=blocked_embed)

    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.bot_has_permissions(send_messages=True)
    @commands.guild_only()
    @commands.command()
    async def leave(self, ctx: utils.TypedGuildContext):
        "Leaves voice channel TTS Bot is in!"
        if not await self.channel_check(ctx):
            return

        if not isinstance(ctx.author, discord.Member):
            return

        if not ctx.author.voice:
            return await ctx.send("Error: You need to be in a voice channel to make me leave!")

        if not ctx.guild.voice_client:
            return await ctx.send("Error: How do I leave a voice channel if I am not in one?")

        if ctx.author.voice.channel != ctx.guild.voice_client.channel:
            return await ctx.send("Error: You need to be in the same voice channel as me to make me leave!")

        await ctx.guild.voice_client.disconnect(force=True)
        await ctx.send("Left voice channel!")


    @commands.bot_has_permissions(send_messages=True, add_reactions=True)
    @commands.cooldown(1, 60, commands.BucketType.member)
    @commands.command(aliases=("clear", "leaveandjoin"))
    @commands.guild_only()
    async def skip(self, ctx: utils.TypedGuildContext):
        "Clears the message queue!"
        if not await self.channel_check(ctx):
            return

        vc = ctx.guild.voice_client
        if not vc or (not vc.is_playing() and vc.message_queue.empty() and vc.audio_buffer.empty()):
            return await ctx.send("**Error:** Nothing in message queue to skip!")

        vc.skip()
        return await ctx.message.add_reaction("\N{THUMBS UP SIGN}")

    @skip.after_invoke
    async def reset_cooldown(self, ctx: utils.TypedGuildContext):
        if ctx.author_permissions().administrator:
            self.skip.reset_cooldown(ctx)
