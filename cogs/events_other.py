from asyncio import sleep
from inspect import cleandoc
from subprocess import call

import discord
from discord.ext import commands


def setup(bot):
    bot.add_cog(events_other(bot))

class events_other(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.Cog.listener()
    async def on_message(self, message):
        if message.content in (self.bot.user.mention, f"<@!{self.bot.user.id}>"):
            await message.channel.send(f"Current Prefix for this server is: `{await self.bot.command_prefix(self.bot, message)}`")

        if message.channel in (self.bot.channels["dm_logs"],self.bot.channels["suggestions"]) and not message.author.bot and message.reference:
            referenced_message = await message.channel.fetch_message(message.reference.message_id)
            reference_author = referenced_message.author
            if reference_author.discriminator == "0000":
                todm= reference_author.name
                converter = commands.UserConverter()
                todm = await converter.convert(message.channel, todm)
                dm = self.bot.get_command("dm")
                ctx = await self.bot.get_context(message)
                await dm(ctx, todm, message=message.content)
        if message.channel.id == 749971061843558440 and message.embeds and str(message.author) == "GitHub#0000":
            if " new commit" in message.embeds[0].title:
                update_for_main = message.embeds[0].title.startswith("[Discord-TTS-Bot:master]") and self.bot.user.id == 513423712582762502
                update_for_dev = message.embeds[0].title.startswith("[Discord-TTS-Bot:dev]") and self.bot.user.id == 698218518335848538

                if update_for_main or update_for_dev:
                    await self.bot.channels['logs'].send("Detected new bot commit! Pulling changes")
                    call(['git', 'pull'])
                    print("===============================================")
                    await self.bot.channels['logs'].send("Restarting bot...")
                    await self.bot.close()

    @commands.Cog.listener()
    async def on_guild_join(self, guild):
        self.bot.queue[guild.id] = dict()

        await self.bot.channels["servers"].send(f"Just joined {guild}! I am now in {len(self.bot.guilds)} different servers!".replace("@", "@ "))

        owner = await guild.fetch_member(guild.owner_id)
        try:
            await owner.send(cleandoc(f"""
            Hello, I am {self.bot.user.name} and I have just joined your server {guild}
            If you want me to start working do `-setup <#text-channel>` and everything will work in there
            If you want to get support for {self.bot.user.name}, join the support server!
            https://discord.gg/zWPWwQC
            """))
        except discord.errors.HTTPException:
            pass

        try:
            if owner.id in [member.id for member in self.bot.supportserver.members if member is not None]:
                role = self.bot.supportserver.get_role(738009431052386304)
                await self.bot.supportserver.get_member(owner.id).add_roles(role)

                embed = discord.Embed(description=f"**Role Added:** {role.mention} to {owner.mention}\n**Reason:** Owner of {guild}")
                embed.set_author(name=f"{owner} (ID {owner.id})", icon_url=owner.avatar_url)

                await self.bot.channels["logs"].send(embed=embed)
        except AttributeError:
            pass

    @commands.Cog.listener()
    async def on_guild_remove(self, guild):
        await self.bot.settings.remove(guild)
        self.bot.should_return[guild.id] = True
        await sleep(0)

        if guild.id in self.bot.queue:
            self.bot.queue.pop(guild.id, None)
        if guild.id in self.bot.should_return:
            self.bot.should_return.pop(guild.id, None)

        await self.bot.channels["servers"].send(f"Just left/got kicked from {guild}. I am now in {len(self.bot.guilds)} servers".replace("@", "@ "))
