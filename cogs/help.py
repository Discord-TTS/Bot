from __future__ import annotations

from typing import Dict, TYPE_CHECKING, List, Mapping, Optional, Union

import discord
from discord.ext import commands
from utils import NETURAL_COLOUR

if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(FancyHelpCommandCog(bot))


class FakeCog:
    def __init__(self, name: str):
        self.qualified_name = name


class FancyHelpCommandCog(commands.Cog, name="Uncategoried"):
    def __init__(self, bot: TTSBot):
        help_command = FancyHelpCommand()

        bot.help_command = help_command
        bot.help_command.cog = self
        bot.help_command.add_check(
            commands.bot_has_permissions(
                send_messages=True, embed_links=True
            ).predicate # type: ignore
        )


class FancyHelpCommand(commands.HelpCommand):
    if TYPE_CHECKING:
        import utils
        context: utils.TypedContext

    def __init__(self, *args, **kwargs):
        kwargs["verify_checks"] = False
        super().__init__(*args, **kwargs)

    def get_ending_note(self, is_group: bool = False):
        args = " ".join(self.context.message.content.split()[1:]) if is_group else ""
        return f"Use {self.context.clean_prefix}{self.invoked_with}{f' {args} '}[command] for more info on a command."

    def get_command_signature(self, command: commands.Command) -> str:
        cmd_usage = f"{self.context.clean_prefix}{command.qualified_name}"
        cmd_usage += f" {command.signature}" if command.signature else ""

        return cmd_usage

    def get_bot_mapping(self):
        bot = self.context.bot

        known_cogs_names = ("Main Commands", "Settings", "Extra Commands")

        known_cogs = [bot.get_cog(cog) for cog in known_cogs_names]
        unknown_cogs = [cog for cog in bot.cogs.values() if cog not in known_cogs]
        cogs: List[commands.Cog] = known_cogs + unknown_cogs # type: ignore

        mapping: Dict[Optional[commands.Cog], List[commands.Command]]

        mapping = {cog: cog.get_commands() for cog in cogs}
        mapping[None] = [c for c in bot.all_commands.values() if c.cog is None]
        return mapping

    async def send_bot_help(self, mapping: Mapping[Optional[commands.Cog], List[commands.Command]]) -> None:
        description = ""
        for cog, commands in mapping.items():
            if cog is None:
                cog = FakeCog("Uncategoried")

            commands = await self.filter_commands(commands)
            if commands:
                command_names = "\n".join(f"`{self.get_command_signature(c)}:` {c.short_doc or 'no description'}" for c in commands)
                description += f"**__{cog.qualified_name}__**\n{command_names}\n\n"

        embed = discord.Embed(
            title="TTS Bot Help!",
            description=description,
            colour=NETURAL_COLOUR
        )

        embed.set_author(name=self.context.author.display_name, icon_url=self.context.author.avatar.url)
        embed.set_footer(text=self.get_ending_note())
        await self.get_destination().send(embed=embed)

    async def send_group_help(self, group: Union[commands.Command, commands.Group]) -> None:
        if isinstance(group, commands.Group):
            group_commands = await self.filter_commands(group.commands, sort=True)
            description = "\n".join(f"`{self.get_command_signature(c)}:` {c.short_doc or 'no description'}" for c in group_commands)
        else:
            description = f"{group.help}\n```{self.context.clean_prefix}{group.qualified_name} {group.signature}```"

        embed = discord.Embed(
            title=f"`{self.context.clean_prefix}{group.qualified_name}` Help!",
            description=description,
            colour=NETURAL_COLOUR
        )

        embed.set_footer(text=self.get_ending_note(isinstance(group, commands.Group)))
        await self.get_destination().send(embed=embed)

    async def send_cog_help(self, cog: commands.Cog) -> None:
        await super().send_error_message(self.command_not_found(cog.qualified_name)) # type: ignore

    send_command_help = send_group_help
