from __future__ import annotations

import asyncio
import collections
from typing import TYPE_CHECKING, Awaitable, Optional, TypeVar, Union

import discord
from discord.ext import commands


_T = TypeVar("_T")
if TYPE_CHECKING:
    from main import TTSBotPremium
    from player import TTSVoicePlayer


# Cleanup classes
class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBotPremium):
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
    bot: TTSBotPremium
    message: TypedMessage
    author: Union[TypedMember, discord.User]

    def reply(self, *args, **kwargs) -> Awaitable[discord.Message]:
        return self.send(*args, **kwargs)

class TypedGuildContext(TypedContext):
    guild: TypedGuild
    author: TypedMember
    channel: discord.TextChannel

    def author_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.author)
    def bot_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.guild.me)

    def reply(self, *args, **kwargs) -> Awaitable[discord.Message]:
        if self.bot_permissions().read_message_history:
            kwargs["reference"] = self.message

        return self.send(*args, **kwargs)

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
