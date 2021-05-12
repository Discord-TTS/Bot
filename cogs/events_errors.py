import asyncio
import concurrent
from inspect import cleandoc
from io import StringIO
from sys import exc_info
from traceback import format_exception
from typing import Optional

import discord
from discord.ext import commands

import utils


def setup(bot):
    bot.add_cog(events_errors(bot))

class events_errors(utils.CommonCog):
    def __init__(self, bot, *args, **kwargs):
        super().__init__(bot, *args, **kwargs)

        self.bot.on_error = self.on_error


    async def send_error(self, ctx: commands.Context, error: str, fix: str, dm: bool = False) -> Optional[discord.Message]:
        if not dm:
            permissions = ctx.channel.permissions_for(ctx.guild.me)

            if not permissions.send_messages:
                return

            if not permissions.embed_links:
                return await ctx.reply("An Error Occurred! Please give me embed links permissions so I can tell you more!")

        error_embed = discord.Embed(
            title="An Error Occurred!",
            colour=discord.Colour.from_rgb(255, 0, 0),
            description=f"Sorry but {error}, to fix this, please {fix}!"
        ).set_author(
            name=ctx.author.display_name,
            icon_url=ctx.author.avatar_url
        ).set_footer(
            text="Support Server: https://discord.gg/zWPWwQC"
        )

        return await ctx.reply(embed=error_embed)


    async def on_error(self, event, error=None, *args, **kwargs):
        info = "No Info"
        args = list(args)

        if isinstance(error, BaseException):
            etype, value, tb = type(error), error, error.__traceback__
        else:
            args.insert(0, error)
            etype, value, tb = exc_info()

        if event == "on_message":
            message = args[0]

            if message.guild is None:
                info = f"DM support | Sent by {message.author}"
            else:
                info = f"General TTS | Sent by {message.author} in {message.guild} | {message.guild.id}"

        elif event in ("on_guild_join", "on_guild_remove"):
            guild = args[0]
            info = f"Guild = {guild} | {guild.id}"

        try:
            error_message = f"Event: `{event}`\nInfo: `{info}`\n```{''.join(format_exception(etype, value, tb))}```"
        except:
            error_message = f"```{''.join(format_exception(etype, value, tb))}```"

        await self.bot.channels["errors"].send(cleandoc(error_message))

    @commands.Cog.listener()
    async def on_command_error(self, ctx, error):
        if hasattr(ctx.command, 'on_error') or isinstance(error, (commands.CommandNotFound, commands.NotOwner)):
            return

        command = f"`{ctx.prefix}{ctx.command}`"
        error = getattr(error, 'original', error)

        if isinstance(error, commands.UserInputError):
            error_name = type(error).__name__
            fix = f"check out `{ctx.prefix}help {ctx.command}`"

            if error_name.endswith("NotFound"):
                reason = f"I cannot convert `{error.argument.replace('`', '')}` into a {error_name.replace('NotFound', '').lower()}"
            elif isinstance(error, commands.BadBoolArgument):
                reason = f"I cannot convert `{error.argument.replace('`', '')}` to True/False"
            elif isinstance(error, commands.BadUnionArgument):
                types = [converter.__name__.replace("Converter", "") for converter in error.converters]
                reason = f"I cannot convert your argument into {' or '.join(types)}"
            elif isinstance(error, commands.ArgumentParsingError):
                reason = "I cannot parse your message into multiple arguments"
                fix = "try removing quote marks or adding some in"
            else:
                reason = "you typed the command wrong"

            await self.send_error(ctx, reason, fix)

        elif isinstance(error, commands.CommandOnCooldown):
            cooldown_error = await self.send_error(ctx, f"{command} is on cooldown", f"try again in {error.retry_after:.1f} seconds")
            await asyncio.sleep(error.retry_after)

            if cooldown_error:
                await cooldown_error.delete()
            if ctx.channel.permissions_for(ctx.guild.me).manage_messages:
                await ctx.message.delete()

        elif isinstance(error, commands.NoPrivateMessage):
            await self.send_error(ctx, f"{command} cannot be used in private messages", f"try running it on a server with {self.bot.user} in", dm=True)

        elif isinstance(error, commands.MissingPermissions):
            await self.send_error(ctx, "you cannot run this command", f"you are missing the permissions: {', '.join(error.missing_perms)}")

        elif isinstance(error, commands.BotMissingPermissions):
            await self.send_error(ctx,
                f"I cannot run {command} as I am missing permissions",
                f"give me {', '.join(error.missing_perms)}",
                dm="send_messages" not in error.missing_perms
            )

        elif isinstance(error, discord.errors.Forbidden):
            await asyncio.gather(
                self.send_error(ctx, "I encountered an unknown permission error", "please give TTS Bot the required permissions"),
                self.bot.channels["errors"].send(f"```discord.errors.Forbidden``` caused by {ctx.message.content} sent by {ctx.author}")
            )

        else:
            await self.send_error(ctx, "an unknown error occured", "get in contact with us via the support server for help")

            context_part    = f"{ctx.author} caused an error with the message: {ctx.message.clean_content}"
            error_traceback =  "".join(format_exception(type(error), error, error.__traceback__))
            full_error = f"{context_part}\n```{error_traceback}```"

            if len(full_error) < 2000:
                await self.bot.channels["errors"].send(full_error)
            else:
                await self.bot.channels["errors"].send(
                    file=discord.File(
                        StringIO(full_error),
                        filename="long error.txt"
                    )
                )
