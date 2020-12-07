from inspect import cleandoc

import discord
from discord.ext import commands

def setup(bot):
    bot.add_cog(cmds_dev(bot))

class cmds_dev(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.command()
    @commands.is_owner()
    async def end(self, ctx):
        self.cache_cleanup.cancel()
        await self.bot.close()

    @commands.command()
    @commands.is_owner()
    async def channellist(self, ctx):
        channellist = "".join([f"{voice_client.channel} in {voice_client.channel.guild.name}" for voice_client in self.bot.voice_clients])

        tempplaying = dict()
        for key in self.bot.playing:
            if self.bot.playing[key] != 0:
                tempplaying[key] = self.bot.playing[key]
        await ctx.send(f"TTS Bot Voice Channels:\n{channellist}\nAnd just incase {tempplaying}")

    @commands.command()
    async def debug(self, ctx, reset="nope"):
        if reset.lower() == "reset":
            self.bot.playing[ctx.guild.id] = 0
            self.bot.queue[ctx.guild.id] = dict()
            embed = discord.Embed(
                title="Values Reset!",
                description="Playing and queue values for this guild have been reset, hopefully this will fix issues."
            )
            embed.set_footer(text="Debug Command, please only run if told.")
            return await ctx.send(embed=embed)

        with open("queue.txt", "w") as f:   f.write(str(self.bot.queue[ctx.guild.id]))
        await ctx.author.send(
            cleandoc(f"""
                **TTS Bot debug info!**
                Playing is currently set to {self.bot.playing.get(ctx.guild.id)}
                Guild is chunked: {ctx.guild.chunked}
                Queue for {ctx.guild.name} | {ctx.guild.id} is attached:
            """),
            file=discord.File("queue.txt"))
