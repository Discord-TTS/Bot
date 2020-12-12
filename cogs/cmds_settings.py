import asyncio
from inspect import cleandoc
from random import choice as pick_random
from typing import Optional
import re

import discord
from discord.ext import commands
from gtts.lang import tts_langs

from utils import basic

NoneType = type(None)
tts_langs = tts_langs()
to_enabled = {True: "Enabled", False: "Disabled"}

def setup(bot):
    bot.add_cog(Settings(bot))

class Settings(commands.Cog):
    def __init__(self, bot):
        self.bot = bot

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def settings(self, ctx, *, help = None):
        if not isinstance(help, NoneType): help = help.lower()

        if help == "help":
            message = cleandoc("""
              -set channel `#channel`: Sets the text channel to read from
              -set xsaid `true/false`: Enable/disable "person said" before every message
              -set autojoin `true/false`: Auto joins a voice channel when a text is sent
              -set ignorebots `true/false`: Do not read other bot messages
              -set nickname `@person` `new name`: Sets your (or someone else if admin) name for xsaid.
              -set limits: Do `-settings limits help` to check!

              -set voice `language-code`: Changes your voice to a `-voices` code, equivalent to `-voice`""")
            embed = discord.Embed(title="Settings > Help", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="Available properties:", value=message, inline=False)
        elif help == "limits":
            return await self.limits(ctx)
        elif help == "limits help":
            return await self.help(ctx)
        else:
            lang, nickname = await asyncio.gather(
                self.bot.setlangs.get(ctx.author),
                self.bot.nicknames.get(ctx.guild, ctx.author)
            )
            say, channel, join, bot_ignore = await self.bot.settings.get(
                ctx.guild,
                settings=(
                    "xsaid",
                    "channel",
                    "auto_join",
                    "bot_ignore"
                    )
                )

            channel = ctx.guild.get_channel(int(channel))

            if channel is None:
                channel = "has not been setup yet"
            else:
                channel = channel.name

            if nickname == ctx.author.display_name:
                nickname = "has not been set yet"

            # Show settings embed
            message1 = cleandoc(f"""
              :small_orange_diamond: Channel: `#{channel}`
              :small_orange_diamond: XSaid: `{say}`
              :small_orange_diamond: Auto Join: `{join}`
              :small_orange_diamond: Ignore Bots: `{bot_ignore}`
              :star: Limits: Do `-settings limits` to check!""")

            message2 = cleandoc(f"""
              :small_blue_diamond: Language: `{lang}`
              :small_blue_diamond: Nickname: `{nickname}`""")

            embed = discord.Embed(title="Current Settings", url="https://discord.gg/zWPWwQC", color=0x3498db)
            embed.add_field(name="**Server Wide**", value=message1, inline=False)
            embed.add_field(name="**User Specific**", value=message2, inline=False)

        embed.set_footer(text="Change these settings with -set property value!")
        await ctx.send(embed=embed)

    @commands.guild_only()
    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.group()
    async def set(self, ctx):
        if ctx.invoked_subcommand is None:
            await ctx.send("Error: Invalid property, do `-settings help` to get a list!")

    @set.command()
    @commands.has_permissions(administrator=True)
    async def xsaid(self, ctx, value: bool):
        await self.bot.settings.set(ctx.guild, "xsaid", value)
        await ctx.send(f"xsaid is now: {to_enabled[value]}")

    @set.command(aliases=["auto_join"])
    @commands.has_permissions(administrator=True)
    async def autojoin(self, ctx, value: bool):
        await self.bot.settings.set(ctx.guild, "auto_join", value)
        await ctx.send(f"Auto Join is now: {to_enabled[value]}")

    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    @commands.has_permissions(administrator=True)
    async def botignore(self, ctx, value: bool):
        await self.bot.settings.set(ctx.guild, "bot_ignore", value)
        await ctx.send(f"Ignoring Bots is now: {to_enabled[value]}")

    @set.command(aliases=["nick_name", "nickname", "name"])
    @commands.bot_has_permissions(embed_links=True)
    async def nick(self, ctx, user: Optional[discord.Member] = False, *, nickname):
        if user:
            if nickname:
                if not ctx.channel.permissions_for(ctx.author).administrator:
                    return await ctx.send("Error: You need admin to set other people's nicknames!")
            else:
                nickname = ctx.author.display_name
        else:
            user = ctx.author

        if not nickname:
            raise commands.UserInputError(ctx.message)

        if "<" in nickname and ">" in nickname:
            await ctx.send("Hey! You can't have mentions/emotes in your nickname!")
        elif not re.match(r'^(\w|\s)+$', nickname):
            await ctx.send("Hey! Please keep your nickname to only letters, numbers, and spaces!")
        else:
            await self.bot.nicknames.set(ctx.guild, user, nickname)
            await ctx.send(embed=discord.Embed(title="Nickname Change", description=f"Changed {user.name}'s nickname to {nickname}"))

    @set.command()
    @commands.has_permissions(administrator=True)
    async def channel(self, ctx, channel: discord.TextChannel):
        await self.setup(ctx, channel)

    @set.command(aliases=("voice", "lang"))
    async def language(self, ctx, voicecode):
        await self.voice(ctx, voicecode)

    @set.group()
    @commands.has_permissions(administrator=True)
    async def limits(self, ctx):
        additional_message = None
        if ctx.invoked_subcommand is not None: return
        if ctx.message.content != f"{self.bot.command_prefix}set limits":
            additional_message = "Error: Invalid property!"

        msg_length, repeated_chars = await self.bot.settings.get(
            ctx.guild,
            settings=(
                "msg_length",
                "repeated_chars"
            )
        )

        message1 = cleandoc(f"""
            :small_orange_diamond: Max Message Length: `{msg_length} seconds`
            :small_orange_diamond: Max Repeated Characters: `{repeated_chars}`
            """)

        embed = discord.Embed(title="Current Limits", description=message1, url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_footer(text="Change these settings with -set limits property value!")
        await ctx.send(additional_message, embed=embed)

    @limits.command()
    async def help(self, ctx):
        message = cleandoc("""
            -set limits max_length `seconds`: Max seconds for a TTS'd message
            -set limits chars `number`: Max repetion of a character (0 = off)
            """)

        embed = discord.Embed(title="Settings > Limits > Help", url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.add_field(name="Available properties:", value=message, inline=False)
        embed.set_footer(text="Change these settings with -set limits property value!")
        await ctx.send(embed=embed)

    @limits.command(aliases=("length", "max_length", "max_msg_length", "msglength", "maxlength"))
    async def msg_length(self, ctx, length: int):
        if length > 60: return await ctx.send("Hey! You can't set max message length above 60 seconds!")
        if length < 20: return await ctx.send("Hey! You can't set max message length below 20 seconds!")

        await self.bot.settings.set(ctx.guild, "msg_length", str(length))
        await ctx.send(f"Max message length (in seconds) is now: {length}")

    @limits.command(aliases=("repeated_characters", "repeated_letters", "chars"))
    async def repeated_chars(self, ctx, chars: int):
        if chars > 100: return await ctx.send("Hey! You can't set max repeated chars above 100!")
        if chars < 5: return await ctx.send("Hey! You can't set max repeated chars below 5!")

        await self.bot.settings.set(ctx.guild, "repeated_chars", str(chars))
        await ctx.send(f"Max repeated characters is now: {chars}")

    @commands.guild_only()
    @commands.has_permissions(administrator=True)
    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def setup(self, ctx, channel: discord.TextChannel):
        await self.bot.settings.set(ctx.guild, "channel", str(channel.id))

        embed = discord.Embed(
            title="TTS Bot has been setup!",
            description=cleandoc(f"""
                TTS Bot will now accept commands and read from {channel.mention}.
                Just do `-join` and start talking!
                """)
            )
        embed.set_footer(text=pick_random(basic.footer_messages))
        embed.set_thumbnail(url=str(self.bot.user.avatar_url))
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))

        await ctx.send(embed=embed)

    @commands.bot_has_permissions(read_messages=True, send_messages=True)
    @commands.command()
    async def voice(self, ctx, lang: str):
        if lang in tts_langs:
            await self.bot.setlangs.set(ctx.author, lang)
            await ctx.send(f"Changed your voice to: {tts_langs[lang]}")
        else:
            await ctx.send("Invalid voice, do `-voices`")

    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    async def voices(self, ctx, lang=None):
        if lang in tts_langs:
            return await self.voice(ctx, lang)

        lang = await self.bot.setlangs.get(ctx.author)
        langs_string = basic.remove_chars(list(tts_langs.keys()), "[", "]")

        embed = discord.Embed(title="TTS Bot Languages")
        embed.set_footer(text=pick_random(basic.footer_messages))
        embed.add_field(name="Currently Supported Languages", value=langs_string)
        embed.add_field(name="Current Language used", value=f"{tts_langs[lang]} | {lang}")
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))

        await ctx.send(embed=embed)
