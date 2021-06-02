from __future__ import annotations

import asyncio
from inspect import cleandoc
from subprocess import call
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
    bot.add_cog(events_other(bot))

class events_other(utils.CommonCog):

    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedGuildMessage):
        if message.content in (self.bot.user.mention, f"<@!{self.bot.user.id}>"):
            await message.channel.send(f"Current Prefix for this server is: `{await self.bot.command_prefix(self.bot, message)}`")

        if message.reference and message.guild == self.bot.support_server and message.channel.name in ("dm_logs", "suggestions") and not message.author.bot:
            dm_message = message.reference.resolved or await message.channel.fetch_message(message.reference.message_id) # type: ignore
            dm_sender = dm_message.author # type: ignore
            if dm_sender.discriminator != "0000":
                return

            dm_command = self.bot.get_command("dm")
            ctx = await self.bot.get_context(message)

            todm = await commands.UserConverter().convert(ctx, dm_sender.name)
            await dm_command(ctx, todm, message=message.content)

        if message.channel.id == 749971061843558440 and message.embeds and str(message.author) == "GitHub#0000":
            embed_title = message.embeds[0].title
            if " new commit" not in embed_title: # type: ignore
                return

            correct_id = self.bot.user.id == 698218518335848538
            correct_title = embed_title.startswith("[Discord-TTS-Bot:dev]") # type: ignore
            if correct_title and correct_id:
                await self.bot.channels['logs'].send("Detected new bot commit! Pulling changes")
                call(['git', 'pull'])
                await self.bot.close()

    @commands.Cog.listener()
    async def on_guild_join(self, guild: discord.Guild):
        _, prefix, owner = await asyncio.gather(
            self.bot.channels["servers"].send(f"Just joined {guild}! I am now in {len(self.bot.guilds)} different servers!"),
            self.bot.settings.get(guild, setting="prefix"),
            guild.fetch_member(guild.owner_id)
        )

        embed = discord.Embed(title=f"Welcome to {self.bot.user.name}!", description=WELCOME_MESSAGE.format(guild=guild, prefix=prefix))
        embed.set_footer(text=f"Support Server: https://discord.gg/zWPWwQC | Bot Invite: https://bit.ly/TTSBot")
        embed.set_author(name=str(owner), icon_url=str(owner.avatar_url))

        try: await owner.send(embed=embed)
        except discord.errors.HTTPException: pass

        support_server = self.bot.support_server
        if owner in support_server.members:
            role = support_server.get_role(738009431052386304)
            if not role:
                return

            await support_server.get_member(owner.id).add_roles(role)

            embed = discord.Embed(description=f"**Role Added:** {role.mention} to {owner.mention}\n**Reason:** Owner of {guild}")
            embed.set_author(name=f"{owner} (ID {owner.id})", icon_url=str(owner.avatar_url))

            await self.bot.channels["logs"].send(embed=embed)

    @commands.Cog.listener()
    async def on_guild_remove(self, guild: discord.Guild):
        await asyncio.gather(
            self.bot.settings.remove(guild),
            self.bot.channels["servers"].send(f"Just got kicked from {guild}. I am now in {len(self.bot.guilds)} servers")
        )
