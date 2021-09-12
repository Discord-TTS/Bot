from __future__ import annotations

from functools import partial
from typing import Any, Generic, Iterable, Optional, TypeVar

import discord

from .classes import TypedGuildContext

_T = TypeVar("_T")
class CommandView(Generic[_T], discord.ui.View):
    ret: _T
    def __init__(self, ctx: TypedGuildContext, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.message: Optional[discord.Message] = None
        self.ctx = ctx


    async def on_error(self, *args: Any) -> None:
        await self.ctx.bot.on_error("on_interaction", *args)

    def stop(self, ret: _T) -> None:
        self.ret = ret
        return super().stop()

    async def wait(self) -> _T:
        await super().wait()
        return self.ret

    async def interaction_check(self, interaction: discord.Interaction) -> bool:
        assert isinstance(interaction.user, discord.Member)

        if interaction.user != self.ctx.author:
            await interaction.response.send_message("You don't own this interaction!", ephemeral=True)
            return False

        if interaction.guild is not None and not interaction.permissions.administrator:
            await interaction.response.send_message("You don't have permission use this interaction!", ephemeral=True)
            return False

        return True


class BoolView(CommandView[bool]):
    children: list[discord.ui.Button]
    def __init__(self, ctx: TypedGuildContext, true: str = "True", false: str = "False", *args, **kwargs):
        super().__init__(ctx, *args, **kwargs)

        async def button_click(*_, value: bool):
            self.stop(value)

        for label, style, value in ((true, discord.ButtonStyle.success, True), (false, discord.ButtonStyle.danger, False)):
            button = discord.ui.Button(label=label, style=style)
            button.callback = partial(button_click, value=value)
            self.add_item(button)

    def stop(self, ret: bool) -> None:
        super().stop(ret)
        if self.message is not None:
            for button in self.children:
                button.disabled = True

            self.ctx.bot.create_task(self.message.edit(view=self))

class ShowTracebackView(discord.ui.View):
    def __init__(self, traceback: str, *args, **kwargs):
        super().__init__(*args, **kwargs, timeout=None)
        self.traceback = traceback

    @discord.ui.button(label="Show Traceback", style=discord.ButtonStyle.danger, custom_id="ShowTracebackView:show_tb")
    async def show_tb(self, _, interaction: discord.Interaction):
        await interaction.response.send_message(self.traceback, ephemeral=True)


class ChannelSelector(discord.ui.Select):
    view: CommandView
    def __init__(self,
        ctx: TypedGuildContext,
        channels: Iterable[discord.abc.GuildChannel],
    *args: Any, **kwargs: Any):

        self.ctx = ctx
        self.channels = channels
        super().__init__(*args, **kwargs, options=[
            discord.SelectOption(
                label=f"""#{
                    (channel.name[:21] + '...')
                    if len(channel.name) >= 25
                    else channel.name
                }""",
                value=str(channel.id)
            )
            for channel in channels
        ])

    async def callback(self, interaction: discord.Interaction):
        channel_id = int(self.values[0])

        channel = self.ctx.guild.get_channel(channel_id)
        if channel is None and channel_id in [c.id for c in self.channels]:
            self.ctx.bot.logger.error(f"Couldn't find channel: {channel}")

            err = "Sorry, but that channel has been deleted!"
            await interaction.response.send_message(err, ephemeral=True)

            self.options = [option for option in self.options if option.value != self.values[0]]
            self.view.message = await self.view.message.edit(view=self.view) # type: ignore
        else:
            if self.view.message is not None:
                await self.view.message.delete()

            self.view.stop(channel)
