from __future__ import annotations

import asyncio
from functools import partial
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, List, Literal,
                    Optional, TypeVar, Union, overload)

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
    cog: CommonCog
    args: List[Any]
    channel: TextChannel
    message: TypedMessage
    guild: Optional[TypedGuild]
    author: Union[discord.User, TypedMember]
    command: Union[TypedCommand, commands.Group]
    invoked_subcommand: Optional[commands.Command]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.interaction: Optional[discord.Interaction] = None

    @overload
    async def send(self, *args: Any, return_msg: Literal[False] = False, **kwargs: Any) -> None: ...
    @overload
    async def send(self, *args: Any, return_msg: Literal[True] = True, **kwargs: Any) -> discord.Message: ...

    async def send(self,*args: Any, return_msg: bool = False, **kwargs: Any) -> Optional[discord.Message]:
        reference = kwargs.pop("reference", None)

        if self.interaction is None:
            send = partial(super().send, reference=reference)
        elif not self.interaction.response.is_done() and not return_msg:
            send = self.interaction.response.send_message
        else:
            if not self.interaction.response.is_done():
                await self.interaction.response.defer()

            send = partial(self.interaction.followup.send, wait=return_msg)

        return await send(*args, **kwargs)

    def reply(self, *args: Any, **kwargs: Any):
        return self.send(*args, **kwargs)

class TypedGuildContext(TypedContext):
    guild: TypedGuild
    author: TypedMember
    channel: discord.TextChannel

    def author_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.author)
    def bot_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.guild.me)

    def reply(self, *args: Any, **kwargs: Any):
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
    name: str
    help: Optional[str]
    qualified_name: str
