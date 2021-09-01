from __future__ import annotations

import asyncio
import re
from inspect import cleandoc
from random import choice as pick_random
from typing import TYPE_CHECKING, Optional

import discord
from discord.ext import commands
from gtts.lang import tts_langs as _tts_langs

import utils

if TYPE_CHECKING:
    from main import TTSBot


tts_langs: set[str] = set(_tts_langs().keys())
to_enabled = {True: "Enabled", False: "Disabled"}
langs_lookup: dict[str, str] = {lang: name for lang, name in _tts_langs().items()}

def setup(bot: TTSBot):
    bot.add_cog(SettingCommands(bot))

class SettingCommands(utils.CommonCog, name="Settings"):
    "TTS Bot settings commands, configuration is done here."

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.guild_only()
    @commands.command()
    async def settings(self, ctx: utils.TypedGuildContext):
        "Displays the current settings!"
        guild_settings = self.bot.settings[ctx.guild.id]
        userinfo, nickname = await asyncio.gather(
            self.bot.userinfo.get(ctx.author.id),
            self.bot.nicknames.get((ctx.guild.id, ctx.author.id))
        )

        lang = userinfo.get("lang", "en")
        nickname = nickname.get("name", ctx.author.display_name)

        channel = ctx.guild.get_channel(guild_settings["channel"])
        channel_name = channel.name if channel else "has not been setup yet"

        if nickname == ctx.author.display_name:
            nickname = "has not been set yet"

        # Show settings embed
        sep1, sep2, sep3 = utils.OPTION_SEPERATORS
        server_settings = cleandoc(f"""
            {sep1} Setup Channel: `#{channel_name}`
            {sep1} Auto Join: `{guild_settings['auto_join']}`
            {sep1} Command Prefix: `{guild_settings['prefix']}`
        """)
        tts_settings = cleandoc(f"""
            {sep2} <User> said: message `{guild_settings['xsaid']}`
            {sep2} Ignore bot's messages: `{guild_settings['bot_ignore']}`
            {sep2} Default Server Language: `{guild_settings['default_lang']}`

            {sep2} Max Time to Read: `{guild_settings['msg_length']} seconds`
            {sep2} Max Repeated Characters: `{guild_settings['repeated_chars']}`
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
    async def set(self, ctx: utils.TypedContext):
        "Changes a setting!"
        if ctx.invoked_subcommand is None:
            await ctx.send_help(ctx.command)

    @set.command()
    @commands.has_permissions(administrator=True)
    @utils.decos.bool_button
    async def xsaid(self, ctx: utils.TypedGuildContext, value: bool):
        "Makes the bot say \"<user> said\" before each message"
        await self.bot.settings.set(ctx.guild.id, {"xsaid": value})
        await ctx.reply(f"xsaid is now: {to_enabled[value]}")

    @set.command(aliases=["auto_join"])
    @commands.has_permissions(administrator=True)
    @utils.decos.bool_button
    async def autojoin(self, ctx: utils.TypedGuildContext, value: bool):
        "If you type a message in the setup channel, the bot will join your vc"
        await self.bot.settings.set(ctx.guild.id, {"auto_join": value})
        await ctx.reply(f"Auto Join is now: {to_enabled[value]}")

    @set.command(aliases=["bot_ignore", "ignore_bots", "ignorebots"])
    @commands.has_permissions(administrator=True)
    @utils.decos.bool_button
    async def botignore(self, ctx: utils.TypedGuildContext, value: bool):
        "Messages sent by bots and webhooks are not read"
        await self.bot.settings.set(ctx.guild.id, {"bot_ignore": value})
        await ctx.reply(f"Ignoring Bots is now: {to_enabled[value]}")


    @set.command(aliases=["defaultlang", "default_lang", "defaultlanguage", "slang", "serverlanguage"])
    @commands.has_permissions(administrator=True)
    async def server_language(self, ctx: utils.TypedGuildContext, language: str):
        "Changes the default language messages are read in"
        if language not in tts_langs:
            return await ctx.send(f"Invalid voice, do `{ctx.prefix}voices`")

        await self.bot.settings.set(ctx.guild.id, {"default_lang": language})
        await ctx.send(f"Default language for this server is now: {language}")

    @set.command()
    @commands.has_permissions(administrator=True)
    async def prefix(self, ctx: utils.TypedGuildContext, *, prefix: str):
        """The prefix used before commands"""
        if len(prefix) > 5 or prefix.count(" ") > 1:
            return await ctx.send("**Error**: Invalid Prefix! Please use 5 or less characters with maximum 1 space.")

        await self.bot.settings.set(ctx.guild.id, {"prefix": prefix})
        await ctx.send(f"Command Prefix is now: {prefix}")

    @set.command(aliases=["nick_name", "nickname", "name"])
    @commands.bot_has_permissions(embed_links=True)
    async def nick(self, ctx: utils.TypedGuildContext, optional_user: Optional[discord.Member] = None, *, nickname: str):
        "Replaces your username in \"<user> said\" with a given name"
        user = optional_user or ctx.author
        nickname = nickname or ctx.author.display_name

        if user != ctx.author and not ctx.author_permissions().administrator:
            return await ctx.send("Error: You need admin to set other people's nicknames!")

        if "<" in nickname and ">" in nickname:
            await ctx.send("Hey! You can't have mentions/emotes in your nickname!")
        elif not re.match(r"^(\w|\s)+$", nickname):
            await ctx.send("Hey! Please keep your nickname to only letters, numbers, and spaces!")
        else:
            await asyncio.gather(
                self.bot.settings.set(ctx.guild.id, {}),
                self.bot.userinfo.set(ctx.author.id, {})
            )

            self.bot.nicknames[(ctx.guild.id, user.id)] = {"name": nickname}
            await ctx.send(embed=discord.Embed(title="Nickname Change", description=f"Changed {user.name}'s nickname to {nickname}"))

    @set.command(aliases=("length", "max_length", "max_msg_length", "msglength", "maxlength", "msg_length"))
    async def max_time_to_read(self, ctx: utils.TypedGuildContext, length: int):
        "Max seconds for a TTS'd message"
        if length > 60:
            return await ctx.send("Hey! You can't set the Max Time to Read above 60 seconds!")
        if length < 20:
            return await ctx.send("Hey! You can't set the Max Time to Read below 20 seconds!")

        await self.bot.settings.set(ctx.guild.id, {"msg_length": length})
        await ctx.send(f"Max Time to Read (in seconds) is now: {length}")

    @set.command(aliases=("repeated_characters", "repeated_letters", "chars"))
    async def repeated_chars(self, ctx: utils.TypedGuildContext, chars: int):
        "Max repetion of a character (0 = off)"
        if chars > 100:
            return await ctx.send("Hey! You can't set max repeated chars above 100!")
        if chars < 5 and chars != 0:
            return await ctx.send("Hey! You can't set max repeated chars below 5!")

        await self.bot.settings.set(ctx.guild.id, {"repeated_chars": chars})
        await ctx.send(f"Max repeated characters is now: {chars}")


    @set.command(aliases=("voice", "lang", "_language"))
    async def language(self, ctx: utils.TypedGuildContext, voicecode: str):
        "Changes the language your messages are read in, full list in `-voices`"
        await self.voice(ctx, voicecode)


    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.has_permissions(administrator=True)
    @commands.guild_only()
    @commands.command()
    async def setup(self, ctx: utils.TypedGuildContext, channel: Optional[discord.TextChannel]):
        "Setup the bot to read messages from `<channel>`"
        if channel is None:
            select_view = utils.CommandView[discord.TextChannel](ctx)
            for channels in discord.utils.as_chunks((channel
                for channel in ctx.guild.text_channels
                if (channel.permissions_for(ctx.guild.me).read_messages
                    and channel.permissions_for(ctx.author).read_messages)
            ), 25):
                try:
                    select_view.add_item(utils.ChannelSelector(ctx, channels))
                except ValueError:
                    return await ctx.send_error(
                        error="we cannot show a menu as this server has too many channels",
                        fix=f"use the slash command or do `{ctx.prefix}setup #channel`"
                    )

            await ctx.reply("Select a channel!", view=select_view)
            channel = await select_view.wait()

        embed = discord.Embed(
            title="TTS Bot has been setup!",
            description=cleandoc(f"""
                TTS Bot will now accept commands and read from {channel.mention}.
                Just do `{ctx.prefix}join` and start talking!
            """)
        )
        embed.set_footer(text=pick_random(utils.FOOTER_MSGS))
        embed.set_thumbnail(url=self.bot.user.display_avatar.url)
        embed.set_author(name=ctx.author.display_name, icon_url=ctx.author.display_avatar.url)

        await self.bot.settings.set(ctx.guild.id, {"channel": channel.id})
        await ctx.send(embed=embed)

    @commands.bot_has_permissions(send_messages=True)
    @commands.command(hidden=True)
    async def voice(self, ctx: utils.TypedGuildContext, lang: str):
        "Changes the language your messages are read in, full list in `-voices`"
        if lang in tts_langs:
            await self.bot.userinfo.set(ctx.author.id, {"lang": lang})
            await ctx.send(f"Changed your voice to: {langs_lookup[lang]}")
        else:
            await ctx.send(f"Invalid voice, do `{ctx.prefix}voices`")

    @commands.bot_has_permissions(send_messages=True, embed_links=True)
    @commands.command(aliases=["languages", "list_languages", "getlangs", "list_voices"])
    async def voices(self, ctx: utils.TypedGuildContext, lang: Optional[str] = None):
        "Lists all the language codes that TTS bot accepts"
        if lang in tts_langs:
            return await self.voice(ctx, lang)

        userinfo = await self.bot.userinfo.get(ctx.author.id)
        langs_string = str(tts_langs).strip("{}")
        lang = userinfo.get("lang")
        if lang is None:
            lang = "en"

        embed = discord.Embed(title="TTS Bot Languages")
        embed.set_footer(text=pick_random(utils.FOOTER_MSGS))
        embed.add_field(name="Currently Supported Languages", value=langs_string)
        embed.add_field(name="Current Language used", value=f"{langs_lookup[lang]} | {lang}")
        embed.set_author(name=ctx.author.display_name, icon_url=ctx.author.display_avatar.url)

        await ctx.send(embed=embed)
