import asyncio
from inspect import cleandoc
from random import choice as pick_random

import discord
from discord.ext import commands

from utils import basic


def setup(bot):
    bot.add_cog(cmds_main(bot))


class cmds_main(commands.Cog, name="Main Commands"):
    def __init__(self, bot):
        self.bot = bot

    @commands.guild_only()
    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def join(self, ctx):
        "Joins the voice channel you're in!"
        if ctx.channel.id != int(await ctx.bot.settings.get(ctx.guild, "channel")):
            return await ctx.send(f"Error: Wrong channel, do {ctx.bot.command_prefix}channel get the channel that has been setup.")

        if not ctx.author.voice:
            return await ctx.send("Error: You need to be in a voice channel to make me join your voice channel!")

        channel = ctx.author.voice.channel
        permissions = channel.permissions_for(ctx.guild.me)

        if not permissions.view_channel:
            return await ctx.send("Error: Missing Permission to view your voice channel!")

        if not permissions.speak or not permissions.use_voice_activation:
            return await ctx.send("Error: I do not have permssion to speak!")

        if ctx.guild.voice_client and ctx.guild.voice_client == channel:
            return await ctx.send("Error: I am already in your voice channel!")

        if ctx.guild.voice_client and ctx.guild.voice_client != channel:
            return await ctx.send("Error: I am already in a voice channel!")

        embed = discord.Embed(
            title="Joined your voice channel!",
            description="Just type normally and TTS Bot will say your messages!"
        )
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))
        embed.set_footer(text=pick_random(basic.footer_messages))

        self.bot.should_return[ctx.guild.id] = True
        self.bot.queue[ctx.guild.id] = dict()

        await channel.connect()
        self.bot.should_return[ctx.guild.id] = False

        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.bot_has_permissions(send_messages=True)
    @commands.command()
    async def leave(self, ctx):
        "Leaves voice channel TTS Bot is in!"
        if ctx.channel.id != int(await self.bot.settings.get(ctx.guild, "channel")):
            return await ctx.send("Error: Wrong channel, do -channel get the channel that has been setup.")

        elif not ctx.author.voice:
            return await ctx.send("Error: You need to be in a voice channel to make me leave!")

        elif not ctx.guild.voice_client:
            return await ctx.send("Error: How do I leave a voice channel if I am not in one?")

        elif ctx.author.voice.channel != ctx.guild.voice_client.channel:
            return await ctx.send("Error: You need to be in the same voice channel as me to make me leave!")

        self.bot.should_return[ctx.guild.id] = True
        self.bot.queue[ctx.guild.id] = dict()
        await ctx.guild.voice_client.disconnect(force=True)

        await ctx.send("Left voice channel!")

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True, add_reactions=True)
    @commands.cooldown(1, 60, commands.BucketType.member)
    @commands.command(aliases=("clear", "leaveandjoin"))
    async def skip(self, ctx):
        "Clears the message queue!"
        if ctx.channel.id != int(await ctx.bot.settings.get(ctx.guild, "channel")):
            return await ctx.send(f"Error: Wrong channel, do {ctx.bot.command_prefix}channel get the channel that has been setup.")

        if self.bot.queue.get(ctx.guild.id) in (None, dict()) or self.bot.currently_playing[ctx.guild.id].done():
            return await ctx.send("**Error:** Nothing in message queue to skip!")

        ctx.guild.voice_client.stop()
        self.bot.currently_playing[ctx.guild.id].set_result("skipped")
        return await ctx.message.add_reaction("\N{THUMBS UP SIGN}")

    @skip.after_invoke
    async def reset_cooldown(self, ctx):
        if ctx.channel.permissions_for(ctx.author).administrator:
            self.skip.reset_cooldown(ctx)
