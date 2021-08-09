from __future__ import annotations

import asyncio
from collections import defaultdict
from typing import (Literal, Optional, TYPE_CHECKING, Any, Dict, Generic, Iterable, List, Tuple,
                    TypeVar, Union)

from discord.ext import tasks

import utils


_T = TypeVar("_T")
_DK = TypeVar("_DK")
_CACHE_ITEM = Dict[Literal[
    "channel", "xsaid", "bot_ignore", "auto_join",
    "msg_length", "repeated_chars", "prefix",
    "blocked", "lang", "name", "default_lang"
], Any]

if TYPE_CHECKING:
    from main import TTSBot


def _unpack_id(identifer: Union[Iterable[_T], _T]) -> Tuple[_T, ...]:
    return tuple(identifer) if isinstance(identifer, Iterable) else (identifer,)

def setup(bot: TTSBot):
    bot.settings, bot.userinfo, bot.nicknames = (
        TableHandler(bot, broadcast=True, default_id=0,
        select="SELECT * FROM guilds WHERE guild_id = $1",
        delete="DELETE FROM guilds WHERE guild_id = $1",
        insert="""
            INSERT INTO guilds(guild_id, {setting})
            VALUES($1, $2)

            ON CONFLICT (guild_id)
            DO UPDATE SET {setting} = EXCLUDED.{setting}
        """,
    ), TableHandler(bot, broadcast=False, default_id=0,
        select="SELECT * FROM userinfo WHERE user_id = $1",
        delete="DELETE FROM userinfo WHERE user_id = $1",
        insert="""
            INSERT INTO userinfo(user_id, {setting})
            VALUES($1, $2)

            ON CONFLICT (user_id)
            DO UPDATE SET {setting} = EXCLUDED.{setting}
        """,
    ), TableHandler(bot, broadcast=False, default_id=(0, 0),
        select="SELECT * from nicknames WHERE guild_id = $1 and user_id = $2",
        delete="DELETE FROM nicknames WHERE guild_id = $1 and user_id = $2",
        insert="""
            INSERT INTO nicknames(guild_id, user_id, {setting})
            VALUES($1, $2, $3)

            ON CONFLICT (guild_id, user_id)
            DO UPDATE SET {setting} = EXCLUDED.{setting}
        """,
    ))

class TableHandler(Generic[_DK]):
    def __init__(self, bot: TTSBot, select: str, insert: str, delete: str, default_id: _DK, broadcast: bool):
        self.bot = bot
        self.pool = bot.pool
        bot.add_listener(self.on_invalidate_cache)

        self.broadcast = broadcast
        self.select_query = select
        self.insert_query = insert
        self.delete_query = delete
        self.default_id = default_id

        self._starting_write = False
        self._not_fully_fetched: List[_DK] = []
        self._cache: Dict[_DK, _CACHE_ITEM] = {}
        self.defaults: Optional[_CACHE_ITEM] = None

        self._write_deltas: defaultdict[_DK, _CACHE_ITEM] = defaultdict(dict)
        self._write_tasks: defaultdict[_DK, List[asyncio.Future[None]]] = defaultdict(list)


    async def on_invalidate_cache(self, identifier: _DK):
        if isinstance(identifier, list):
            identifier = tuple(identifier) # type: ignore

        self._cache.pop(identifier, None) # type: ignore

    async def _fetch_defaults(self) -> _CACHE_ITEM:
        row = await self.bot.pool.fetchrow(self.select_query, *_unpack_id(self.default_id))
        assert row is not None

        self.defaults = dict(row)
        return self.defaults


    def __getitem__(self, identifer: _DK):
        if identifer not in self._not_fully_fetched:
            return self._cache[identifer] # type: ignore

        raise KeyError

    def __setitem__(self, identifier: _DK, new_settings: _CACHE_ITEM):
        if identifier not in self._cache:
            self._cache[identifier] = {}
            self._not_fully_fetched.append(identifier)

        self._cache[identifier].update(new_settings)
        self._write_deltas[identifier].update(new_settings)

        self._write_tasks[identifier].append(asyncio.Future())
        if not self.insert_writes.is_running() and not self._starting_write:
            self._starting_write = True
            self.insert_writes.start()

    def __delitem__(self, identifier: _DK):
        del self._cache[identifier]
        self.bot.create_task(self.bot.pool.execute(
            self.delete_query, identifier
        ))


    async def get(self, identifer: _DK) -> _CACHE_ITEM:
        try:
            return self[identifer]
        except KeyError:
            return await self._fill_cache(identifer)

    async def set(self, identifer: _DK, new_settings: _CACHE_ITEM):
        self[identifer] = new_settings
        await self._write_tasks[identifer][-1]


    @tasks.loop(count=1)
    async def insert_writes(self):
        await asyncio.sleep(1)
        self._starting_write = False
        exceptions = [err for err in await asyncio.gather(*(
            self._insert_write(pending_id)
            for pending_id in self._write_tasks.keys()
        ), return_exceptions=True) if err is not None]

        self.bot.logger.debug(
            f"Inserted {len(self._write_tasks.keys())} change(s)"
            f" with {len(exceptions)} errors"
        )

        self._write_deltas = defaultdict(dict)
        self._write_tasks = defaultdict(list)

        await asyncio.gather(*(
            self.bot.on_error("insert_writes", err)
            for err in exceptions
        ))

    async def _insert_write(self, raw_identifier: _DK):
        identifier = _unpack_id(raw_identifier)
        queries: defaultdict[str, List[Tuple[Any, ...]]] = defaultdict(list)

        for setting, value in self._write_deltas.pop(raw_identifier).items():
            query = self.insert_query.format(setting=setting)
            queries[query].append((*identifier, value, ))

        async with self.pool.acquire() as conn:
            for query, args in queries.items():
                await conn.executemany(query, args)

        if self.bot.websocket is not None and self.broadcast:
            await self.bot.websocket.send(
                utils.data_to_ws_json("SEND", target="*", **{
                    "c": "invalidate_cache",
                    "a": {"identifer": raw_identifier},
                })
            )

        for fut in self._write_tasks[raw_identifier]:
            fut.set_result(None)

    async def _fill_cache(self, identifier: _DK) -> _CACHE_ITEM:
        record = await self.pool.fetchrow(self.select_query, *_unpack_id(identifier))
        if record is None:
            self._cache[identifier] = self.defaults or await self._fetch_defaults()
        else:
            self._cache[identifier] = dict(record)

        return self._cache[identifier]
