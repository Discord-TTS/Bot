from __future__ import annotations

import asyncio
import functools
import re
from inspect import cleandoc
from random import choice as pick_random
from typing import (TYPE_CHECKING, Dict, List,
                    Optional, Tuple, TypeVar, Union, cast)

import discord
from discord.ext import commands, menus

import utils


if TYPE_CHECKING:
    from main import TTSBotPremium
    from typing_extensions import ParamSpec


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

TO_ENABLED = {True: "Enabled", False: "Disabled"}
class cmds_settings(utils.CommonCog, name="Settings"):
    "TTS Bot settings commands, configuration is done here."
    _translation_langs: Dict[str, str]

    @property
    async def translation_langs(self) -> Dict[str, str]:
        if not getattr(self, "_translation_langs", False):
            url = f"{utils.TRANSLATION_URL}/languages"
            params = {
                "type": "target",
                "auth_key": self.bot.config["Translation"]["key"]
            }

            async with self.bot.session.get(url, params=params) as resp:
                resp.raise_for_status()
                self._translation_langs = {
                    voice["language"].lower(): voice["name"]
                    for voice in await resp.json()
                }

        return self._translation_langs


    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.guild_only()
    @commands.command()
    async def settings(self, ctx: utils.TypedGuildContext):
        "Displays the current settings!"
        lang, variant, nickname = await asyncio.gather(
            self.bot.userinfo.get("lang", ctx.author, default="en-us"),
            self.bot.userinfo.get("variant", ctx.author, default="a"),
            self.bot.nicknames.get(ctx.guild, ctx.author)
        )

        xsaid, prefix, channel, auto_join, bot_ignore, msg_length, target_lang, default_lang, to_translate, repeated_chars = await self.bot.settings.get(
            ctx.guild,
            settings=[
                "xsaid",
                "prefix",
                "channel",
                "auto_join",
                "bot_ignore",
                "msg_length",
                "target_lang",
                "default_lang",
                "to_translate",
                "repeated_chars"
            ]
        )

        channel = ctx.guild.get_channel(channel)
        channel_name = channel.name if channel else "has not been setup yet"

        voice = await self.bot.safe_get_voice(ctx, lang, variant)
        default_voice = await self.bot.safe_get_voice(ctx, *default_lang.split()) if default_lang else None

        voice = voice if isinstance(voice, utils.Voice) else None
        default_voice = voice if isinstance(default_voice, utils.Voice) else None

        if nickname == ctx.author.display_name:
            nickname = "has not been set yet"


        # Show settings embed
        sep1, sep2, sep3, sep4 = utils.OPTION_SEPERATORS
        server_settings = cleandoc(f"""
            {sep1} Setup Channel: `#{channel_name}`
            {sep1} Auto Join: `{auto_join}`
            {sep1} Command Prefix: `{prefix}`
        """)
        tts_settings = cleandoc(f"""
            {sep2} <User> said: message `{xsaid}`
            {sep2} Ignore bot's messages: `{bot_ignore}`
            {sep2} Default Server Voice: `{default_voice}`

            {sep2} Max Time to Read: `{msg_length} seconds`
            {sep2} Max Repeated Characters: `{repeated_chars}`
        """)

        translation_settings = cleandoc(f"""
            {sep4} Translation: `{TO_ENABLED[to_translate]}`
            {sep4} Target Language: `{target_lang}`
        """)

        user_settings = cleandoc(f"""
            {sep3} Voice: `{voice}`
            {sep3} Nickname: `{nickname}`
        """)

        if not to_translate:
            # Crosses out target language if translation is off, this is a
            # terrible way to do it, if you can, clean this up.
            lines = translation_settings.split("\n")
            lines[1] = f"~~{lines[1]}~~"
            translation_settings = "\n".join(lines)

        embed = discord.Embed(title="Current Settings", url="https://discord.gg/zWPWwQC", color=0x3498db)
        embed.add_field(name="**General Server Settings**", value=server_settings,      inline=False)
        embed.add_field(name="**TTS Settings**",            value=tts_settings,         inline=False)
        embed.add_field(name="**Translation Settings (BETA)**",    value=translation_settings, inline=False)
        embed.add_field(name="**User Specific**",           value=user_settings,        inline=False)

        embed.set_footer(text=f"Change these settings with {ctx.prefix}set property value!")
        await ctx.send(embed=embed)


    @commands.bot_has_permissions(send_messages=True)
    @commands.guild_only()
    @commands.group()
    async def set(self, ctx: utils.TypedContext):
        "Changes a setting!"
        if ctx.invoked_subcommand is None:
            await ctx.send_help(ctx.command)

    @set.command()
    @commands.has_permissions(administrator=True)
    @utils.decos.make_fancy
    async def xsaid(self, ctx: utils.TypedGuildContext, value: bool):
        "Makes the bot say \"<user> said\" before each message"
        await self.bot.settings.set(ctx.guild, "xsaid", value)
        await ctx.send(f"xsaid is now: {TO_ENABLED[value]}")

    @set.command(aliases=["auto_join"])
    @commands.has_permissions(administrator=True)
    @utils.decos.make_fancy
    async def autojoin(self, ctx: utils.TypedGuildContext, value: bool):
        "If you type a message in the setup channel, the bot will join your vc"
        await self.bot.settings.set(ctx.guild, "auto_join", value)
        await ctx.send(f"Auto Join is now: {TO_ENABLED[value]}")

    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    @commands.has_permissions(administrator=True)
    @utils.decos.make_fancy
    async def botignore(self, ctx: utils.TypedGuildContext, value: bool):
        "Messages sent by bots and webhooks are not read"
        await self.bot.settings.set(ctx.guild, "bot_ignore", value)
        await ctx.send(f"Ignoring Bots is now: {TO_ENABLED[value]}")


    @set.command(aliases=["defaultlang", "default_lang", "defaultlanguage", "slang", "serverlanguage"])
    @commands.has_permissions(administrator=True)
    async def server_language(self, ctx: utils.TypedGuildContext, lang: str, variant: str = ""):
        "Changes the default language messages are read in"
        ret = await self.bot.safe_get_voice(ctx, lang, variant)
        if isinstance(ret, discord.Embed):
            return await ctx.send(embed=ret)

        voice = ret
        await self.bot.settings.set(ctx.guild, "default_lang", voice.raw)
        await ctx.send(f"Default language for this server is now: {voice}")


    @set.command(aliases=["translate", "to_translate", "should_translate"])
    async def translation(self, ctx: utils.TypedGuildContext, value: bool):
        await self.bot.settings.set(ctx.guild, "to_translate", value)
        await ctx.send(f"Translation is now: {TO_ENABLED[value]}")

    @set.command(aliases=["tlang", "tvoice", "target_voice", "target", "target_language"])
    @commands.has_permissions(administrator=True)
    async def target_lang(self, ctx: utils.TypedGuildContext, lang: Optional[str] = None):
        "Changes the target language for translation"
        lang = lang.lower() if lang else lang

        if not lang or lang.lower() not in await self.translation_langs:
            langs = str(list((await self.translation_langs).keys())).strip("[]")
            embed = discord.Embed(
                title="`lang` was not passed or was incorrect!",
                description=cleandoc(f"Supported languages: {langs}")
            )

            return await ctx.send(embed=embed)

        await self.bot.settings.set(ctx.guild, "target_lang", lang)
        await ctx.send(
            f"The target translation language is now: `{lang}`" + (
            f". You may want to enable translation with `{ctx.prefix}set translation on`"
            if not (await self.bot.settings.get(ctx.guild, ["to_translate"]))[0] else ""
        ))


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

    @set.command(aliases=("length", "max_length", "max_msg_length", "msglength", "maxlength", "msg_length"))
    async def max_time_to_read(self, ctx: utils.TypedGuildContext, length: int):
        "Max seconds for a TTS'd message"
        if length > 60:
            return await ctx.send("Hey! You can't set the Max Time to Read above 60 seconds!")
        if length < 20:
            return await ctx.send("Hey! You can't set the Max Time to Read below 20 seconds!")

        await self.bot.settings.set(ctx.guild, "msg_length", length)
        await ctx.send(f"Max Time to Read (in seconds) is now: {length}")

    @set.command(aliases=("repeated_characters", "repeated_letters", "chars"))
    async def repeated_chars(self, ctx: utils.TypedGuildContext, chars: int):
        "Max repetion of a character (0 = off)"
        if chars > 100:
            return await ctx.send("Hey! You can't set max repeated chars above 100!")
        if chars < 5 and chars != 0:
            return await ctx.send("Hey! You can't set max repeated chars below 5!")

        await self.bot.settings.set(ctx.guild, "repeated_chars", chars)
        await ctx.send(f"Max repeated characters is now: {chars}")


    @set.command(aliases=("voice", "lang", "_language"))
    async def language(self, ctx: utils.TypedGuildContext, lang: str, variant: str = ""):
        "Changes the language your messages are read in, full list in `-voices`"
        await self.voice(ctx, lang, variant)


    @commands.has_permissions(administrator=True)
    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.guild_only()
    @commands.command()
    @utils.decos.make_fancy
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
        embed.set_thumbnail(url=self.bot.user.avatar.url)
        embed.set_author(name=ctx.author.display_name, icon_url=ctx.author.avatar.url)

        await ctx.send(embed=embed)

    @commands.bot_has_permissions(read_messages=True, send_messages=True, embed_links=True)
    @commands.command()
    async def voice(self, ctx: utils.TypedContext, lang: str, variant: str = ""):
        "Changes the voice your messages are read in, full list in `-voices`"
        lang, variant = lang.lower(), variant.lower()
        ret = await self.bot.safe_get_voice(ctx, lang, variant)
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
    @utils.decos.require_voices
    async def voices(self, ctx: utils.TypedContext):
        "Lists all the language codes that TTS bot accepts"
        lang, variant = await asyncio.gather(
            self.bot.userinfo.get("lang", ctx.author, default="en-us"),
            self.bot.userinfo.get("variant", ctx.author, default="a")
        )

        langs = {voice.lang for voice in self.bot._voice_data}
        pages = sorted("\n".join(
            v.formatted for v in self.bot._voice_data if v.lang == lang
        ) for lang in langs)

        voice = await self.bot.get_voice(lang, variant)
        paginator = Paginator(voice, pages, per_page=1)
        menu = menus.MenuPages(source=paginator, clear_reactions_after=True)

        await menu.start(ctx)
