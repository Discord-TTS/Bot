from __future__ import annotations

import asyncio
from functools import partial, wraps
from typing import Awaitable, Callable, Optional, TypeVar, cast

from typing_extensions import ParamSpec

from .classes import CommonCog


Return = TypeVar("Return")
Params = ParamSpec("Params")

def handle_errors(func: Callable[Params, Awaitable[Optional[Return]]]) -> Callable[Params, Awaitable[Optional[Return]]]:
    async def wrapper(*args: Params.args, **kwargs: Params.kwargs) -> Optional[Return]:
        try:
            return await func(*args, **kwargs)
        except Exception as error:
            if isinstance(error, asyncio.CancelledError):
                raise

            self = cast(CommonCog, args[0])
            await self.bot.on_error(func.__name__, error)

        return None
    return wraps(func)(wrapper)

def run_in_executor(func: Callable[Params, Return]) -> Callable[Params, Awaitable[Return]]:
    def wrapper(*args: Params.args, **kwargs: Params.kwargs) -> Awaitable[Return]:
        self = cast(CommonCog, args[0])
        callable_func = partial(func, *args, **kwargs)

        return self.bot.loop.run_in_executor(None, callable_func)

    return wraps(func)(wrapper)
