from __future__ import annotations

from typing import Optional, TYPE_CHECKING, Union

import discord
from discord.ext import commands


if TYPE_CHECKING:
    from main import TTSBot
    from player import TTSVoicePlayer


# Cleanup classes
class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBot, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.bot = bot


# Typed Classes for silencing type errors.
class TypedContext(commands.Context):
    bot: TTSBot
    author: Union[TypedMember, TypedUser]

class TypedGuildContext(TypedContext):
    guild: TypedGuild


class TypedMessage(discord.Message):
    guild: TypedGuild
    author: Union[TypedMember, TypedUser]

class TypedGuildMessage(TypedMessage):
    channel: discord.TextChannel


class TypedUser(discord.User):
    guild: TypedGuild

class TypedMember(discord.Member):
    guild: TypedGuild
    voice: TypedVoiceState


class TypedGuild(discord.Guild):
    voice_client: TTSVoicePlayer

class TypedVoiceState(discord.VoiceState):
    channel: Optional[Union[discord.VoiceChannel, discord.StageChannel]]
