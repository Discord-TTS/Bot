from __future__ import annotations

import asyncio
from typing import Any, List, TYPE_CHECKING, Awaitable, Callable, Optional, TypeVar, Union

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

class SafeDict(dict[str, int]):
    def add(self, event: str):
        if event not in self:
            self[event] = 0

        self[event] += 1

class ClearableQueue(asyncio.Queue[_T]):
    _queue: collections.deque[_T]

    def clear(self):
        self._queue.clear()


# Typed Classes for silencing type errors.
TextChannel = Union[discord.TextChannel, discord.DMChannel]
VoiceChannel = Union[discord.VoiceChannel, discord.StageChannel]
class TypedContext(commands.Context):
    bot: TTSBot
    prefix: str
    args: List[Any]
    channel: TextChannel
    message: TypedMessage
    command: TypedCommand
    guild: Optional[TypedGuild]
    author: Union[discord.User, TypedMember]
    invoked_subcommand: Optional[commands.Command]

    def reply(self, *args: Any, **kwargs: Any) -> Awaitable[discord.Message]:
        return self.send(*args, **kwargs)

class TypedGuildContext(TypedContext):
    guild: TypedGuild
    author: TypedMember
    channel: discord.TextChannel

    def author_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.author)
    def bot_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.guild.me)

    def reply(self, *args: Any, **kwargs: Any) -> Awaitable[discord.Message]:
        if self.bot_permissions().read_message_history:
            kwargs["reference"] = self.message

        return self.send(*args, **kwargs)


class TypedMessage(discord.Message):
    content: str
    guild: Optional[TypedGuild]
    author: Union[TypedMember, discord.User]

class TypedGuildMessage(TypedMessage):
    guild: TypedGuild
    author: TypedMember
    channel: discord.TextChannel


class TypedMember(discord.Member):
    guild: TypedGuild
    voice: TypedVoiceState
    avatar: discord.Asset

class TypedGuild(discord.Guild):
    owner_id: int
    voice_client: Optional[TTSVoicePlayer]
    fetch_member: Callable[[int], Awaitable[TypedMember]]

class TypedVoiceState(discord.VoiceState):
    channel: VoiceChannel

class TypedDMChannel(discord.DMChannel):
    recipient: discord.User

class TypedCommand(commands.Command):
    qualified_name: str
