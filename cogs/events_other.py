from __future__ import annotations

import asyncio
from inspect import cleandoc
from typing import TYPE_CHECKING, cast

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


WELCOME_MESSAGE = cleandoc("""
    Hello! Someone invited me to your server `{guild}`!
    TTS Bot is a text to speech bot, as in, it reads messages from a text channel and speaks it into a voice channel

    **Most commands need to be done on your server, such as `{prefix}setup` and `{prefix}join`**

    I need someone with the administrator permission to do `{prefix}setup #channel`
    You can then do `{prefix}join` in that channel and I will join your voice channel!
    Then, you can just type normal messages and I will say them, like magic!

    You can view all the commands with `{prefix}help`
    Ask questions by either responding here or asking on the support server!
""")

def setup(bot: TTSBot):
    bot.add_cog(OtherEvents(bot))

class OtherEvents(utils.CommonCog):

    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedGuildMessage):
        if message.content in (self.bot.user.mention, f"<@!{self.bot.user.id}>"):
            await message.channel.send(f"Current Prefix for this server is: `{await self.bot.command_prefix(self.bot, message)}`")

        if message.reference and message.guild == self.bot.support_server and message.channel.name in ("dm_logs", "suggestions") and not message.author.bot:
            dm_message = message.reference.resolved
            if (
                not dm_message
                or isinstance(dm_message, discord.DeletedReferencedMessage)
                or dm_message.author.discriminator != "0000"
            ):
                return

            dm_command = cast(commands.Command, self.bot.get_command("dm"))
            ctx = await self.bot.get_context(message)

            real_user = await self.bot.user_from_dm(dm_message.author.name)
            if not real_user:
                return

            await dm_command(ctx, real_user, message=message.content)

    @commands.Cog.listener()
    async def on_guild_join(self, guild: discord.Guild):
        _, prefix, owner = await asyncio.gather(
            self.bot.channels["servers"].send(f"Just joined {guild}! I am now in {len(self.bot.guilds)} different servers!"),
            self.bot.settings.get(guild, ["prefix"]),
            guild.fetch_member(guild.owner_id)
        )

        embed = discord.Embed(
            title=f"Welcome to {self.bot.user.name}!",
            description=WELCOME_MESSAGE.format(guild=guild, prefix=prefix[0])
        ).set_footer(
            text="Support Server: https://discord.gg/zWPWwQC | Bot Invite: https://bit.ly/TTSBot"
        ).set_author(name=str(owner), icon_url=str(owner.avatar_url))

        try: await owner.send(embed=embed)
        except discord.errors.HTTPException: pass

        support_server = self.bot.support_server
        if support_server and owner in support_server.members:
            role = support_server.get_role(738009431052386304)
            if not role:
                return

            support_guild_member = support_server.get_member(owner.id)
            if not support_guild_member:
                return

            await support_guild_member.add_roles(role)
            self.bot.logger.info(f"Added OFS role to {support_guild_member}")

    @commands.Cog.listener()
    async def on_guild_remove(self, guild: discord.Guild):
        await asyncio.gather(
            self.bot.settings.remove(guild),
            self.bot.channels["servers"].send(f"Just got kicked from {guild}. I am now in {len(self.bot.guilds)} servers")
        )


    # IPC events that have been plugged into bot.dispatch
    @commands.Cog.listener()
    async def on_websocket_msg(self, msg):
        self.bot.logger.debug(f"Recieved Websocket message: {msg}")

    @commands.Cog.listener()
    async def on_close(self):
        await self.bot.close(utils.KILL_EVERYTHING)

    @commands.Cog.listener()
    async def on_restart(self):
        await self.bot.close(utils.RESTART_CLUSTER)

    @commands.Cog.listener()
    async def on_reload(self, cog: str):
        self.bot.reload_extension(f"cogs.{cog}")

    @commands.Cog.listener()
    async def on_change_log_level(self, level: str):
        level = level.upper()
        self.bot.logger.setLevel(level)
        for handler in self.bot.logger.handlers:
            handler.setLevel(level)
