from __future__ import annotations

import asyncio
import re
from inspect import cleandoc
from random import choice as pick_random
from typing import Set, TYPE_CHECKING, Dict, Optional

import discord
from discord.ext import commands
from gtts.lang import tts_langs as _tts_langs

import utils


if TYPE_CHECKING:
    from main import TTSBot


tts_langs: Set[str] = set(_tts_langs().keys())
langs_lookup: Dict[str, str] = {
    lang: name
    for lang, name in _tts_langs().items()
    if "-" not in lang
}

to_enabled = {True: "Enabled", False: "Disabled"}

def setup(bot: TTSBot):
    bot.add_cog(SettingCommands(bot))

class SettingCommands(utils.CommonCog, name="Settings"):
    "TTS Bot settings commands, configuration is done here."

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.guild_only()
    @commands.command()
    async def settings(self, ctx: utils.TypedGuildContext, *, help: Optional[str] = None):
        "Displays the current settings!"

        if help:
            return await ctx.send_help(f"set{' limit' if help.lower() == 'limits' else ''}")

        lang, nickname = await asyncio.gather(
            self.bot.userinfo.get("lang", ctx.author, default="en"),
            self.bot.nicknames.get(ctx.guild, ctx.author)
        )

        xsaid, prefix, channel, auto_join, bot_ignore, default_lang = await self.bot.settings.get(
            ctx.guild,
            settings=[
                "xsaid",
                "prefix",
                "channel",
                "auto_join",
                "bot_ignore",
                "default_lang"
            ]
        )

        channel = ctx.guild.get_channel(channel)
        channel_name = channel.name if channel else "has not been setup yet"

        if nickname == ctx.author.display_name:
            nickname = "has not been set yet"

        # Show settings embed
        sep1, sep2, sep3 = utils.OPTION_SEPERATORS
        server_settings = cleandoc(f"""
            {sep1} Setup Channel: `#{channel_name}`
            {sep1} Auto Join: `{auto_join}`
            {sep1} Command Prefix: `{prefix}`
        """)

        tts_settings = cleandoc(f"""
            {sep2} <User> said: message `{xsaid}`
            {sep2} Ignore bot's messages: `{bot_ignore}`
            {sep2} Default Server Language: `{default_lang}`
        """)

        user_settings = cleandoc(f"""
            {sep3} Language: `{lang}`
            {sep3} Nickname: `{nickname}`
        """)

        embed = discord.Embed(title="Current Settings", url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.add_field(name="**General Server Settings**", value=server_settings, inline=False)
        embed.add_field(name="**TTS Settings**",            value=tts_settings, inline=False)
        embed.add_field(name="**User Specific**",           value=user_settings, inline=False)

        embed.set_footer(text=f"Change these settings with {ctx.prefix}set property value!")
        await ctx.send(embed=embed)


    @commands.bot_has_permissions(send_messages=True)
    @commands.guild_only()
    @commands.group()
    async def set(self, ctx: commands.Context):
        "Changes a setting!"
        if ctx.invoked_subcommand is None:
            await ctx.send_help(ctx.command)

    @set.command()
    @commands.has_permissions(administrator=True)
    async def xsaid(self, ctx: utils.TypedGuildContext, value: bool):
        "Makes the bot say \"<user> said\" before each message"
        await self.bot.settings.set(ctx.guild, "xsaid", value)
        await ctx.send(f"xsaid is now: {to_enabled[value]}")

    @set.command(aliases=["auto_join"])
    @commands.has_permissions(administrator=True)
    async def autojoin(self, ctx: utils.TypedGuildContext, value: bool):
        "If you type a message in the setup channel, the bot will join your vc"
        await self.bot.settings.set(ctx.guild, "auto_join", value)
        await ctx.send(f"Auto Join is now: {to_enabled[value]}")

    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    @commands.has_permissions(administrator=True)
    async def botignore(self, ctx: utils.TypedGuildContext, value: bool):
        "Messages sent by bots and webhooks are not read"
        await self.bot.settings.set(ctx.guild, "bot_ignore", value)
        await ctx.send(f"Ignoring Bots is now: {to_enabled[value]}")


    @set.command(aliases=["defaultlang", "default_lang", "defaultlanguage", "slang", "serverlanguage"])
    @commands.has_permissions(administrator=True)
    async def server_language(self, ctx: utils.TypedGuildContext, language: str):
        "Changes the default language messages are read in"
        if language not in tts_langs:
            return await ctx.send(f"Invalid voice, do `{ctx.prefix}voices`")

        await self.bot.settings.set(ctx.guild, "default_lang", language)
        await ctx.send(f"Default language for this server is now: {language}")

    @set.command()
    @commands.has_permissions(administrator=True)
    async def prefix(self, ctx: utils.TypedGuildContext, *, prefix: str):
        """The prefix used before commands"""
        if len(prefix) > 5 or prefix.count(" ") > 1:
            return await ctx.send("**Error**: Invalid Prefix! Please use 5 or less characters with maximum 1 space.")

        await self.bot.settings.set(ctx.guild, "prefix", prefix)
        await ctx.send(f"Command Prefix is now: {prefix}")

    @set.command(aliases=["nick_name", "nickname", "name"])
    @commands.bot_has_permissions(embed_links=True)
    async def nick(self, ctx: utils.TypedGuildContext, optional_user: Optional[discord.Member] = None, *, nickname: str):
        "Replaces your username in \"<user> said\" with a given name"
        user = optional_user or ctx.author
        nickname = nickname or ctx.author.display_name

        if user != ctx.author and not ctx.author_permissions().administrator:
            return await ctx.send("Error: You need admin to set other people's nicknames!")

        if not nickname:
            raise commands.UserInputError(ctx.message.content)

        if "<" in nickname and ">" in nickname:
            await ctx.send("Hey! You can't have mentions/emotes in your nickname!")
        elif not re.match(r"^(\w|\s)+$", nickname):
            await ctx.send("Hey! Please keep your nickname to only letters, numbers, and spaces!")
        else:
            await self.bot.nicknames.set(ctx.guild, user, nickname)
            await ctx.send(embed=discord.Embed(title="Nickname Change", description=f"Changed {user.name}'s nickname to {nickname}"))


    @set.command()
    @commands.has_permissions(administrator=True)
    async def channel(self, ctx: utils.TypedGuildContext, channel: discord.TextChannel):
        "Alias of `-setup`"
        await self.setup(ctx, channel)

    @set.command(aliases=("voice", "lang", "_language"))
    async def language(self, ctx: utils.TypedGuildContext, voicecode: str):
        "Changes the language your messages are read in, full list in `-voices`"
        await self.voice(ctx, voicecode)


    @set.group()
    @commands.has_permissions(administrator=True)
    async def limits(self, ctx: utils.TypedGuildContext):
        "A group of settings to modify the limits of what the bot reads"
        if ctx.invoked_subcommand is not None:
            return

        if ctx.message.content != f"{ctx.prefix}set limits":
            return await ctx.send_help(ctx.command)

        msg_length, repeated_chars = await self.bot.settings.get(
            ctx.guild,
            settings=[
                "msg_length",
                "repeated_chars"
            ]
        )

        sep = utils.OPTION_SEPERATORS[0]
        message1 = cleandoc(f"""
            {sep} Max Message Length: `{msg_length} seconds`
            {sep} Max Repeated Characters: `{repeated_chars}`
        """)

        embed = discord.Embed(title="Current Limits", description=message1, url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.set_footer(text=f"Change these settings with {ctx.prefix}set limits property value!")
        await ctx.send(embed=embed)

    @limits.command(aliases=("length", "max_length", "max_msg_length", "msglength", "maxlength"))
    async def msg_length(self, ctx: utils.TypedGuildContext, length: int):
        "Max seconds for a TTS'd message"
        if length > 60:
            return await ctx.send("Hey! You can't set max message length above 60 seconds!")
        if length < 20:
            return await ctx.send("Hey! You can't set max message length below 20 seconds!")

        await self.bot.settings.set(ctx.guild, "msg_length", length)
        await ctx.send(f"Max message length (in seconds) is now: {length}")

    @limits.command(aliases=("repeated_characters", "repeated_letters", "chars"))
    async def repeated_chars(self, ctx: utils.TypedGuildContext, chars: int):
        "Max repetion of a character (0 = off)"
        if chars > 100:
            return await ctx.send("Hey! You can't set max repeated chars above 100!")
        if chars < 5 and chars != 0:
            return await ctx.send("Hey! You can't set max repeated chars below 5!")

        await self.bot.settings.set(ctx.guild, "repeated_chars", chars)
        await ctx.send(f"Max repeated characters is now: {chars}")

    @commands.has_permissions(administrator=True)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.guild_only()
    @commands.command()
    async def setup(self, ctx: utils.TypedGuildContext, channel: discord.TextChannel):
        "Setup the bot to read messages from `<channel>`"
        await self.bot.settings.set(ctx.guild, "channel", channel.id)

        embed = discord.Embed(
            title="TTS Bot has been setup!",
            description=cleandoc(f"""
                TTS Bot will now accept commands and read from {channel.mention}.
                Just do `{ctx.prefix}join` and start talking!
            """)
        )
        embed.set_footer(text=pick_random(utils.FOOTER_MSGS))
        embed.set_thumbnail(url=self.bot.avatar_url)
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))

        await ctx.send(embed=embed)

    @commands.bot_has_permissions(send_messages=True)
    @commands.command(hidden=True)
    async def voice(self, ctx: utils.TypedGuildContext, lang: str):
        "Changes the language your messages are read in, full list in `-voices`"
        if lang in tts_langs:
            await self.bot.userinfo.set("lang", ctx.author, lang)
            await ctx.send(f"Changed your voice to: {langs_lookup[lang]}")
        else:
            await ctx.send(f"Invalid voice, do `{ctx.prefix}voices`")

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    async def voices(self, ctx: utils.TypedGuildContext, lang: Optional[str] = None):
        "Lists all the language codes that TTS bot accepts"
        if lang in tts_langs:
            return await self.voice(ctx, lang)

        lang = (await self.bot.userinfo.get("lang", ctx.author, default="en")).split("-")[0]
        langs_string = str(tts_langs).strip("{}")

        embed = discord.Embed(title="TTS Bot Languages")
        embed.set_footer(text=pick_random(utils.FOOTER_MSGS))
        embed.add_field(name="Currently Supported Languages", value=langs_string)
        embed.add_field(name="Current Language used", value=f"{langs_lookup[lang]} | {lang}")
        embed.set_author(name=ctx.author.display_name, icon_url=str(ctx.author.avatar_url))

        await ctx.send(embed=embed)
