from __future__ import annotations

import asyncio
from inspect import cleandoc
from typing import TYPE_CHECKING, List, cast

import discord
import utils
from discord.ext import commands


if TYPE_CHECKING:
    from main import TTSBotPremium


data_lookup = {
    "guild_count":  lambda b: len(b.guilds),
    "voice_count":  lambda b: len(b.voice_clients),
    "member_count": lambda b: sum(guild.member_count for guild in b.guilds),
    "has_support":  lambda b: None if b.get_support_server() is None else b.cluster_id,
}

WELCOME_MESSAGE = cleandoc("""
    Hello! Someone invited me to your server `{guild}`!
    TTS Bot Premium is a text to speech bot, as in, it reads messages from a text channel and speaks it into a voice channel

    **Most commands need to be done on your server, such as `{prefix}setup` and `{prefix}join`**

    I need someone with the administrator permission to do `{prefix}setup #channel`
    You can then do `{prefix}join` in that channel and I will join your voice channel!
    Then, you can just type normal messages and I will say them, like magic!

    You can view all the commands with `{prefix}help`
    Ask questions by either responding here or asking on the support server!
""")

def setup(bot: TTSBotPremium):
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

            dm_command = cast(commands.Command, self.bot.get_command("dm"))
            ctx = await self.bot.get_context(message)

            real_user = await self.bot.user_from_dm(dm_message.author.name)
            if not real_user:
                return

            await dm_command(ctx, real_user, message=message.content)

    @commands.Cog.listener()
    async def on_guild_join(self, guild: utils.TypedGuild):
        _, prefix, owner = await asyncio.gather(
            self.bot.channels["servers"].send(f"Just joined {guild}! I am now in {len(self.bot.guilds)} different servers!"),
            self.bot.settings.get(guild, ["prefix"]),
            guild.fetch_member(guild.owner_id)
        )

        embed = discord.Embed(
            title=f"Welcome to {self.bot.user.name}!",
            description=WELCOME_MESSAGE.format(guild=guild, prefix=prefix[0])
        ).set_footer(
            text="Support Server: https://discord.gg/zWPWwQC | Bot Invite: Ask Gnome!#6669"
        ).set_author(name=str(owner), icon_url=owner.avatar.url)

        try: await owner.send(embed=embed)
        except discord.errors.HTTPException: pass

        if self.bot.websocket is None or self.bot.get_support_server() is not None:
            return await self.on_ofs_add(owner.id)

        json = {"c": "ofs_add", "a": {"owner_id": owner.id}}
        wsjson = utils.data_to_ws_json("SEND", target="support", **json)

        await self.bot.websocket.send(wsjson)

    @commands.Cog.listener()
    async def on_guild_remove(self, guild: discord.Guild):
        await asyncio.gather(
            self.bot.settings.remove(guild),
            self.bot.channels["servers"].send(f"Just got kicked from {guild}. I am now in {len(self.bot.guilds)} servers")
        )


    # IPC events that have been plugged into bot.dispatch
    @commands.Cog.listener()
    async def on_websocket_msg(self, msg: str):
        self.bot.logger.debug(f"Recieved Websocket message: {msg}")

    @commands.Cog.listener()
    async def on_close(self):
        await self.bot.close(utils.KILL_EVERYTHING)

    @commands.Cog.listener()
    async def on_restart(self):
        await self.bot.close(utils.RESTART_CLUSTER)

    @commands.Cog.listener()
    async def on_reload(self, cog: str):
        self.bot.reload_extension(cog)

    @commands.Cog.listener()
    async def on_change_log_level(self, level: str):
        level = level.upper()
        self.bot.logger.setLevel(level)
        for handler in self.bot.logger.handlers:
            handler.setLevel(level)

    @commands.Cog.listener()
    async def on_request(self, info: List[str], nonce: str, *_):
        data = {to_get: data_lookup[to_get](self.bot) for to_get in info}
        wsjson = utils.data_to_ws_json("RESPONSE", target=nonce, **data)

        await self.bot.websocket.send(wsjson) # type: ignore

    @commands.Cog.listener()
    async def on_ofs_add(self, owner_id: int):
        support_server: discord.Guild = self.bot.get_support_server() # type: ignore
        role = support_server.get_role(703307566654160969)
        if not role:
            return

        try:
            owner_member = await support_server.fetch_member(owner_id)
        except discord.HTTPException:
            return

        await owner_member.add_roles(role)
        self.bot.logger.info(f"Added OFS role to {owner_member}")
