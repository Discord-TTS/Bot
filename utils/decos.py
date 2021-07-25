from __future__ import annotations

import asyncio
from functools import wraps
from typing import Any, TYPE_CHECKING, Awaitable, Callable, Optional, TypeVar, cast

from .views import BoolView, ChannelSelector, GenericView
from .classes import CommonCog, TypedGuildContext
from .funcs import to_thread

if TYPE_CHECKING:
    from typing_extensions import ParamSpec

    _T = TypeVar("_T")
    _R = TypeVar("_R")
    _P = ParamSpec("_P")


def make_fancy(
    func: Callable[[CommonCog, TypedGuildContext, _T], Awaitable[_R]]
) -> Callable[[CommonCog, TypedGuildContext, Optional[_T]], Awaitable[Optional[_R]]]:

    async def wrapper(self: CommonCog, ctx: TypedGuildContext, value: Optional[Any] = None) -> Optional[Any]:
        if value is not None:
            return await func(self, ctx, value)

        type_to_convert = [*func.__annotations__.values()][1]
        if type_to_convert == "discord.TextChannel":
            select_view = GenericView.from_item(ChannelSelector, ctx)
            message = await ctx.reply("Select a channel!", view=select_view)
            select_view.message = message # type: ignore

        elif type_to_convert == "bool":
            bool_view = BoolView(ctx)
            await ctx.reply("What do you want to set this to?", view=bool_view)

        else:
            ctx.bot.logger.error(f"Unknown Conversion Type: {type_to_convert}")

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
