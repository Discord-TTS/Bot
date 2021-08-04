from __future__ import annotations

import asyncio
import collections
from functools import partial
from io import BytesIO
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, List, Literal,
                    Optional, Tuple, TypeVar, Union, overload)
from dataclasses import dataclass

import discord
from discord.ext import commands
from pydub import AudioSegment

from .funcs import to_thread

_T = TypeVar("_T")
if TYPE_CHECKING:
    from main import TTSBotPremium
    from player import TTSVoicePlayer


# Cleanup classes
class VoiceNotFound(Exception):
    pass

class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBotPremium):
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

class TTSAudioMaker:
    def __init__(self, bot: TTSBotPremium):
        self.bot = bot

    async def get_tts(self, text: str, voice: Tuple[str, str], max_length: float) -> Union[Tuple[bytes, float], Tuple[None, None]]:
        ogg = await self.bot.cache.get((text, *voice))
        if not ogg:
            ogg = await self.bot.gtts.get(text, voice_lang=voice)
            file_length = await to_thread(self.get_duration, ogg)

            if file_length >= max_length:
                return (None, None)

            await self.bot.cache.set((text, *voice), ogg)
        else:
            file_length = await to_thread(self.get_duration, ogg)

        return ogg, file_length

    def get_duration(self, audio_data: bytes) -> float:
        audio_file = BytesIO(audio_data)
        segment = AudioSegment.from_file_using_temporary_files(audio_file)
        return len(segment) / 1000 # type: ignore


@dataclass
class Voice:
    voice_name: str

    lang: str
    gender: str
    variant: str

    @property
    def tuple(self) -> Tuple[str, str]:
        return self.voice_name, self.lang

    @property
    def raw(self) -> str:
        return f"{self.lang} {self.variant}"

    @property
    def formatted(self) -> str:
        return f"{self.lang} - {self.variant} ({self.gender})"

    def __repr__(self):
        return f"<Voice {self.lang=} {self.variant=} {self.gender=}>"

    def __str__(self):
        return self.formatted

# Typed Classes for silencing type errors.
TextChannel = Union[discord.TextChannel, discord.DMChannel]
VoiceChannel = Union[discord.VoiceChannel, discord.StageChannel]
class TypedContext(commands.Context):
    prefix: str
    cog: CommonCog
    args: List[Any]
    bot: TTSBotPremium
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
            kwargs.pop("ephemeral", False)
            send = partial(super().send, reference=reference)
        elif not (self.interaction.response.is_done() or return_msg or "file" in kwargs):
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
