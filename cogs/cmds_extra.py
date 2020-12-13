from inspect import cleandoc
from time import monotonic

import discord
from discord.ext import commands


start_time = monotonic()

def setup(bot):
    bot.add_cog(cmds_extra(bot))

class cmds_extra(commands.Cog, name="Extra Commands"):
    def __init__(self, bot):
        self.bot = bot

    @commands.command()
    async def uptime(self, ctx):
        "Shows how long TTS Bot has been online"
        await ctx.send(f"{self.bot.user.mention} has been up for {(monotonic() - start_time) / 60:.2f} minutes")

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command(hidden=True)
    async def tts(self, ctx):
        if ctx.message.content == f"{self.bot.command_prefix}tts":
            await ctx.send(f"You don't need to do `{self.bot.command_prefix}tts`! {self.bot.user.mention} is made to TTS any message, and ignore messages starting with `{self.bot.command_prefix}`!")

    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command(aliases=["info", "stats"])
    async def botstats(self, ctx):
        "Shows various different stats"

        channels = len(self.bot.voice_clients)

        main_section = cleandoc(f"""
          Currently in:
            :small_blue_diamond: {channels} voice channels
            :small_orange_diamond: {len(self.bot.guilds)} servers
          and can be used by {sum(guild.member_count for guild in self.bot.guilds):,} people!
        """)

        footer = cleandoc("""
            Support Server: https://discord.gg/zWPWwQC
            Repository: https://github.com/Gnome-py/Discord-TTS-Bot
        """)

        embed = discord.Embed(title=f"{self.bot.user.name}: Now open source!", description=main_section, url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_footer(text=footer)
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))

        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def channel(self, ctx):
        "Shows the current setup channel!"
        channel = int(await self.bot.settings.get(ctx.guild, "channel"))

        if channel == ctx.channel.id:
            await ctx.send("You are in the right channel already!")
        elif channel != 0:
            await ctx.send(f"The current setup channel is: <#{channel}>")
        else:
            await ctx.send("The channel hasn't been setup, do `-setup #textchannel`")
