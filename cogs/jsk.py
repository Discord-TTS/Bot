from __future__ import annotations
from typing import Optional, TYPE_CHECKING, cast

from discord.ext import commands


if TYPE_CHECKING:
    from main import TTSBotPremium


def setup(bot: TTSBotPremium):
    try:
        from jishaku import Jishaku
    except ModuleNotFoundError:
        print("jishaku not installed, -jsk will not be available")
    else:
        bot.add_cog(Jishaku(bot=bot))
        jsk = cast(Optional[commands.Command], bot.get_command("jsk"))

        assert jsk is not None
        jsk.hidden = True
