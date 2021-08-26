from __future__ import annotations

import asyncio
import uuid
from functools import partial
from io import BytesIO
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, Literal, Optional,
                    TypeVar, Union, cast, overload)

import discord
import voxpopuli
from discord.ext import commands
from mutagen import mp3 as mutagen
from pydub import AudioSegment

from .constants import AUDIODATA, GTTS_ESPEAK_DICT, RED
from .funcs import data_to_ws_json, to_thread

if TYPE_CHECKING:
    import collections

    from main import TTSBot
    from player import TTSVoicePlayer

    from .websocket_types import WS_TARGET


_T = TypeVar("_T")
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

    async def get_tts(self, text: str, lang: str, max_length: Union[float, int]) -> Union[AUDIODATA, tuple[None, None]]:
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
                audio_file = BytesIO(audio)
                file_length = await to_thread(self.get_duration, audio_file, mode)
            except mutagen.HeaderNotFoundError:
                return None, None

            if file_length > max_length:
                return self.bot.log("on_above_max_length"), None

            await self.bot.cache.set((mode, text, lang), audio)
        else:
            audio_file = BytesIO(audio)
            file_length = await to_thread(self.get_duration, audio_file, mode)

        audio_file.seek(0)
        return audio_file, file_length


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


    def get_duration(self, audio_file: BytesIO, mode: Literal["mp3", "wav"]) -> float:
        if mode == "mp3":
            return mutagen.MP3(audio_file).info.length

        segment = AudioSegment.from_file_using_temporary_files(audio_file)
        return len(segment) / 1000 # type: ignore


# Typed Classes for silencing type errors.
VoiceChannel = Union[discord.VoiceChannel, discord.StageChannel]
GuildTextChannel = Union[discord.TextChannel, discord.Thread]
TextChannel = Union[GuildTextChannel, discord.DMChannel]
class TypedContext(commands.Context):
    bot: TTSBot
    prefix: str
    cog: CommonCog
    channel: TextChannel
    message: TypedMessage
    command: commands.Command
    guild: Optional[TypedGuild]
    author: Union[discord.User, TypedMember]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.interaction: Optional[discord.Interaction] = None

    async def send_error(self, error: str,
        fix: str = "get in contact with us via the support server for help",
    ) -> Optional[discord.Message]:

        if self.guild:
            self = cast(TypedGuildContext, self)
            permissions = self.bot_permissions()

            if not permissions.send_messages:
                return

            if not permissions.embed_links:
                msg = "An Error Occurred! Please give me embed links permissions so I can tell you more!"
                return await self.reply(msg, ephemeral=True)

        error_embed = discord.Embed(
            colour=RED,
            title="An Error Occurred!",
            description=f"Sorry but {error}, to fix this, please {fix}!"
        ).set_author(
            name=self.author.display_name,
            icon_url=self.author.display_avatar.url
        ).set_footer(
            text="Support Server: https://discord.gg/zWPWwQC"
        )

        return await self.reply(embed=error_embed, ephemeral=True)

    # I wish this could be data: List[_T] -> ...Dict[_T, Any] but no, typing bad
    async def request_ws_data(self, *to_request: str, target: WS_TARGET = "*", args: dict[str, dict[str, Any]] = None) -> Optional[list[dict[str, Any]]]:
        assert self.bot.websocket is not None

        args = args or {}
        ws_uuid = uuid.uuid4()
        wsjson = data_to_ws_json(
            command="REQUEST", target=target,
            info=to_request, args=args, nonce=ws_uuid
        )

        await self.bot.websocket.send(wsjson)
        try:
            check = lambda _, nonce: uuid.UUID(nonce) == ws_uuid
            return (await self.bot.wait_for(timeout=10, check=check, event="response"))[0]
        except asyncio.TimeoutError:
            self.bot.logger.error("Timed out requesting data from WS!")
            await self.send_error("the bot timed out fetching this info")


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
    channel: GuildTextChannel

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
    reference: Optional[TypedMessageReference]

class TypedGuildMessage(TypedMessage):
    guild: TypedGuild
    channel: GuildTextChannel


class TypedMember(discord.Member):
    guild: TypedGuild
    voice: TypedVoiceState

class TypedGuild(discord.Guild):
    owner_id: int
    voice_client: Optional[TTSVoicePlayer]
    fetch_member: Callable[[int], Awaitable[TypedMember]]


class TypedVoiceState(discord.VoiceState):
    channel: VoiceChannel

class TypedMessageReference(discord.MessageReference):
    resolved: Optional[Union[TypedMessage, discord.DeletedReferencedMessage]]
