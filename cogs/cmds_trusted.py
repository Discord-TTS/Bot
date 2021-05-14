from configparser import ConfigParser
from io import StringIO

import discord
from discord.ext import commands

import utils


config = ConfigParser()
config.read("config.ini")

def setup(bot):
    bot.add_cog(cmds_trusted(bot))

class cmds_trusted(utils.CommonCog, command_attrs={"hidden": True}):
    "TTS Bot commands meant only for trusted users."

    def is_trusted(ctx):
        if str(ctx.author.id) in config["Main"]["trusted_ids"]:
            return True

        raise commands.errors.NotOwner

    @commands.command()
    @commands.check(is_trusted)
    async def block(self, ctx, user: discord.User, notify: bool = False):
        if await self.bot.userinfo.get("blocked", user, default=False):
            return await ctx.send(f"{user} | {user.id} is already blocked!")

        await self.bot.userinfo.block(user)

        await ctx.send(f"Blocked {user} | {user.id}")
        if notify:
            await user.send("You have been blocked from support DMs.")

    @commands.command()
    @commands.check(is_trusted)
    async def unblock(self, ctx, user: discord.User, notify: bool = False):
        if not await self.bot.userinfo.get("blocked", user, default=False):
            return await ctx.send(f"{user} | {user.id} isn't blocked!")

        await self.bot.userinfo.unblock(user)

        await ctx.send(f"Unblocked {user} | {user.id}")
        if notify:
            await user.send("You have been unblocked from support DMs.")

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True)
    async def getinvite(self, ctx, guild: int):
        guild = self.bot.get_guild(guild)

        for channel in guild.channels:
            try:
                invite = str(await channel.create_invite())
            except:
                continue
            if invite:
                return await ctx.send(f"Invite to {guild} | {guild.id}: {invite}")

        await ctx.send("Error: No permissions to make an invite!")

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def dm(self, ctx, todm: discord.User, *, message):
        embed = discord.Embed(title="Message from the developers:", description=message)
        embed.set_author(name=str(ctx.author), icon_url=ctx.author.avatar_url)

        sent = await todm.send(embed=embed)
        await ctx.send(f"Sent message to {todm}:", embed=sent.embeds[0])

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def r(self, ctx, *, message):
        async for history_message in ctx.channel.history(limit=10):
            if history_message.author.discriminator == "0000":
                converter = commands.UserConverter()
                todm = await converter.convert(ctx, history_message.author.name)
                return await self.dm(ctx, todm, message=message)
        await ctx.send("Webhook not found")

    @commands.command()
    @commands.check(is_trusted)
    async def dmhistory(self, ctx, user: discord.User, amount=10):
        messages=[]
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
    async def refreshroles(self, ctx):
        support_server = self.bot.support_server
        if not support_server.chunked:
            await support_server.chunk(cache=True)

        ofs_role = support_server.get_role(738009431052386304)
        highlighted_ofs = support_server.get_role(703307566654160969)

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
                embed.set_author(name=f"{guild_owner} (ID {guild_owner.id})", icon_url=guild_owner.avatar_url)
                embed.set_thumbnail(url=guild_owner.avatar_url)

            if highlighted_ofs not in guild_owner.roles:
                if [True for role in ofs_roles if role in guild_owner.roles]:
                    additional_message = f"Added highlighted owner of a server to {guild_owner.mention}"
                    await guild_owner.add_roles(highlighted_ofs)

            if embed or additional_message:
                await self.bot.channels["logs"].send(additional_message, embed=embed)

        await ctx.send("Done!")

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    async def lookupinfo(self, ctx, mode, *, guild):
        mode = mode.lower()
        guild_object = False

        if mode == "id":
            guild_object = self.bot.get_guild(int(guild))

        elif mode == "name":
            for all_guild in self.bot.guilds:
                if guild in all_guild.name:
                    guild_object = all_guild

        if not guild_object:
            raise commands.BadArgument

        owner = self.bot.get_user(guild_object.owner_id)
        if not owner:
            owner = await self.bot.fetch_user(guild_object.owner_id)

        embed = discord.Embed(title=guild_object.name)
        embed.add_field(name="Guild ID", value=guild_object.id, inline=False)
        embed.add_field(name="Owner Name | ID", value=f"{owner.name} | {owner.id}", inline=False)
        embed.add_field(name="Member Count", value=guild_object.member_count, inline=False)
        embed.set_thumbnail(url=str(guild_object.icon_url))
        await ctx.send(embed=embed)

    @commands.command()
    @commands.check(is_trusted)
    @commands.bot_has_permissions(send_messages=True, attach_files=True)
    async def serverlist(self, ctx):
        servers = [guild.name for guild in self.bot.guilds]

        await ctx.send(
            file=discord.File(
                StringIO(str(servers)),
                filename="servers.txt"
            )
        )
