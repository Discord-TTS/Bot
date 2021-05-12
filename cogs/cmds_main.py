from inspect import cleandoc
from random import choice as pick_random

import discord
from discord.ext import commands

from utils import basic
from player import TTSVoicePlayer

def setup(bot):
    bot.add_cog(cmds_main(bot))


class cmds_main(commands.Cog, name="Main Commands"):
    "TTS Bot main commands, required for the bot to work."

    def __init__(self, bot):
        self.bot = bot

    async def channel_check(self, ctx):
        if ctx.channel.id != await self.bot.settings.get(ctx.guild, "channel"):
            await ctx.send(f"Error: Wrong channel, do {ctx.prefix}channel get the channel that has been setup.")
            return False

        return True


    @commands.guild_only()
    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def join(self, ctx):
        "Joins the voice channel you're in!"
        if not await self.channel_check(ctx):
            return

        if not ctx.author.voice:
            return await ctx.send("Error: You need to be in a voice channel to make me join your voice channel!")

        voice_client = ctx.guild.voice_client
        voice_channel = ctx.author.voice.channel
        permissions = voice_channel.permissions_for(ctx.guild.me)

        if not permissions.view_channel:
            return await ctx.send("Error: Missing Permission to view your voice channel!")

        if not permissions.speak or not permissions.use_voice_activation:
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
        join_embed.set_thumbnail(url=str(self.bot.user.avatar_url))
        join_embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))
        join_embed.set_footer(text=pick_random(basic.footer_messages))

        await voice_channel.connect(cls=TTSVoicePlayer)
        await ctx.send(embed=join_embed)

        if self.bot.blocked:
            blocked_embed = discord.Embed(title="TTS Bot is currently blocked by Google")
            blocked_embed.description = cleandoc(f"""
                During this temporary block, voice has been swapped to a worse quality voice.
                If you want to avoid this, consider TTS Bot Premium, which you can get by donating via Patreon: `{ctx.prefix}donate`
                """)
            blocked_embed.set_footer(text="You can join the support server for more info: discord.gg/zWPWwQC")

            await ctx.send(embed=blocked_embed)

    @commands.guild_only()
    @commands.cooldown(1, 10, commands.BucketType.guild)
    @commands.bot_has_permissions(send_messages=True)
    @commands.command()
    async def leave(self, ctx):
        "Leaves voice channel TTS Bot is in!"
        if not await self.channel_check(ctx):
            return

        if not ctx.author.voice:
            return await ctx.send("Error: You need to be in a voice channel to make me leave!")

        if not ctx.guild.voice_client:
            return await ctx.send("Error: How do I leave a voice channel if I am not in one?")

        if ctx.author.voice.channel != ctx.guild.voice_client.channel:
            return await ctx.send("Error: You need to be in the same voice channel as me to make me leave!")

        await ctx.guild.voice_client.disconnect(force=True)
        await ctx.send("Left voice channel!")


    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True, add_reactions=True)
    @commands.cooldown(1, 60, commands.BucketType.member)
    @commands.command(aliases=("clear", "leaveandjoin"))
    async def skip(self, ctx):
        "Clears the message queue!"
        if not await self.channel_check(ctx):
            return

        vc = ctx.guild.voice_client
        if not vc or (not vc.is_playing() and vc.message_queue.empty() and vc.audio_buffer.empty()):
            return await ctx.send("**Error:** Nothing in message queue to skip!")

        ctx.guild.voice_client.skip()
        return await ctx.message.add_reaction("\N{THUMBS UP SIGN}")

    @skip.after_invoke
    async def reset_cooldown(self, ctx):
        if ctx.channel.permissions_for(ctx.author).administrator:
            self.skip.reset_cooldown(ctx)
