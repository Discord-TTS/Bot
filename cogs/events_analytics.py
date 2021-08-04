from __future__ import annotations

from typing import TYPE_CHECKING

from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBotPremium


def setup(bot: TTSBotPremium):
    bot.analytics_buffer = utils.SafeDict()
    bot.add_cog(AnalyticsEvents(bot))

class AnalyticsEvents(utils.CommonCog):
    @commands.Cog.listener()
    async def on_command(self, ctx: utils.TypedContext):
        self.bot.log(ctx.command.qualified_name)
        if ctx.interaction is None:
            self.bot.log("on_normal_command")

    @commands.Cog.listener()
    async def on_resumed(self):
        self.bot.log("on_resumed")
