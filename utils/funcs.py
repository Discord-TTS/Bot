"Useful functions used throughout the bot"

from __future__ import annotations

import asyncio
import sys
from inspect import cleandoc
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, Literal, Optional,
                    Sequence, TypeVar, Union, overload)

from utils.constants import OPTION_SEPERATORS, READABLE_TYPE


if TYPE_CHECKING:
    import re

    import aioredis
    import discord
    from typing_extensions import ParamSpec

    _P = ParamSpec("_P")
    _R = TypeVar("_R")


_sep = OPTION_SEPERATORS[2]

def emoji_match_to_cleaned(match: re.Match) -> str:
    emoji_name: str
    is_animated, emoji_name = bool(match[1]), match.group(2)

    emoji_prefix = "animated " * is_animated + "emoji "
    return emoji_prefix + emoji_name

def exts_to_format(attachments: Sequence[discord.Attachment]) -> Optional[str]:
    "Returns a description of the given attachment(s)"
    if not attachments:
        return None

    if len(attachments) >= 2:
        return "multiple files"

    ext = attachments[0].filename.split(".")[-1]
    returned_format_gen = (file_type for exts, file_type in READABLE_TYPE.items() if ext in exts)

    return next(returned_format_gen, "a file")

async def get_redis_info(cache_db: aioredis.Redis) -> str:
    rstats = await cache_db.info("stats")
    hits: int = rstats["keyspace_hits"]
    misses: int = rstats["keyspace_misses"]

    # Redis is actually stupid, so stats reset on server restart... :(
    if not (hits and misses):
        return ""

    total_queries = hits + misses
    hit_rate = (hits / (total_queries)) * 100
    return cleandoc(f"""
        Redis Info:
        {_sep} `Total Queries: {total_queries}`
        {_sep} `Hit Rate:      {hit_rate:.2f}%`

        {_sep} `Key Hits:      {hits}`
        {_sep} `Key Misses:    {misses}`
    """)

@overload
def to_async(
    coro: Awaitable[_R],
    loop: Optional[asyncio.AbstractEventLoop] = None,
    return_result: Literal[True] = True
) -> _R: ...
@overload
def to_async(
    coro: Awaitable[Any],
    loop: Optional[asyncio.AbstractEventLoop] = None,
    return_result: Literal[False] = False
) -> None: ...

def to_async(
    coro: Awaitable[Union[_R, Any]],
    loop: Optional[asyncio.AbstractEventLoop] = None,
    return_result: bool = True
) -> Optional[_R]:
    """Gets to an async env and returns the coro's result
    Notes: Can be used for swapping threads, if loop is passed."""

    if not loop:
        loop = asyncio.get_event_loop()
        if loop.is_running():
            raise RuntimeError

        result = loop.run_until_complete(coro)
        return result if return_result else None

    future = asyncio.run_coroutine_threadsafe(coro, loop)
    return future.result() if return_result else None


if sys.version_info >= (3, 9):
    to_thread = asyncio.to_thread
    removeprefix = str.removeprefix
else:
    # For the people running older than 3.9, these are some functions
    # backported, may not be 100% accurate but get the job done.
    async def to_thread(func: Callable[_P, _R]) -> _R:
        "asyncio.to_thread but for older python versions"
        return await asyncio.get_event_loop().run_in_executor(None, func)
    def removeprefix(self: str, __prefix: str) -> str:
        "str.removeprefix but for older python versions"
        return self[len(__prefix):] if self.startswith(__prefix) else self
