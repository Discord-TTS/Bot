from __future__ import annotations

import asyncio
from functools import wraps
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, Optional, TypeVar,
                    Union, cast)

import discord

from .classes import CommonCog, TypedGuildContext
from .funcs import to_thread
from .views import BoolView, ChannelSelector, GenericView


_T = TypeVar("_T")
if TYPE_CHECKING:
    from typing_extensions import ParamSpec

    _R = TypeVar("_R")
    _P = ParamSpec("_P")


def make_fancy(
    func: Callable[[CommonCog, TypedGuildContext, _T], Awaitable[_R]]
) -> Callable[[CommonCog, TypedGuildContext, Optional[_T]], Awaitable[Optional[_R]]]:

    async def wrapper(self: CommonCog, ctx: TypedGuildContext, value: Optional[_T] = None) -> Optional[Any]:
        if value is not None:
            return await func(self, ctx, value)

        type_to_convert = [*func.__annotations__.values()][1]
        if type_to_convert == "discord.TextChannel":
            select_view = GenericView.from_item(ChannelSelector, ctx)
            select_view.message = await ctx.reply("Select a channel!", view=select_view) # type: ignore

        elif type_to_convert == "bool":
            bool_view = BoolView(ctx)
            bool_view.message = await ctx.reply("What do you want to set this to?", view=bool_view) # type: ignore

        else:
            ctx.bot.logger.error(f"Unknown Conversion Type: {type_to_convert}")

    wrapper.__annotations__["value"] = Union[discord.TextChannel, bool, str]
    wrapper.__name__ = func.__name__
    wrapper.__doc__ = func.__doc__

    return wrapper

def handle_errors(func: Callable[_P, Awaitable[Optional[_R]]]) -> Callable[_P, Awaitable[Optional[_R]]]:
    async def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Optional[_R]:
        try:
            return await func(*args, **kwargs)
        except Exception as error:
            if isinstance(error, (asyncio.CancelledError, RecursionError)):
                raise

            self = cast(CommonCog, args[0])
            await self.bot.on_error(func.__name__, error)

        return None
    return wraps(func)(wrapper)

def run_in_executor(func: Callable[_P, _R]) -> Callable[_P, Awaitable[_R]]:
    def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Awaitable[_R]:
        return to_thread(func, *args, **kwargs)

    return wraps(func)(wrapper)
