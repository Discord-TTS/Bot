from __future__ import annotations

from typing import Optional, TYPE_CHECKING, Union
from attr import dataclass

import discord
from discord.ext import commands


if TYPE_CHECKING:
    from main import TTSBot
    from player import TTSVoicePlayer


# Cleanup classes
@dataclass
class CommonCog(commands.Cog):
    bot: TTSBot


# Typed Classes for silencing type errors.
class TypedContext(commands.Context):
    bot: TTSBot
    author: Union[TypedMember, discord.User]

    async def reply(self, *args, **kwargs) -> discord.Message:
        if not self.guild or self.channel.permissions_for(self.guild.me).read_message_history: # type: ignore
            return await super().reply(*args, **kwargs)
        else:
            return await super().send(*args, **kwargs)

class TypedGuildContext(TypedContext):
    guild: TypedGuild


class TypedMessage(discord.Message):
    guild: Optional[TypedGuild]
    author: Union[TypedMember, discord.User]

class TypedGuildMessage(TypedMessage):
    guild: TypedGuild
    channel: discord.TextChannel


class TypedMember(discord.Member):
    guild: TypedGuild
    voice: Optional[TypedVoiceState]


class TypedGuild(discord.Guild):
    voice_client: Optional[TTSVoicePlayer]

class TypedVoiceState(discord.VoiceState):
    channel: Optional[Union[discord.VoiceChannel, discord.StageChannel]]
