from __future__ import annotations

from typing import Any, Coroutine, Iterable, Type, cast

import discord

from .classes import TypedGuildContext, TypedMessage


class GenericView(discord.ui.View):
    @classmethod
    def from_item(cls,
        item: Type[discord.ui.Item[GenericView]],
        *args: Any, **kwargs: Any
    ):
        self = cls()
        self.add_item(item(*args, **kwargs))
        return self


class CommandView(GenericView):
    message: TypedMessage
    def __init__(self, ctx: TypedGuildContext, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.ctx = ctx

    @classmethod
    def from_item(cls,
        item: Type[discord.ui.Item[GenericView]],
        ctx: TypedGuildContext, *args: Any, **kwargs: Any
    ):
        self = cls(ctx)
        self.add_item(item(ctx, *args, **kwargs)) # type: ignore
        return self


    def _clean_args(self, *args: Any):
        return [arg for arg in args if arg is not None][1:]

    def recall_command(self, *args: Any) -> Coroutine[Any, Any, Any]:
        self.stop()
        return self.ctx.command(*self._clean_args(*self.ctx.args), *args) # type: ignore


    async def on_error(self, *args: Any) -> None:
        self.ctx.bot.dispatch("interaction_error", *args)

    async def interaction_check(self, interaction: discord.Interaction) -> bool:
        assert isinstance(interaction.user, discord.Member)

        if interaction.user != self.ctx.author:
            await interaction.response.send_message("You don't own this interaction!", ephemeral=True)
            return False

        if interaction.guild is not None and not interaction.permissions.administrator:
            await interaction.response.send_message("You don't have permission use this interaction!", ephemeral=True)
            return False

        return True


class BoolView(CommandView):
    @discord.ui.button(label="True", style=discord.ButtonStyle.success)
    async def yes(self, *_):
        await self.recall_command(True)

    @discord.ui.button(label="False", style=discord.ButtonStyle.danger)
    async def no(self, *_):
        await self.recall_command(False)

    def stop(self) -> None:
        super().stop()
        for button in self.children:
            button = cast(discord.ui.Button, button)
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
            await self.view.recall_command(channel)
            await self.view.message.delete()
