from __future__ import annotations

from typing import TYPE_CHECKING, Union

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


def setup(bot: TTSBot):
    bot.add_cog(OwnerCommands(bot))

class OwnerCommands(utils.CommonCog, command_attrs={"hidden": True}):
    "TTS Bot commands meant only for the bot owner."

    @commands.command()
    @commands.is_owner()
    @commands.bot_has_permissions(send_messages=True, manage_messages=True, manage_webhooks=True)
    async def sudo(self, ctx: utils.TypedContext, user: Union[discord.User, str], *, message: str):
        """mimics another user"""
        await ctx.message.delete()

        if isinstance(user, str):
            avatar = "https://cdn.discordapp.com/.png"
        else:
            avatar = user.avatar.url
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
    @commands.guild_only()
    @commands.bot_has_permissions(send_messages=True)
    async def say(self, ctx: utils.TypedGuildContext, channel: discord.TextChannel, *, to_say: str):
        if ctx.bot_permissions().manage_messages:
            await ctx.message.delete()

        await channel.send(to_say)

    @commands.command(aliases=("rc", "reload"))
    @commands.is_owner()
    async def reload_cog(self, ctx: commands.Context, *, to_reload: str):
        try:
            self.bot.reload_extension(to_reload)
        except Exception as e:
            await ctx.send(f"**`ERROR:`** {type(e).__name__} - {e}")
        else:
            await ctx.send("**`SUCCESS`**")
            if self.bot.websocket is None:
                return

            await self.bot.websocket.send(
                utils.data_to_ws_json(
                    "SEND", target="*", **{
                        "c": "reload",
                        "a": {"cog": to_reload},
                })
            )
