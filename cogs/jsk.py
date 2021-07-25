from typing import Optional, cast

from discord.ext import commands


def setup(bot: commands.Bot):
    try:
        from jishaku import Jishaku
    except ModuleNotFoundError:
        print("jishaku not installed, -jsk will not be available")
    else:
        bot.add_cog(Jishaku(bot=bot))
        jsk = cast(Optional[commands.Command], bot.get_command("jsk"))

        assert jsk is not None
        jsk.hidden = True
