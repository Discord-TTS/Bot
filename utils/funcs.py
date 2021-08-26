"Useful functions used throughout the bot"

from __future__ import annotations

import asyncio
import sys
from inspect import cleandoc
from typing import (TYPE_CHECKING, Any, Callable, Optional, Sequence, TypeVar,
                    Union)

import orjson

from .constants import OPTION_SEPERATORS, READABLE_TYPE

if TYPE_CHECKING:
    import re

    import aioredis
    import discord
    from typing_extensions import ParamSpec

    from .websocket_types import WS_TARGET
    _P = ParamSpec("_P")
    _R = TypeVar("_R")
    _T = TypeVar("_T")


_sep = OPTION_SEPERATORS[0]
def construct_unslotted(cls: type[_T], *args, **kwargs) -> _T:
    "Constructs cls without any slots, allowing attribute addition"
    # This is subclassing cls, with the name of cls, then constructing it
    # as subclassing deletes __slots__, this works perfectly, if a bit hacky
    return type(cls.__name__, (cls,), {})(*args, **kwargs) # type: ignore

def data_to_ws_json(command: str, target: Union[WS_TARGET, str], **kwargs: Any) -> bytes:
    "Turns arguments and kwargs into usable data for the WS IPC system"
    wsjson = {"c": command.lower(), "a": kwargs, "t": target}
    return orjson.dumps(wsjson)

def emoji_match_to_cleaned(match: re.Match[str]) -> str:
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
