from __future__ import annotations

import asyncio
from functools import partial, wraps
from typing import Awaitable, Callable, Optional, TypeVar, cast

from typing_extensions import ParamSpec

from .classes import CommonCog


_R = TypeVar("_R")
_P = ParamSpec("_P")

def handle_errors(func: Callable[_P, Awaitable[Optional[_R]]]) -> Callable[_P, Awaitable[Optional[_R]]]:
    async def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Optional[_R]:
        try:
            return await func(*args, **kwargs)
        except Exception as error:
            if isinstance(error, asyncio.CancelledError):
                raise

            self = cast(CommonCog, args[0])
            await self.bot.on_error(func.__name__, error)

        return None
    return wraps(func)(wrapper)

def run_in_executor(func: Callable[_P, _R]) -> Callable[_P, Awaitable[_R]]:
    def wrapper(*args: _P.args, **kwargs: _P.kwargs) -> Awaitable[_R]:
        self = cast(CommonCog, args[0])
        callable_func = partial(func, *args, **kwargs)

        return self.bot.loop.run_in_executor(None, callable_func)

    return wraps(func)(wrapper)
