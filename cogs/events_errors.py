from __future__ import annotations

import asyncio
from sys import exc_info
from traceback import format_exception
from typing import TYPE_CHECKING, Any, Optional, cast

import discord
import psutil
from discord.ext import commands

import utils

if TYPE_CHECKING:
    from main import TTSBotPremium
    from player import TTSVoiceClient


BLANK = ("\u200B", "\u200B", True)
def setup(bot: TTSBotPremium):
    cog = ErrorEvents(bot)

    bot.add_cog(cog)
    bot.on_error = cog.on_error

class ErrorEvents(utils.CommonCog):
    async def send_unhandled_msg(self,
        event: str,
        traceback: str,
        extra_fields: list[tuple[Any, Any, bool]],
        author_name: Optional[str] = None,
        icon_url: Optional[str] = None,
    ):
        error_webhook = self.bot.channels["errors"]
        row = await self.bot.pool.fetchrow("""
            UPDATE errors SET occurrences = occurrences + 1
            WHERE traceback = $1
            RETURNING *
        """, traceback)
        if row is not None:
            err_msg = await error_webhook.fetch_message(row["message_id"])
            err_msg.embeds[0].set_footer(text=f"This error has occurred {row['occurrences']} times!")
            return await err_msg.edit(embeds=err_msg.embeds)

        fields = [
            ("Event", event, True),
            ("Bot User", self.bot.user, True),
            BLANK, *extra_fields,
            ("Ram Usage", f"{psutil.virtual_memory().percent}%", True),
            ("Running Task Count", len(asyncio.all_tasks()),     True),
            ("Local Thread Count", psutil.Process().num_threads(), True),
        ]
        if self.bot.cluster_id is not None:
            fields.extend((
                ("Cluster ID", self.bot.cluster_id, True),
                ("Handling Shards", self.bot.shard_ids, True)
            ))

        error_msg = discord.Embed(title=traceback.split("\n")[-2][:256], colour=utils.RED)
        if author_name is not None:
            if icon_url is None:
                error_msg.set_author(name=author_name)
            else:
                error_msg.set_author(name=author_name, icon_url=icon_url)

        for name, value, inline in fields:
            value = value if value == "\u200B" else f"`{value}`"
            error_msg.add_field(name=name, value=value, inline=inline)

        view = utils.ShowTracebackView(f"```\n{traceback}```")
        err_msg = await error_webhook.send(embed=error_msg, view=view, wait=True)
        message_id = await self.bot.pool.fetchval("""
            INSERT INTO errors(traceback, message_id)
            VALUES($1, $2)

            ON CONFLICT (traceback)
            DO UPDATE SET occurrences = errors.occurrences + 1
            RETURNING errors.message_id
        """, traceback, err_msg.id)

        assert message_id is not None
        if message_id != err_msg.id:
            return await err_msg.delete()

        if self.bot.websocket is not None:
            ws_json = utils.data_to_ws_json("SEND", target="support", **{
                "c": "load_view",
                "a": {
                    "traceback": traceback,
                    "message_id": message_id
                }
            })
            await self.bot.websocket.send(ws_json)


    async def on_error(self, event_method: str, error: Optional[BaseException] = None, *targs: Any, **_):
        args = list(targs)
        if isinstance(error, BaseException):
            etype, value, tb = type(error), error, error.__traceback__
        else:
            args.insert(0, error)
            etype, value, tb = exc_info()

        if etype == BlockingIOError and "Resource temporarily unavailable" in str(value):
            self.bot.logger.error("BlockingIOError: Resource temporarily unavailable, killing everything!")
            return await self.bot.close(utils.KILL_EVERYTHING)

        icon_url: Optional[str] = None
        author_name: Optional[str] = None
        fields: list[tuple[Any, Any, bool]] = [] # name, value, inline

        if event_method == "on_message":
            message: utils.TypedMessage = args[0]
            if message.guild:
                fields.extend((
                    ("Guild Name", message.guild.name, True),
                    ("Guild ID", message.guild.id, True),
                ))

            fields.append(("Channel Type", type(message.channel).__name__, True))
            author_name, icon_url = str(message.author), message.author.display_avatar.url

        elif event_method in {"on_guild_join", "on_guild_remove"}:
            guild: discord.Guild = args[0]
            author_name, icon_url = guild.name, getattr(guild.icon, "url", None)

        elif event_method in {"play_audio", "fill_audio_buffer"}:
            vc: TTSVoiceClient = args[0]

            guild = vc.guild
            fields.append(("VC Repr", repr(vc), False))
            author_name, icon_url = guild.name, getattr(guild.icon, "url", None)

        await self.send_unhandled_msg(
            icon_url=icon_url,
            event=event_method,
            extra_fields=fields,
            author_name=author_name,
            traceback="".join(format_exception(etype, value, tb)),
        )

    @commands.Cog.listener()
    async def on_command_error(self, ctx: utils.TypedContext, error: commands.CommandError): # sourcery no-metrics skip
        command = f"`{ctx.prefix}{ctx.command}`"
        error = getattr(error, "original", error)

        if isinstance(error, commands.CommandNotFound):
            return
        elif isinstance(error, commands.NotOwner):
            if ctx.interaction:
                await ctx.send("You do not have permission to run this command!")

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

            await ctx.send_error(reason, fix)

        elif isinstance(error, commands.CommandOnCooldown):
            cooldown_error = await ctx.send_error(
                error=f"{command} is on cooldown",
                fix=f"try again in {error.retry_after:.1f} seconds"
            )

            if ctx.guild is not None and ctx.interaction is None:
                await asyncio.sleep(error.retry_after)
                ctx = cast(utils.TypedGuildContext, ctx)

                if cooldown_error:
                    await cooldown_error.delete()
                if ctx.bot_permissions().manage_messages:
                    await ctx.message.delete()

        elif isinstance(error, commands.NoPrivateMessage):
            await ctx.send_error(
                error=f"{command} cannot be used in private messages",
                fix=f"try running it on a server with {self.bot.user.name} in"
            )

        elif isinstance(error, commands.MissingPermissions):
            missing_perms = ", ".join(error.missing_permissions)
            await ctx.send_error(
                error="you cannot run this command",
                fix=f"ask for the following permissions: {missing_perms}"
            )

        elif isinstance(error, commands.BotMissingPermissions):
            await ctx.send_error(
                error=f"I cannot run {command} as I am missing permissions",
                fix=f"give me {', '.join(error.missing_permissions)}"
            )

        elif isinstance(error, commands.CheckFailure):
            if "global check" not in str(error):
                await ctx.send_error(
                    error="you ran this command in the wrong channel",
                    fix=f"do {ctx.prefix}channel get the channel that has been setup"
                )

        elif isinstance(error, discord.errors.Forbidden):
            self.bot.logger.error(f"`discord.errors.Forbidden` caused by {ctx.message.content} sent by {ctx.author}")
            await ctx.send_error(
                error="I encountered an unknown permission error",
                fix="give TTS Bot the required permissions"
            )

        elif isinstance(error, asyncio.TimeoutError):
            self.bot.logger.error(f"`asyncio.TimeoutError:` Unhandled in {ctx.command.qualified_name}")

        else:
            traceback = "".join(format_exception(type(error), error, error.__traceback__))
            fields = [
                ("Command", ctx.command.qualified_name, True),
                ("Slash Command", ctx.interaction is not None, True),
            ]

            channel = ("Channel Type", type(ctx.channel).__name__, True)
            if ctx.guild is not None:
                fields.extend((
                    BLANK,
                    ("Guild Name", ctx.guild.name, True),
                    ("Guild ID", ctx.guild.id, True),
                    channel
                ))
            else:
                fields.append(channel)

            await asyncio.gather(
                ctx.send_error(error="an unknown error occurred"),
                self.send_unhandled_msg(
                    event="command",
                    extra_fields=fields,
                    traceback=traceback,
                    author_name=str(ctx.author),
                    icon_url=ctx.author.display_avatar.url,
                )
            )
