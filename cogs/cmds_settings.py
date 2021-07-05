from __future__ import annotations

import asyncio
import functools
import re
from dataclasses import dataclass
from inspect import cleandoc
from random import choice as pick_random
from typing import (Any, Coroutine, TYPE_CHECKING, Awaitable, Callable, Dict, List, Optional,
                    Tuple, TypeVar, Union, cast)

import discord
from discord.ext import commands, menus  # type: ignore
from typing_extensions import ParamSpec

import utils


if TYPE_CHECKING:
    from main import TTSBotPremium


to_enabled = {True: "Enabled", False: "Disabled"}

Return = TypeVar("Return")
Params = ParamSpec("Params")

def require_voices(func: Callable[Params, Awaitable[Return]]) -> Callable[Params, Coroutine[Any, Any, Return]]:
    @functools.wraps(func)
    async def wrapper(*args: Params.args, **kwargs: Params.kwargs) -> Return:
        self = cast(cmds_settings, args[0])
        if not getattr(self, "_voice_data", None):
            self._voice_data = sorted(
                [Voice(
                    voice_name=v["name"],

                    variant=v["name"][-1].lower(),
                    lang=v["languageCodes"][0].lower(),
                    gender=v["ssmlGender"].capitalize()
                )
                for v in await self.bot.gtts.get_voices()
                if "Standard" in v["name"]],
                key=lambda v: v.formatted
            )

        return await func(*args, **kwargs)
    return wrapper

class VoiceNotFound(Exception): pass

@dataclass
class Voice:
    voice_name: str

    lang: str
    gender: str
    variant: str

    @property
    def tuple(self) -> Tuple[str, str]:
        return self.voice_name, self.lang

    @property
    def raw(self) -> str:
        return f"{self.lang} {self.variant}"

    @property
    def formatted(self) -> str:
        return f"{self.lang} - {self.variant} ({self.gender})"

    def __repr__(self):
        return f"<Voice {self.lang=} {self.variant=} {self.gender=}>"

    def __str__(self):
        return self.formatted


class Paginator(menus.ListPageSource):
    def __init__(self, current_lang, *args, **kwargs):
        self.current_lang = current_lang
        super().__init__(*args, **kwargs)

    async def format_page(self, menu, entries):
        embed = discord.Embed(title=f"{menu.ctx.bot.user.name} Languages", description=f"**Currently Supported Languages**\n{entries}")
        embed.add_field(name="Current Language used", value=self.current_lang)

        embed.set_author(name=menu.ctx.author.name, icon_url=menu.ctx.author.avatar_url)
        embed.set_footer(text=pick_random(utils.FOOTER_MSGS))
        return embed


def setup(bot: TTSBotPremium):
    bot.add_cog(cmds_settings(bot))

class cmds_settings(utils.CommonCog, name="Settings"):
    "TTS Bot settings commands, configuration is done here."
    _voice_data: List[Voice]

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.guild_only()
    @commands.command()
    async def settings(self, ctx: utils.TypedGuildContext, *, help: Optional[str] = None):
        "Displays the current settings!"

        if help:
            return await ctx.send_help(f"set{' limit' if help.lower() == 'limits' else ''}")

        lang, variant, nickname = await asyncio.gather(
            self.bot.userinfo.get("lang", ctx.author, default="en-us"),
            self.bot.userinfo.get("variant", ctx.author, default="a"),
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
                "default_lang",
            ]
        )

        channel = ctx.guild.get_channel(channel)
        channel_name = channel.name if channel else "has not been setup yet"

        voice = await self.safe_get_voice(ctx, lang, variant)
        default_voice = await self.safe_get_voice(ctx, *default_lang.split()) if default_lang else None

        voice = voice if isinstance(voice, Voice) else None
        default_voice = voice if isinstance(default_voice, Voice) else None

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
            {sep2} Default Server Voice: `{default_voice}`
        """)

        user_settings = cleandoc(f"""
            {sep3} Voice: `{voice}`
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
    async def server_language(self, ctx: utils.TypedGuildContext, lang: str, variant: str = ""):
        "Changes the default language messages are read in"
        ret = await self.safe_get_voice(ctx, lang, variant)
        if isinstance(ret, discord.Embed):
            return await ctx.send(embed=ret)

        voice = ret
        await self.bot.settings.set(ctx.guild, "default_lang", voice.raw)
        await ctx.send(f"Default language for this server is now: {voice}")

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

        if user != ctx.author and not ctx.channel.permissions_for(ctx.author).administrator: # type: ignore
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
    async def channel(self, ctx: commands.Context, channel: discord.TextChannel):
        "Alias of `-setup`"
        await self.setup(ctx, channel)

    @set.command(aliases=("voice", "lang"))
    async def language(self, ctx: commands.Context, lang: str, variant: str = ""):
        "Changes the language your messages are read in, full list in `-voices`"
        await self.voice(ctx, lang, variant)


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

    @require_voices
    async def get_voice(self, lang: str, variant: Optional[str] = None) -> Voice:
        generator = {
            True:  (voice for voice in self._voice_data if voice.lang == lang and voice.variant == variant),
            False: (voice for voice in self._voice_data if voice.lang == lang)
        }

        voice = next(generator[bool(variant)], None)
        if not voice:
            raise VoiceNotFound(f"Cannot find voice with {lang} {variant}")

        return voice

    async def safe_get_voice(self, ctx: commands.Context, lang: str, variant: Optional[str] = None) -> Union[Voice, discord.Embed]:
        try:
            return await self.get_voice(lang, variant)
        except VoiceNotFound:
            embed = discord.Embed(title=f"Cannot find voice with language `{lang}` and variant `{variant}` combo!")
            embed.set_author(name=str(ctx.author), icon_url=str(ctx.author.avatar_url))
            embed.set_footer(text=f"Try {ctx.prefix}voices for a full list!")
            return embed

    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command(hidden=True)
    async def voice(self, ctx, lang: str, variant = ""):
        "Changes the voice your messages are read in, full list in `-voices`"
        lang, variant = lang.lower(), variant.lower()
        ret = await self.safe_get_voice(ctx, lang, variant)
        if isinstance(ret, discord.Embed):
            return await ctx.send(embed=ret)

        voice = ret
        await asyncio.gather(
            self.bot.userinfo.set("lang", ctx.author, lang),
            self.bot.userinfo.set("variant", ctx.author, variant)
        )

        await ctx.send(f"Changed your voice to: {voice}")

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    @require_voices
    async def voices(self, ctx: commands.Context):
        "Lists all the language codes that TTS bot accepts"
        lang, variant = await asyncio.gather(
            self.bot.userinfo.get("lang", ctx.author, default="en-us"),
            self.bot.userinfo.get("variant", ctx.author, default="a")
        )

        langs = {voice.lang for voice in self._voice_data}
        pages = sorted("\n".join(v.formatted for v in self._voice_data if v.lang == lang) for lang in langs)

        menu = menus.MenuPages(source=Paginator(await self.get_voice(lang, variant), pages, per_page=1), clear_reactions_after=True)
        await menu.start(ctx)
