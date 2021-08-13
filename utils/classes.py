from __future__ import annotations

import asyncio
from functools import partial
from io import BytesIO
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, List, Literal,
                    Optional, Tuple, TypeVar, Union, overload)

import discord
import voxpopuli
from discord.ext import commands
from mutagen import mp3 as mutagen
from pydub import AudioSegment

from .constants import GTTS_ESPEAK_DICT
from .funcs import to_thread

_T = TypeVar("_T")
if TYPE_CHECKING:
    import collections

    from main import TTSBot
    from player import TTSVoicePlayer


# Cleanup classes
class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBot):
        self.bot = bot

class ClearableQueue(asyncio.Queue[_T]):
    _queue: collections.deque[_T]

    def clear(self):
        self._queue.clear()

class SafeDict(dict[str, int]):
    def add(self, event: str):
        if event not in self:
            self[event] = 0

        self[event] += 1

class TTSAudioMaker:
    def __init__(self, bot: TTSBot):
        self.bot = bot

    async def get_tts(self, text: str, lang: str, max_length: Union[float, int]) -> Union[Tuple[bytes, float], Tuple[None, None]]:
        mode = "wav" if self.bot.blocked else "mp3"
        audio = await self.bot.cache.get((mode, text, lang))
        if audio is None:
            try:
                coro = self.get_espeak if self.bot.blocked else self.get_gtts
                audio = await asyncio.wait_for(coro(text, lang), timeout=10)
            except asyncio.TimeoutError:
                self.bot.log("on_generate_timeout")
                raise

            if audio is None:
                return None, None

            try:
                file_length = await to_thread(self.get_duration, audio, mode)
            except mutagen.HeaderNotFoundError:
                return None, None

            if file_length > max_length:
                return self.bot.log("on_above_max_length"), None

            await self.bot.cache.set((mode, text, lang), audio)
        else:
            file_length = await to_thread(self.get_duration, audio, mode)

        return audio, file_length


    async def get_gtts(self, text: str, lang: str) -> bytes:
        mp3 = await self.bot.gtts.get(text=text, lang=lang)
        self.bot.log("on_gtts_complete")
        return mp3

    async def get_espeak(self, text: str, lang: str) -> bytes:
        if text.startswith("-") and " " not in text:
            text += " " # fix espeak hang

        lang = GTTS_ESPEAK_DICT.get(lang, "en")
        wav = await voxpopuli.Voice(lang=lang, speed=130, volume=2).to_audio(text)

        self.bot.log("on_espeak_complete")
        return wav


    def get_duration(self, audio_data: bytes, mode: Literal["mp3", "wav"]) -> float:
        audio_file = BytesIO(audio_data)
        if mode == "mp3":
            return mutagen.MP3(audio_file).info.length

        segment = AudioSegment.from_file_using_temporary_files(audio_file)
        return len(segment) / 1000 # type: ignore


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
            if isinstance(reference, discord.Message):
                reference = reference.to_reference(fail_if_not_exists=False)

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
        if self.interaction is not None:
            return self.interaction.permissions

        return self.channel.permissions_for(self.author)
    def bot_permissions(self) -> discord.Permissions:
        return self.channel.permissions_for(self.guild.me)

    def reply(self, *args: Any, **kwargs: Any):
        if self.bot_permissions().read_message_history:
            kwargs["reference"] = self.message

        return self.send(*args, **kwargs)


class TypedMessage(discord.Message):
    guild: Optional[TypedGuild]
    author: Union[TypedMember, discord.User]

class TypedGuildMessage(TypedMessage):
    guild: TypedGuild
    channel: Union[discord.TextChannel, discord.Thread]


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
