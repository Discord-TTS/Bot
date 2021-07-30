from __future__ import annotations

from typing import Any, Coroutine, Type

import discord

from .classes import TypedGuildContext, TypedMessage


class GenericView(discord.ui.View):
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
        return self.ctx.command(*self._clean_args(*self.ctx.args), *args)

    async def on_error(self, *args: Any) -> None:
        self.ctx.bot.dispatch("interaction_error", *args)

class BoolView(GenericView):
    def __init__(self, ctx: TypedGuildContext, *args: Any, **kwargs: Any):
        super().__init__(ctx, *args, **kwargs)

    @discord.ui.button(label="True", style=discord.ButtonStyle.success)
    async def yes(self, *_):
        await self.recall_command(True)

    @discord.ui.button(label="False", style=discord.ButtonStyle.danger)
    async def no(self, *_):
        await self.recall_command(False)


class GenericItemMixin:
    view: GenericView

class ChannelSelector(GenericItemMixin, discord.ui.Select):
    def __init__(self, ctx: TypedGuildContext, *args: Any, **kwargs: Any):
        self.ctx = ctx
        self.channels = ctx.guild.text_channels

        discord.ui.Select.__init__(self, *args, **kwargs, options=[
            discord.SelectOption(
                label=f"""#{
                    (channel.name[:22] + '..')
                    if len(channel.name) > 25
                    else channel.name
                }""",
                value=str(channel.id)
            )
            for channel in self.channels
        ])

    async def callback(self, interaction: discord.Interaction):
        channel = self.ctx.guild.get_channel(int(self.values[0]))

        if channel is None:
            self.ctx.bot.logger.error(f"Couldn't find channel: {channel}")

            err = f"Sorry, but that channel has been deleted!"
            await interaction.response.send_message(err, ephemeral=True)

            new_view = GenericView.from_item(self.__class__, self.ctx)
            await self.view.message.edit(view=new_view)
        else:
            await self.view.recall_command(channel)
            await self.view.message.delete()
