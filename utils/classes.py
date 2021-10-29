from __future__ import annotations

import asyncio
from dataclasses import dataclass
import uuid
from io import BytesIO
from typing import (TYPE_CHECKING, Any, Awaitable, Callable, Literal, Optional,
                    TypeVar, Union, cast, overload, Tuple)

import discord
from discord.ext import commands
from pydub import AudioSegment

from .constants import AUDIODATA, RED
from .funcs import data_to_ws_json

if TYPE_CHECKING:
    import collections

    from main import TTSBotPremium
    from player import TTSVoiceClient

    from .websocket_types import WS_TARGET


_T = TypeVar("_T")
# Cleanup classes
class VoiceNotFound(Exception):
    pass

class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBotPremium):
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
    def __init__(self, bot: TTSBotPremium):
        self.bot = bot

    async def get_tts(self, text: str, voice: tuple[str, str], max_length: float) -> Union[AUDIODATA, tuple[None, None]]:
        ogg = await self.bot.cache.get((text, *voice))
        if not ogg:
            ogg = await self.bot.gtts.get(text, voice_lang=voice)
            file_length = await asyncio.to_thread(self.get_duration, ogg)

            if file_length >= max_length:
                return (None, None)

            await self.bot.cache.set((text, *voice), ogg)
        else:
            file_length = await asyncio.to_thread(self.get_duration, ogg)

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
VoiceChannel = Union[discord.VoiceChannel, discord.StageChannel]
GuildTextChannel = Union[discord.TextChannel, discord.Thread]
TextChannel = Union[GuildTextChannel, discord.DMChannel]
class TypedContext(commands.Context):
    prefix: str
    cog: CommonCog
    bot: TTSBotPremium
    channel: TextChannel
    message: TypedMessage
    command: commands.Command
    guild: Optional[TypedGuild]
    author: Union[discord.User, TypedMember]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.interaction: Optional[discord.Interaction] = None

    default_fix = "get in contact with us via the support server for help"
    async def send_error(self, error: str, fix: str = default_fix, **kwargs) -> Optional[discord.Message]:
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

        return await self.reply(embed=error_embed, ephemeral=True, **kwargs)

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
    async def send(self,
        content: Optional[str] = None,
        return_message: Literal[False] = False,
        **kwargs: Any
    ) -> Optional[Union[discord.WebhookMessage, discord.Message]]: ...
    @overload
    async def send(self,
        content: Optional[str] = None,
        return_message: Literal[True] = True,
        **kwargs: Any
    ) -> Union[discord.WebhookMessage, discord.Message]: ...

    async def send(self,
        content: Optional[str] = None,
        return_message: bool = False,
        **kwargs: Any
    ) -> Optional[Union[discord.WebhookMessage, discord.Message]]:
        view = None
        if "view" in kwargs:
            view = kwargs["view"]
            return_message = True

        msg = await super().send(content, return_message=return_message, **kwargs) # type: ignore
        if view is not None:
            view.message = msg

        return msg


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
    voice_client: Optional[TTSVoiceClient]
    fetch_member: Callable[[int], Awaitable[TypedMember]]


class TypedVoiceState(discord.VoiceState):
    channel: VoiceChannel

class TypedMessageReference(discord.MessageReference):
    resolved: Optional[Union[TypedMessage, discord.DeletedReferencedMessage]]
