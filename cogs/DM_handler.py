from __future__ import annotations

import random
from inspect import cleandoc
from typing import TYPE_CHECKING, Dict

import discord
from discord.ext import commands

import utils


if TYPE_CHECKING:
    from main import TTSBot


DM_WELCOME_MESSAGE = cleandoc("""
    **All messages after this will be sent to a private channel where we can assist you.**
    Please keep in mind that we aren't always online and get a lot of messages, so if you don't get a response within a day repeat your message.
    There are some basic rules if you want to get help though:
    `1.` Ask your question, don't just ask for help
    `2.` Don't spam, troll, or send random stuff (including server invites)
    `3.` Many questions are answered in `-help`, try that first (also the default prefix is `-`)
""")


def setup(bot: TTSBot):
    if bot.cluster_id == 0:
        bot.add_cog(DMHandler(bot))


class DMHandler(utils.CommonCog):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.is_welcomed: Dict[int, bool] = {}


    def is_welcome_message(self, message: discord.Message) -> bool:
        if not message.embeds:
            return False

        return message.embeds[0].title == f"Welcome to {self.bot.user.name} Support DMs!"

    @commands.Cog.listener()
    async def on_message(self, message: utils.TypedMessage):
        if message.guild or message.author.bot or message.content.startswith("-"):
            return

        pins = None
        self.bot.log("on_dm")
        if message.author.id not in self.is_welcomed:
            pins = await message.author.pins()
            self.is_welcomed[message.author.id] = any(map(self.is_welcome_message, pins))

        if self.is_welcomed[message.author.id]:
            if "https://discord.gg/" in message.content.lower():
                await message.author.send(f"Join https://discord.gg/zWPWwQC and look in <#694127922801410119> to invite {self.bot.user.mention}!")

            elif message.content.lower() == "help":
                self.bot.logger.info(f"{message.author} just got the 'dont ask to ask' message")
                await message.channel.send("We cannot help you unless you ask a question, if you want the help command just do `-help`!")

            elif not (await self.bot.userinfo.get(message.author.id)).get("blocked", False):
                files = [await attachment.to_file() for attachment in message.attachments]

                author_name = str(message.author)
                author_id = f" ({message.author.id})"
                await self.bot.channels["dm_logs"].send(
                    files=files,
                    content=message.content,
                    avatar_url=message.author.avatar.url,
                    allowed_mentions=discord.AllowedMentions.none(),
                    username=author_name[:32 - len(author_id)] + author_id,
                )

        else:
            if pins is None:
                pins = await message.author.pins()

            if len(pins) >= 49:
                return await message.channel.send("Error: Pinned messages are full, cannot pin the Welcome to Support DMs message!")

            embed = discord.Embed(
                title=f"Welcome to {self.bot.user.name} Support DMs!",
                description=DM_WELCOME_MESSAGE
            ).set_footer(text=random.choice(utils.FOOTER_MSGS))

            dm_message = await message.author.send("Please do not unpin this notice, if it is unpinned you will get the welcome message again!", embed=embed)
            self.bot.logger.info(f"{message.author} just got the 'Welcome to Support DMs' message")

            self.is_welcomed[message.author.id] = True
            await dm_message.pin()

    @commands.Cog.listener()
    async def on_private_channel_pins_update(self, channel: discord.DMChannel, _):
        assert channel.recipient is not None

        welcomed = any(map(self.is_welcome_message, await channel.pins()))
        self.is_welcomed[channel.recipient.id] = welcomed
