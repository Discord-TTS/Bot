from configparser import ConfigParser
from inspect import cleandoc
from os.path import exists
from time import monotonic

from discord.ext import commands

from utils.basic import ensure_webhook

def setup(bot):
    if exists("config.ini"):
        config = ConfigParser()
        config.read("config.ini")

    bot.add_cog(User(bot))

class User(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.command()
    @commands.bot_has_permissions(send_messages=True, read_messages=True)
    async def donate(self, ctx):
        await ctx.send(cleandoc(f"""
            To donate to support the development and hosting of {self.bot.user.mention}, you can donate via Patreon (Fees) or directly via DonateBot.io!
            <https://donatebot.io/checkout/693901918342217758?buyer={ctx.author.id}>
            https://www.patreon.com/Gnome_the_Bot_Maker
        """))

    @commands.command(aliases=["ping"])
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    async def lag(self, ctx):
        ping_before = monotonic()
        ping_message = await ctx.send("Loading!")
        ping = (monotonic() - ping_before) * 1000
        await ping_message.edit(content=f"Current Latency: `{ping:.0f}ms`")

    @commands.command()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    async def suggest(self, ctx, *, suggestion):
        if suggestion.lower().replace("*", "") == "suggestion":
            return await ctx.send("Hey! You are meant to replace `*suggestion*` with your actual suggestion!")

        if not await self.bot.blocked_users.check(ctx.message.author):
            webhook = await ensure_webhook(self.bot.channels["suggestions"], "SUGGESTIONS")
            files = [await attachment.to_file() for attachment in ctx.message.attachments]

            await webhook.send(suggestion, username=str(ctx.author), avatar_url=ctx.author.avatar_url, files=files)

        await ctx.send("Suggestion noted")

    @commands.command()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    async def invite(self, ctx):
        if ctx.guild == self.bot.supportserver:
            await ctx.send(f"Check out <#694127922801410119> to invite {self.bot.user.mention}!")
        else:
            await ctx.send(f"Join https://discord.gg/zWPWwQC and look in #{self.bot.get_channel(694127922801410119).name} to invite {self.bot.user.mention}!")
