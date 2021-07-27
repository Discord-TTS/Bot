from __future__ import annotations

from json import dump
from typing import TYPE_CHECKING, Union

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBotPremium


def setup(bot: TTSBotPremium):
    bot.add_cog(cmds_owner(bot))

class cmds_owner(utils.CommonCog, command_attrs={"hidden": True}):
    "TTS Bot commands meant only for the bot owner."

    @commands.command()
    @commands.is_owner()
    @commands.bot_has_permissions(send_messages=True, manage_messages=True, manage_webhooks=True)
    async def sudo(self, ctx: commands.Context, user: Union[discord.User, str], *, message):
        """mimics another user"""
        await ctx.message.delete()

        if isinstance(user, str):
            avatar = "https://cdn.discordapp.com/avatars/689564772512825363/f05524fd9e011108fd227b85c53e3d87.png"
        else:
            avatar = str(user.avatar_url)
            user = user.display_name

        if not isinstance(ctx.channel, discord.TextChannel):
            return

        webhooks = await ctx.channel.webhooks()
        if len(webhooks) == 0:
            webhook = await ctx.channel.create_webhook(name="Temp Webhook For -sudo")
            await webhook.send(message, username=user, avatar_url=avatar)
            await webhook.delete()
        else:
            webhook = webhooks[0]
            await webhook.send(message, username=user, avatar_url=avatar)

    @commands.command()
    @commands.is_owner()
    async def trust(self, ctx: commands.Context, mode: str, user: Union[discord.User, str] = ""):
        if mode == "list":
            await ctx.send("\n".join(self.bot.trusted))

        elif isinstance(user, str):
            return

        elif mode == "add":
            self.bot.trusted.append(str(user.id))
            self.bot.config["Main"]["trusted_ids"] = str(self.bot.trusted)
            with open("self.bot.config.ini", "w") as configfile:
                self.bot.config.write(configfile)

            await ctx.send(f"Added {user} | {user.id} to the trusted members")

        elif mode == "del" and str(user.id) in self.bot.trusted:
            self.bot.trusted.remove(str(user.id))
            self.bot.config["Main"]["trusted_ids"] = str(self.bot.trusted)
            with open("self.bot.config.ini", "w") as configfile:
                self.bot.config.write(configfile)

            await ctx.send(f"Removed {user} | {user.id} from the trusted members")

    @commands.command()
    @commands.is_owner()
    @commands.bot_has_permissions(send_messages=True)
    async def say(self, ctx: commands.Context, channel: discord.TextChannel, *, to_say: str):
        if ctx.channel.permissions_for(ctx.guild.me).manage_messages: # type: ignore
            await ctx.message.delete()

        await channel.send(to_say)

    @commands.command(aliases=("rc", "reload"))
    @commands.is_owner()
    async def reload_cog(self, ctx: commands.Context, *, to_reload: str):
        try:
            self.bot.reload_extension(f"cogs.{to_reload}")
        except Exception as e:
            await ctx.send(f"**`ERROR:`** {type(e).__name__} - {e}")
        else:
            await ctx.send("**`SUCCESS`**")

    @commands.command()
    @commands.is_owner()
    async def add_premium(self, ctx: commands.Context, guild_id: int, user: discord.User):
        await self.bot.wait_until_ready()

        guild = self.bot.get_guild(guild_id)
        if user.id in tuple(self.bot.donators.values()):
            return await ctx.send(f"{user} is already linked to a guild, check json for more details.")
        if not guild:
            return await ctx.send("I'm not in that guild!")

        self.bot.donators[guild_id] = user.id

        async with self.bot.pool.acquire() as conn:
            query = "INSERT INTO donators(guild_id, user_id) VALUES ($1, $2);"

            await conn.execute(query, guild_id, user.id)

        await ctx.send(f"Linked {user.mention} ({user} | {user.id}) to {guild.name}")
