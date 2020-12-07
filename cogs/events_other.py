from inspect import cleandoc
from subprocess import call

import discord
from discord.ext import commands

NoneType = type(None)

def setup(bot):
    bot.add_cog(events_other(bot))

class events_other(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.Cog.listener()
    async def on_message(self, message):
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

        await self.bot.channels["servers"].send(f"Just joined {guild.name}! I am now in {len(self.bot.guilds)} different servers!".replace("@", "@ "))

        owner = await guild.fetch_member(guild.owner_id)
        try:    await owner.send(cleandoc(f"""
            Hello, I am {self.bot.user.name} and I have just joined your server {guild.name}
            If you want me to start working do `-setup <#text-channel>` and everything will work in there
            If you want to get support for {self.bot.user.name}, join the support server!
            https://discord.gg/zWPWwQC
            """))
        except discord.errors.HTTPException:    pass

        try:
            if owner.id in [member.id for member in self.bot.supportserver.members if not isinstance(member, NoneType)]:
                role = self.bot.supportserver.get_role(738009431052386304)
                await self.bot.supportserver.get_member(owner.id).add_roles(role)

                embed = discord.Embed(description=f"**Role Added:** {role.mention} to {owner.mention}\n**Reason:** Owner of {guild.name}")
                embed.set_author(name=f"{owner} (ID {owner.id})", icon_url=owner.avatar_url)

                await self.bot.channels["logs"].send(embed=embed)
        except AttributeError:  pass

    @commands.Cog.listener()
    async def on_guild_remove(self, guild):
        await self.bot.settings.remove(guild)
        self.bot.playing[guild.id] = 2

        if guild.id in self.bot.queue:  self.bot.queue.pop(guild.id, None)
        if guild.id in self.bot.playing:  self.bot.playing.pop(guild.id, None)
        await self.bot.channels["servers"].send(f"Just left/got kicked from {guild.name}. I am now in {len(self.bot.guilds)} servers".replace("@", "@ "))
