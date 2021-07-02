from __future__ import annotations

from typing import Dict, TYPE_CHECKING

from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.analytics_buffer = utils.SafeDict()
    bot.add_cog(events_analytics(bot))

class events_analytics(utils.CommonCog):
    @commands.Cog.listener()
    async def on_command(self, ctx: utils.TypedContext):
        self.bot.analytics_buffer.add(ctx.command.qualified_name)

    @commands.Cog.listener()
    async def on_resumed(self):
        self.bot.analytics_buffer.add("on_resumed")

    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedMessage):
        if not message.guild:
            self.bot.analytics_buffer.add("on_dm")
        elif message.guild.me is None and not message.guild.unavailable:
            # Weird bug, gonna check on it for a while.
            self.bot.analytics_buffer.add("on_me_none")
