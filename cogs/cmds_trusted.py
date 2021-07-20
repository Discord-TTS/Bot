from __future__ import annotations

from typing import TYPE_CHECKING, List

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(TrustedCommands(bot))

class TrustedCommands(utils.CommonCog, command_attrs={"hidden": True}):
    "TTS Bot commands meant only for trusted users."
    def cog_check(self, ctx: utils.TypedContext):
        if str(ctx.author.id) in self.bot.config["Main"]["trusted_ids"]:
            ctx.bot.logger.warning(f"{ctx.author} passed Trusted Check and ran {ctx.command.qualified_name}")
            return True

        raise commands.errors.NotOwner

    @commands.command()
    async def block(self, ctx: commands.Context, user: discord.User, notify: bool = False):
        if await self.bot.userinfo.get("blocked", user, default=False):
            return await ctx.send(f"{user} | {user.id} is already blocked!")

        await self.bot.userinfo.block(user)

        await ctx.send(f"Blocked {user} | {user.id}")
        if notify:
            await user.send("You have been blocked from support DMs.")

    @commands.command()
    async def unblock(self, ctx: commands.Context, user: discord.User, notify: bool = False):
        if not await self.bot.userinfo.get("blocked", user, default=False):
            return await ctx.send(f"{user} | {user.id} isn't blocked!")

        await self.bot.userinfo.unblock(user)

        await ctx.send(f"Unblocked {user} | {user.id}")
        if notify:
            await user.send("You have been unblocked from support DMs.")

    @commands.command()
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def dm(self, ctx: utils.TypedContext, todm: discord.User, *, message: str):
        embed = discord.Embed(title="Message from the developers:", description=message)
        embed.set_author(name=str(ctx.author), icon_url=str(ctx.author.avatar_url))

        sent = await todm.send(embed=embed)
        await ctx.send(f"Sent message to {todm}:", embed=sent.embeds[0])

    @commands.command()
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def r(self, ctx: utils.TypedContext, *, message: str):
        async for history_message in ctx.channel.history(limit=10):
            if history_message.author.discriminator != "0000":
                continue

            user = await self.bot.user_from_dm(history_message.author.name)
            if not user:
                continue

            await self.dm(ctx, user, message=message)
        await ctx.send("Webhook not found")

    @commands.command()
    async def dmhistory(self, ctx: commands.Context, user: discord.User, amount: int = 10):
        messages: List[str] = []
        async for message in user.history(limit=amount):
            if message.embeds:
                if message.embeds[0].author:
                    messages.append(f"`{message.embeds[0].author.name} ⚙️`: {message.embeds[0].description}")
                else:
                    messages.append(f"`{message.author} ⚙️`: {message.embeds[0].description}")

            else:
                messages.append(f"`{message.author}`: {message.content}")

        messages.reverse()
        embed = discord.Embed(
            title=f"Message history of {user.name}",
            description="\n".join(messages)
        )

        await ctx.send(embed=embed)
