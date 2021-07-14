from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Optional, Union, TypeVar

import discord
from discord.ext import commands


_T = TypeVar("_T")
if TYPE_CHECKING:
    import collections

    from main import TTSBot
    from player import TTSVoicePlayer


# Cleanup classes
class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBot):
        self.bot = bot

class SafeDict(dict):
    def add(self, event):
        if event not in self:
            self[event] = 0

        self[event] += 1

class ClearableQueue(asyncio.Queue[_T]):
    _queue: collections.deque

    def clear(self):
        self._queue.clear()


# Typed Classes for silencing type errors.
VoiceChannel = Union[discord.VoiceChannel, discord.StageChannel]
class TypedContext(commands.Context):
    bot: TTSBot
    message: TypedMessage
    author: Union[TypedMember, discord.User]

    async def reply(self, *args, **kwargs) -> discord.Message:
        if not self.guild or self.channel.permissions_for(self.guild.me).read_message_history: # type: ignore
            return await super().reply(*args, **kwargs)

        return await super().send(*args, **kwargs)

class TypedGuildContext(TypedContext):
    guild: TypedGuild


class TypedMessage(discord.Message):
    guild: Optional[TypedGuild]
    author: Union[TypedMember, discord.User]

class TypedGuildMessage(TypedMessage):
    guild: TypedGuild
    author: TypedMember
    channel: discord.TextChannel


class TypedMember(discord.Member):
    guild: TypedGuild
    voice: Optional[TypedVoiceState]

class TypedGuild(discord.Guild):
    voice_client: Optional[TTSVoicePlayer]

class TypedVoiceState(discord.VoiceState):
    channel: VoiceChannel
