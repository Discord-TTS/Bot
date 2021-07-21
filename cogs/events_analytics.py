from __future__ import annotations

from typing import TYPE_CHECKING

from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.analytics_buffer = utils.SafeDict()
    bot.add_cog(AnalyticsEvents(bot))

class AnalyticsEvents(utils.CommonCog):
    @commands.Cog.listener()
    async def on_command(self, ctx: utils.TypedContext):
        self.bot.log(ctx.command.qualified_name)

    @commands.Cog.listener()
    async def on_resumed(self):
        self.bot.log("on_resumed")
