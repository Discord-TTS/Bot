from __future__ import annotations

import asyncio
from inspect import cleandoc
from typing import TYPE_CHECKING

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
        if message.guild is None or message.author.bot:
            return

        if message.content in (self.bot.user.mention, f"<@!{self.bot.user.id}>"):
            prefix = await self.bot.command_prefix(self.bot, message)

            cleanup = ('`', '\\`')
            clean_prefix = f"`{prefix.replace(*cleanup)}`"
            permissions = message.channel.permissions_for(message.guild.me) # type: discord.Permissions
            if not permissions.send_messages:
                try:
                    name = discord.utils.escape_markdown(message.guild.name)
                    return await message.author.send(
                        f"My prefix for `{name}` is {clean_prefix} "
                        "however I do not have permission to send messages "
                        "so I cannot respond to your commands!"
                    )
                except discord.Forbidden:
                    return

            msg = f"Current Prefix for this server is: {clean_prefix}"
            await message.channel.send(msg)

        if (
            message.reference
            and message.guild == self.bot.get_support_server()
            and message.channel.name in ("dm_logs", "suggestions")
        ):
            dm_message = message.reference.resolved
            if (
                not dm_message
                or isinstance(dm_message, discord.DeletedReferencedMessage)
                or dm_message.author.discriminator != "0000"
            ):
                return

            dm_command = self.bot.get_command("dm")
            assert dm_command is not None

            ctx = await self.bot.get_context(message)
            real_user = await self.bot.user_from_dm(dm_message.author.name)
            if not real_user:
                return

            await dm_command(ctx, real_user, message=message.content) # type: ignore

    @commands.Cog.listener()
    async def on_guild_join(self, guild: utils.TypedGuild):
        _, owner, settings = await asyncio.gather(
            self.bot.channels["servers"].send(f"Just joined {guild}! I am now in {len(self.bot.guilds)} different servers!"),
            guild.fetch_member(guild.owner_id),
            self.bot.settings.get(guild.id),
        )

        embed = discord.Embed(
            title=f"Welcome to {self.bot.user.name}!",
            description=WELCOME_MESSAGE.format(guild=guild, prefix=settings["prefix"])
        ).set_footer(
            text="Support Server: https://discord.gg/zWPWwQC | Bot Invite: https://bit.ly/TTSBot"
        ).set_author(name=str(owner), icon_url=owner.avatar.url)

        try: await owner.send(embed=embed)
        except discord.errors.HTTPException: pass

        if self.bot.websocket is None or self.bot.get_support_server() is not None:
            return self.bot.dispatch("ofs_add", owner.id)

        json = {"c": "ofs_add", "a": {"owner_id": owner.id}}
        wsjson = utils.data_to_ws_json("SEND", target="support", **json)

        await self.bot.websocket.send(wsjson)

    @commands.Cog.listener()
    async def on_guild_remove(self, guild: discord.Guild):
        del self.bot.settings[guild.id]
        await self.bot.channels["servers"].send(f"Just got kicked from {guild}. I am now in {len(self.bot.guilds)} servers")
