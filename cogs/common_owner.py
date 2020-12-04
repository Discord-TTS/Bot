from configparser import ConfigParser
from typing import Union

import discord
from discord.ext import commands


def setup(bot):
    bot.add_cog(Gnome(bot))

class Gnome(commands.Cog):
    def __init__(self, bot):
        self.bot = bot
    #////////////////////////////////////////////////////////

    @commands.command()
    @commands.is_owner()
    @commands.bot_has_permissions(read_messages=True, send_messages=True, manage_messages=True, manage_webhooks=True)
    async def sudo(self, ctx, user: Union[discord.Member, discord.User, int, str], *, message):
        """mimics another user"""
        await ctx.message.delete()

        if isinstance(user, int):
            try:    user = await self.bot.fetch_user(user)
            except: user = str(user)

        if isinstance(user, str):
            name = user
            avatar = "https://cdn.discordapp.com/avatars/689564772512825363/f05524fd9e011108fd227b85c53e3d87.png"
        else:
            name = user.display_name
            avatar = user.avatar_url

        webhooks = await ctx.channel.webhooks()
        if len(webhooks) == 0:
            webhook = await ctx.channel.create_webhook(name="Temp Webhook For -sudo")
            await webhook.send(message, username=name, avatar_url=avatar)
            await webhook.delete()
        else:
            webhook = webhooks[0]
            await webhook.send(message, username=name, avatar_url=avatar)

    @commands.command()
    @commands.is_owner()
    async def trust(self, ctx, mode, user: Union[discord.User, str] = ""):
        if mode == "list":
            await ctx.send("\n".join(self.bot.trusted))

        elif isinstance(user, str):
            return

        elif mode == "add":
            self.bot.trusted.append(str(user.id))
            config["Main"]["trusted_ids"] = str(self.bot.trusted)
            with open("config.ini", "w") as configfile:
                config.write(configfile)

            await ctx.send(f"Added {str(user)} | {user.id} to the trusted members")

        elif mode == "del":
            if str(user.id) in self.bot.trusted:
                self.bot.trusted.remove(str(user.id))
                config["Main"]["trusted_ids"] = str(self.bot.trusted)
                with open("config.ini", "w") as configfile:
                    config.write(configfile)

                await ctx.send(f"Removed {str(user)} | {user.id} from the trusted members")

    @commands.command()
    @commands.is_owner()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    async def say(self, ctx, channel: discord.TextChannel, *, tosay):
        try:    await ctx.message.delete()
        except: pass

        await channel.send(tosay)

    @commands.command()
    @commands.is_owner()
    async def reload_cog(self, ctx, *, toreload: str):
        try:
            self.bot.reload_extension(toreload)
        except Exception as e:
            await ctx.send(f'**`ERROR:`** {type(e).__name__} - {e}')
        else:
            await ctx.send('**`SUCCESS`**')

    @commands.command()
    @commands.is_owner()
    async def leaveguild(self, ctx, guild : int):
        await self.bot.get_guild(guild).leave()
