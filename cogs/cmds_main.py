from __future__ import annotations

import asyncio
from random import choice as pick_random
from typing import TYPE_CHECKING

import discord
from discord.ext import commands

import utils
from player import TTSVoiceClient

if TYPE_CHECKING:
    from main import TTSBotPremium


def setup(bot: TTSBotPremium):
    bot.add_cog(MainCommands(bot))

class MainCommands(utils.CommonCog, name="Main Commands"):
    "TTS Bot main commands, required for the bot to work."
    def cog_check(self, ctx: utils.TypedContext) -> bool:
        if ctx.guild is None:
            return False

        return ctx.channel.id == self.bot.settings[ctx.guild.id]["channel"]

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.guild_only()
    @commands.command()
    async def join(self, ctx: utils.TypedGuildContext):
        "Joins the voice channel you're in!"
        if not ctx.author.voice:
            return await ctx.send_error(
                error="you need to be in a voice channel to make me join your voice channel",
                fix="join a voice channel and try again"
            )

        voice_client = ctx.guild.voice_client
        voice_channel = ctx.author.voice.channel
        permissions: discord.Permissions = voice_channel.permissions_for(ctx.guild.me)

        missing_perms = []
        if not permissions.view_channel:
            missing_perms.append("view_channel")
        if not permissions.speak:
            missing_perms.append("speak")

        if missing_perms:
            raise commands.BotMissingPermissions(missing_perms)

        if voice_client:
            if voice_client.channel == voice_channel:
                return await ctx.reply("I am already in your voice channel!")

            channel_mention = voice_client.channel.mention
            move_channel_view = utils.BoolView(ctx, "Yes", "No")
            await ctx.reply(f"I am already in {channel_mention}! Would you like me to move to this channel?", view=move_channel_view)

            if await move_channel_view.wait():
                await voice_client.move_to(voice_channel)

            return

        join_embed = discord.Embed(
            title="Joined your voice channel!",
            description="Just type normally and TTS Bot Premium will say your messages!"
        )
        join_embed.set_thumbnail(url=self.bot.user.display_avatar.url)
        join_embed.set_author(name=ctx.author.display_name, icon_url=ctx.author.display_avatar.url)
        join_embed.set_footer(text=pick_random(utils.FOOTER_MSGS))

        if ctx.interaction is not None:
            await ctx.interaction.response.defer()

        try:
            await voice_channel.connect(cls=TTSVoiceClient) # type: ignore
        except asyncio.TimeoutError:
            return await ctx.send_error("I took too long trying to join your voice channel", "try again later")

        await ctx.send(embed=join_embed)

    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.bot_has_permissions(send_messages=True)
    @commands.guild_only()
    @commands.command()
    async def leave(self, ctx: utils.TypedGuildContext):
        "Leaves voice channel TTS Bot is in!"
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
        vc = ctx.guild.voice_client
        if not vc or (not vc.is_playing() and vc.message_queue.empty() and vc.audio_buffer.empty()):
            return await ctx.send("**Error:** Nothing in message queue to skip!")

        vc.skip()
        if ctx.interaction is not None:
            return

        try:
            await ctx.message.add_reaction("\N{THUMBS UP SIGN}")
        except discord.NotFound:
            pass

    @skip.after_invoke # type: ignore (pylance not accepting subclasses)
    async def reset_cooldown(self, ctx: utils.TypedGuildContext):
        if ctx.author_permissions().administrator:
            self.skip.reset_cooldown(ctx)
