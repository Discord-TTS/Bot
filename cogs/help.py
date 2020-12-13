import discord
from discord.ext import commands
from inspect import cleandoc

def setup(bot): pass

class FakeCog():
    def __init__(self, name):
        self.qualified_name = name

class FancyHelpCommand(commands.HelpCommand):
    COLOUR = 0x3498db

    def get_ending_note(self):
        return f"Use {self.clean_prefix}{self.invoked_with} [command] for more info on a command."

    def get_command_signature(self, command):
        cmd_usage = f"{self.clean_prefix}{command.qualified_name}"
        if command.signature:
            cmd_usage = f"{cmd_usage} {command.signature}"

        return cmd_usage

    def get_bot_mapping(self):
        bot = self.context.bot

        known_cogs_names = ("Main Commands", "Settings", "Extra Commands", "cmds_dev")

        known_cogs = [bot.get_cog(cog) for cog in known_cogs_names]
        unknown_cogs = [cog for cog in bot.cogs.values() if cog.__class__.__name__ not in known_cogs_names]

        cogs = known_cogs + unknown_cogs

        mapping = {
            cog: cog.get_commands()
            for cog in cogs
        }
        mapping[None] = [c for c in bot.all_commands.values() if c.cog is None]
        return mapping

    async def send_bot_help(self, mapping):
        description = ""
        for cog, commands in mapping.items():
            if cog is None:
                cog = FakeCog("Uncategoried")

            if commands:
                command_names = "\n".join(f"`{self.get_command_signature(c)}:` {c.short_doc or 'no description'}" for c in commands)
                description += f"**__{cog.qualified_name}__**\n{command_names}\n\n"

        embed = discord.Embed(
            title="TTS Bot Help!",
            description=description,
            colour=self.COLOUR
            )

        embed.set_author(name=self.context.author.display_name, icon_url=str(self.context.author.avatar_url))
        embed.set_footer(text=self.get_ending_note())
        await self.get_destination().send(embed=embed)

    async def send_group_help(self, group):
        description = group.help or ""

        if isinstance(group, commands.Group):
            description = "\n".join(f"`{self.get_command_signature(c)}:` {c.short_doc or 'no description'}" for c in group.commands)

        print(description)
        embed = discord.Embed(
            title=f"`{self.clean_prefix}{group.qualified_name}` Help!",
            description=description,
            colour=self.COLOUR
            )

        embed.set_footer(text=self.get_ending_note())
        await self.get_destination().send(embed=embed)

    send_command_help = send_group_help
    send_cog_help = send_bot_help
