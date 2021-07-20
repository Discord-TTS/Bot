from __future__ import annotations

import asyncio
from inspect import cleandoc
from io import StringIO
from sys import exc_info
from traceback import format_exception
from typing import TYPE_CHECKING, Optional, cast

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


IGNORED_ERRORS = (commands.CommandNotFound, commands.NotOwner)
def setup(bot: TTSBot):
    cog = ErrorEvents(bot)

    bot.add_cog(cog)
    bot.on_error = cog.on_error

class ErrorEvents(utils.CommonCog):
    async def send_error(self, ctx: utils.TypedContext, error: str, fix: str) -> Optional[discord.Message]:
        if ctx.guild:
            ctx = cast(utils.TypedGuildContext, ctx)
            permissions = ctx.bot_permissions()

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
            icon_url=str(ctx.author.avatar_url)
        ).set_footer(
            text="Support Server: https://discord.gg/zWPWwQC"
        )

        return await ctx.reply(embed=error_embed)


    async def on_error(self, event_method: str, error: Optional[BaseException] = None, *args, **_):
        info = "No Info"
        args = list(args)
        event = event_method

        if isinstance(error, BaseException):
            etype, value, tb = type(error), error, error.__traceback__
        else:
            args.insert(0, error)
            etype, value, tb = exc_info()

        if event == "on_message":
            message: discord.Message = args[0]

            if message.guild is None:
                info = f"DM support | Sent by {message.author}"
            else:
                info = f"General TTS | Sent by {message.author} in {message.guild} | {message.guild.id}"

        elif event in {"on_guild_join", "on_guild_remove"}:
            guild: discord.Guild = args[0]
            info = f"Guild = {guild} | {guild.id}"

        if self.bot.cluster_id:
            info += f"\nCluster Info: Cluster ID {self.bot.cluster_id} | Shards {self.bot.shard_count}"

        try:
            error_message = f"Event: `{event}`\nInfo: `{info}`\n```{''.join(format_exception(etype, value, tb))}```"
        except:
            error_message = f"```{''.join(format_exception(etype, value, tb))}```"

        await self.bot.channels["errors"].send(cleandoc(error_message))

    @commands.Cog.listener()
    async def on_command_error(self, ctx: utils.TypedContext, error: commands.CommandError):
        command = f"`{ctx.prefix}{ctx.command}`"
        error = getattr(error, "original", error)

        if isinstance(error, IGNORED_ERRORS):
            return

        elif isinstance(error, commands.UserInputError):
            error_name = type(error).__name__
            fix = f"check out `{ctx.prefix}help {ctx.command}`"

            if error_name.endswith("NotFound"):
                reason = f"I cannot convert `{error.argument.replace('`', '')}` into a {error_name.replace('NotFound', '').lower()}" # type: ignore
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

            if ctx.guild:
                ctx = cast(utils.TypedGuildContext, ctx)
                if cooldown_error:
                    await cooldown_error.delete()
                if ctx.bot_permissions().manage_messages:
                    await ctx.message.delete()

        elif isinstance(error, commands.NoPrivateMessage):
            await self.send_error(ctx, f"{command} cannot be used in private messages", f"try running it on a server with {self.bot.user} in")

        elif isinstance(error, commands.MissingPermissions):
            await self.send_error(ctx, "you cannot run this command", f"you are missing the permissions: {', '.join(error.missing_perms)}") # type: ignore (Stubs bug, actually list[str])

        elif isinstance(error, commands.BotMissingPermissions):
            await self.send_error(ctx,
                f"I cannot run {command} as I am missing permissions",
                f"give me {', '.join(error.missing_perms)}" # type: ignore (Stubs bug, actually list[str])
            )

        elif isinstance(error, discord.errors.Forbidden):
            self.bot.logger.error(f"`discord.errors.Forbidden` caused by {ctx.message.content} sent by {ctx.author}")
            await self.send_error(ctx, "I encountered an unknown permission error", "please give TTS Bot the required permissions")

        elif isinstance(error, asyncio.TimeoutError):
            self.bot.logger.error(f"`asyncio.TimeoutError:` Unhandled in {ctx.command.qualified_name}")

        else:
            await self.send_error(ctx, "an unknown error occured", "get in contact with us via the support server for help")

            context_part = f"{ctx.author} caused an error with the message: {ctx.message.clean_content}"
            error_traceback = "".join(format_exception(type(error), error, error.__traceback__))
            full_error = f"{context_part}\n```{error_traceback}```"

            if len(full_error) < 2000:
                await self.bot.channels["errors"].send(full_error)
            else:
                await self.bot.channels["errors"].send(
                    file=discord.File(
                        StringIO(full_error), # type: ignore
                        filename="long error.txt"
                    )
                )
