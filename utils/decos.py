from __future__ import annotations

import asyncio
from functools import wraps
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, Coroutine,
                    Optional, TypeVar, Union, cast)

import discord
from discord.utils import as_chunks
from discord.ext import commands

from .classes import CommonCog, TypedGuildContext
from .views import BoolView, ChannelSelector, CommandView


if TYPE_CHECKING:
    from typing_extensions import ParamSpec

    _R = TypeVar("_R")
    _P = ParamSpec("_P")

error_lookup = {"bool": commands.BadBoolArgument, "TextChannel": commands.ChannelNotFound}

def make_fancy(
    func: Callable[[CommonCog, TypedGuildContext, Any], Awaitable[_R]]
) -> Callable[[CommonCog, TypedGuildContext, Optional[Union[discord.TextChannel, bool]]], Coroutine[Any, Any, Optional[_R]]]:

    async def wrapper(self: CommonCog, ctx: TypedGuildContext, value: Optional[Union[discord.TextChannel, bool]] = None) -> Optional[_R]:
        type_to_convert: str = [*func.__annotations__.values()][1].split(".")[-1]
        if value is not None or ctx.interaction is not None:
            if type(value).__name__ != type_to_convert:
                raise error_lookup[type_to_convert](str(value))

            return await func(self, ctx, value)

        if type_to_convert == "TextChannel":
            select_view = CommandView(ctx)
            for channels in as_chunks((channel
                for channel in ctx.guild.text_channels
                if (channel.permissions_for(ctx.guild.me).read_messages
                    and channel.permissions_for(ctx.author).read_messages)
            ), 25):
                select_view.add_item(ChannelSelector(ctx, channels))

            await ctx.reply("Select a channel!", view=select_view)
        elif type_to_convert == "bool":
            await ctx.reply("What do you want to set this to?", view=BoolView(ctx))
        else:
            ctx.bot.logger.error(f"Unknown Conversion Type: {type_to_convert}")

    wrapper.__original_func__ = func
    wrapper.__name__ = func.__name__
    wrapper.__doc__ = func.__doc__

    return wrapper

def handle_errors(func: Callable[_P, Awaitable[Optional[_R]]]) -> Callable[_P, Coroutine[Any, Any, Optional[_R]]]:
    async def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Optional[_R]:
        try:
            return await func(*args, **kwargs)
        except Exception as error:
            self = cast(CommonCog, args[0])
            await self.bot.on_error(func.__name__, error)

        return None
    return wraps(func)(wrapper)

def run_in_executor(func: Callable[_P, _R]) -> Callable[_P, Coroutine[Any, Any, _R]]:
    def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Coroutine[Any, Any, _R]:
        return asyncio.to_thread(func, *args, **kwargs)

    return wraps(func)(wrapper)
