from __future__ import annotations

from typing import TYPE_CHECKING

from discord.ext import commands


if TYPE_CHECKING:
    from main import TTSBot


class CommonCog(commands.Cog):
    def __init__(self, bot: TTSBot, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.bot = bot
