from asyncio.exceptions import TimeoutError as asyncio_TimeoutError
from concurrent.futures._base import TimeoutError as concurrent_TimeoutError
from inspect import cleandoc
from io import StringIO
from sys import exc_info
from traceback import format_exception

import discord
from discord.ext import commands


def setup(bot):
    bot.add_cog(events_errors(bot))

class events_errors(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
        self.bot.on_error = self.on_error

    async def on_error(self, event, *args, **kwargs):
        errors = exc_info()
        info = "No Info"

        if event == "on_message":
            message = args[0]

            if message.guild is None:
                info = f"DM support | Sent by {message.author}"
            else:
                info = f"General TTS | Sent by {message.author}"

        elif event in ("on_guild_join", "on_guild_remove"):
            guild = args[0]
            info = f"Guild = {guild.name} | {guild.id}"

        try:    error_message = f"Event: `{event}`\nInfo: `{info}`\n```{''.join(format_exception(errors[0], errors[1], errors[2]))}```"
        except: error_message = f"```{''.join(format_exception(errors[0], errors[1], errors[2]))}```"

        await self.bot.channels["errors"].send(cleandoc(error_message))

    @commands.Cog.listener()
    async def on_command_error(self, ctx, error):
        if hasattr(ctx.command, 'on_error') or isinstance(error, (commands.CommandNotFound, commands.NotOwner)):
            return

        error = getattr(error, 'original', error)

        if isinstance(error, (commands.BadArgument, commands.MissingRequiredArgument, commands.UnexpectedQuoteError, commands.ExpectedClosingQuoteError)):
            return await ctx.send(f"Did you type the command right, {ctx.author.mention}? Try doing -help!")

        elif isinstance(error, (concurrent_TimeoutError, asyncio_TimeoutError)):
            return await ctx.send("**Timeout Error!** Do I have perms to see the channel you are in? (if yes, join https://discord.gg/zWPWwQC and ping Gnome!#6669)")

        elif isinstance(error, commands.NoPrivateMessage):
            return await ctx.author.send("**Error:** This command cannot be used in private messages!")

        elif isinstance(error, commands.MissingPermissions):
            return await ctx.send(f"**Error:** You are missing {', '.join(error.missing_perms)} to run this command!")

        elif isinstance(error, commands.BotMissingPermissions):
            if "send_messages" in error.missing_perms:
                return await ctx.author.send("**Error:** I could not complete this command as I don't have send messages permissions!")

            return await ctx.send(f"**Error:** I am missing the permissions: {', '.join(error.missing_perms)}")

        elif isinstance(error, discord.errors.Forbidden):
            await self.bot.channels["errors"].send(f"```discord.errors.Forbidden``` caused by {ctx.message.content} sent by {ctx.author}")
            return await ctx.author.send("Unknown Permission Error, please give TTS Bot the required permissions. If you want this bug fixed, please do `-suggest *what command you just run*`")

        first_part = f"{ctx.author} caused an error with the message: {ctx.message.clean_content}"
        second_part = ''.join(format_exception(type(error), error, error.__traceback__))
        temp = f"{first_part}\n```{second_part}```"

        if len(temp) >= 2000:
            await self.bot.channels["errors"].send(
                file=discord.File(
                    StringIO(f"{first_part}\n{second_part}"),
                    filename="long error.txt"
                ))
        else:
            await self.bot.channels["errors"].send(temp)
