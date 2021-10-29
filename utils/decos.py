from __future__ import annotations

import asyncio
from functools import wraps
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, Coroutine,
                    Optional, TypeVar, cast)

from .classes import CommonCog, TypedGuildContext, Voice
from .views import BoolView

if TYPE_CHECKING:
    from typing_extensions import ParamSpec

    _R = TypeVar("_R")
    _P = ParamSpec("_P")


def bool_button(
    func: Callable[[CommonCog, TypedGuildContext, bool], Awaitable[_R]]
) -> Callable[[CommonCog, TypedGuildContext, Optional[bool]], Coroutine[Any, Any, Optional[_R]]]:
    async def wrapper(self: CommonCog, ctx: TypedGuildContext, value: Optional[bool] = None) -> Optional[_R]:
        if value is None:
            view = BoolView(ctx)
            await ctx.reply("What do you want to set this to?", view=view)
            value = await view.wait()

        return await func(self, ctx, value)

    wrapper.__name__ = func.__name__
    wrapper.__doc__ = func.__doc__
    return wrapper

def handle_errors(func: Callable[_P, Awaitable[Optional[_R]]]) -> Callable[_P, Coroutine[Any, Any, Optional[_R]]]:
    async def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Optional[_R]:
        try:
            return await func(*args, **kwargs)
        except Exception as error:
            self = cast(CommonCog, args[0])
            await self.bot.on_error(func.__name__, error, self)

        return None
    return wraps(func)(wrapper)

def require_voices(
    func: Callable[_P, Coroutine[Any, Any, _R]]
) -> Callable[_P, Coroutine[Any, Any, _R]]:

    @wraps(func)
    async def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> _R:
        self = cast(CommonCog, args[0])
        if not getattr(self.bot, "_voice_data", None):
            self.bot._voice_data = sorted(
                [Voice(
                    voice_name=v["name"],

                    variant=v["name"][-1].lower(),
                    lang=v["languageCodes"][0].lower(),
                    gender=v["ssmlGender"].capitalize()
                )
                for v in await self.bot.gtts.get_voices()
                if "Standard" in v["name"]],
                key=lambda v: v.formatted
            )

        return await func(*args, **kwargs)
    return wrapper

def run_in_executor(func: Callable[_P, _R]) -> Callable[_P, Coroutine[Any, Any, _R]]:
    def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Coroutine[Any, Any, _R]:
        return asyncio.to_thread(func, *args, **kwargs)

    return wraps(func)(wrapper)
