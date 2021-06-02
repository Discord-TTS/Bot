from __future__ import annotations

from typing import List, TYPE_CHECKING

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def is_trusted(ctx: utils.TypedContext):
    if str(ctx.author.id) in ctx.bot.config["Main"]["trusted_ids"]:
        return True

    raise commands.errors.NotOwner


def setup(bot: TTSBot):
    bot.add_cog(cmds_trusted(bot))

class cmds_trusted(utils.CommonCog, command_attrs={"hidden": True}):
    "TTS Bot commands meant only for trusted users."

    @commands.command()
    @commands.check(is_trusted)
    async def block(self, ctx: commands.Context, user: discord.User, notify: bool = False):
        if await self.bot.userinfo.get("blocked", user, default=False):
            return await ctx.send(f"{user} | {user.id} is already blocked!")

        await self.bot.userinfo.block(user)

        await ctx.send(f"Blocked {user} | {user.id}")
        if notify:
            await user.send("You have been blocked from support DMs.")

    @commands.command()
    @commands.check(is_trusted)
    async def unblock(self, ctx: commands.Context, user: discord.User, notify: bool = False):
        if not await self.bot.userinfo.get("blocked", user, default=False):
            return await ctx.send(f"{user} | {user.id} isn't blocked!")

        await self.bot.userinfo.unblock(user)

        await ctx.send(f"Unblocked {user} | {user.id}")
        if notify:
            await user.send("You have been unblocked from support DMs.")

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True)
    async def getinvite(self, ctx: commands.Context, guild: int):
        guild_obj = self.bot.get_guild(guild)

        for channel in guild_obj.channels:
            try:
                invite = str(await channel.create_invite())
            except:
                continue
            if invite:
                return await ctx.send(f"Invite to {guild_obj} | {guild}: {invite}")

        await ctx.send("Error: No permissions to make an invite!")

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def dm(self, ctx: utils.TypedContext, todm: discord.User, *, message: str):
        embed = discord.Embed(title="Message from the developers:", description=message)
        embed.set_author(name=str(ctx.author), icon_url=str(ctx.author.avatar_url))

        sent = await todm.send(embed=embed)
        await ctx.send(f"Sent message to {todm}:", embed=sent.embeds[0])

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def r(self, ctx: commands.Context, *, message: str):
        async for history_message in ctx.channel.history(limit=10):
            if history_message.author.discriminator == "0000":
                converter = commands.UserConverter()
                todm = await converter.convert(ctx, history_message.author.name)
                return await self.dm(ctx, todm, message=message)
        await ctx.send("Webhook not found")

    @commands.command()
    @commands.check(is_trusted)
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

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True)
    async def refreshroles(self, ctx: commands.Context):
        support_server = self.bot.support_server
        if support_server is None:
            return

        if not support_server.chunked:
            await support_server.chunk(cache=True)

        ofs_role = support_server.get_role(738009431052386304)
        highlighted_ofs = support_server.get_role(703307566654160969)
        if not (ofs_role and highlighted_ofs):
            return


        people_with_owner_of_server = [member.id for member in ofs_role.members]
        people_with_highlighted_ofs = [member.id for member in highlighted_ofs.members]
        support_server_members = [member.id for member in support_server.members]

        owner_list = [guild.owner_id for guild in self.bot.guilds]
        ofs_roles = list()
        for role in (738009431052386304, 738009620601241651, 738009624443224195):
            ofs_roles.append(support_server.get_role(role))

        for ofs_person in people_with_owner_of_server:
            if ofs_person not in owner_list:
                roles = [ofs_role, ]
                embed = discord.Embed(description=f"Role Removed: Removed {ofs_role.mention} from <@{ofs_person}>")
                if ofs_person in people_with_highlighted_ofs:
                    roles.append(highlighted_ofs)

                await support_server.get_member(ofs_person).remove_roles(*roles)
                await self.bot.channels["logs"].send(embed=embed)

        for guild_owner in owner_list:
            if guild_owner not in support_server_members:
                continue

            guild_owner = support_server.get_member(guild_owner)
            additional_message = None
            embed = None

            if guild_owner.id not in people_with_owner_of_server:
                await guild_owner.add_roles(ofs_role)

                embed = discord.Embed(description=f"Role Added: Gave {ofs_role.mention} to {guild_owner.mention}")
                embed.set_author(name=f"{guild_owner} (ID {guild_owner.id})", icon_url=str(guild_owner.avatar_url))
                embed.set_thumbnail(url=str(guild_owner.avatar_url))

            if highlighted_ofs not in guild_owner.roles:
                if [True for role in ofs_roles if role in guild_owner.roles]:
                    additional_message = f"Added highlighted owner of a server to {guild_owner.mention}"
                    await guild_owner.add_roles(highlighted_ofs)

            if not (embed and additional_message):
                continue

            await self.bot.channels["logs"].send(additional_message, embed=embed)

        await ctx.send("Done!")
