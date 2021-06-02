from __future__ import annotations

import asyncio
from functools import partial as make_func
from inspect import cleandoc
from io import BytesIO
from shlex import split
from subprocess import PIPE, Popen, SubprocessError
from typing import Optional, TYPE_CHECKING, Tuple, cast

import asyncgTTS
import discord
from discord.ext import tasks
from discord.opus import Encoder
from mutagen import mp3 as mutagen

from espeak_process import make_espeak
from utils.decos import handle_errors


if TYPE_CHECKING:
    from main import TTSBot


class FFmpegPCMAudio(discord.AudioSource):
    """TEMP FIX FOR DISCORD.PY BUG
    Orignal Source = https://github.com/Rapptz/discord.py/issues/5192
    Currently fixes `io.UnsupportedOperation: fileno` when piping a file-like object into FFmpegPCMAudio
    If this bug is fixed, notify me via Discord (Gnome!#6669) or PR to remove this file with a link to the discord.py commit that fixes this.
    """
    def __init__(self, source, *, executable='ffmpeg', pipe=False, stderr=None, before_options=None, options=None):
        stdin = source if pipe else None
        args = [executable]

        if isinstance(before_options, str):
            args.extend(split(before_options))

        args.append('-i')
        args.append('-' if pipe else source)
        args.extend(('-f', 's16le', '-ar', '48000', '-ac', '2', '-loglevel', 'warning'))

        if isinstance(options, str):
            args.extend(split(options))

        args.append('pipe:1')
        self._process = None
        try:
            self._process = Popen(args, stdin=PIPE, stdout=PIPE, stderr=stderr)
            self._stdout = BytesIO(self._process.communicate(input=stdin)[0])
        except FileNotFoundError:
            raise discord.ClientException(f"{executable} was not found.") from None
        except SubprocessError as exc:
            raise discord.ClientException(f"Popen failed: {exc.__class__.__name__}: {exc}") from exc

    def read(self):
        ret = self._stdout.read(Encoder.FRAME_SIZE)
        if len(ret) != Encoder.FRAME_SIZE:
            return b''
        return ret

    def cleanup(self):
        proc = self._process
        if proc is None:
            return
        proc.kill()
        if proc.poll() is None:
            proc.communicate()

        self._process = None


class TTSVoicePlayer(discord.VoiceClient):
    bot: TTSBot
    guild: discord.Guild
    channel: discord.VoiceChannel

    def __init__(self, bot: TTSBot, channel: discord.VoiceChannel):
        super().__init__(bot, channel)

        self.bot = bot
        self.prefix = None

        self.currently_playing = asyncio.Event()
        self.currently_playing.set()

        self.audio_buffer = asyncio.Queue(maxsize=5)
        self.message_queue = asyncio.Queue()

        self.fill_audio_buffer.start()

    def __repr__(self):
        c = self.channel.id
        abufferlen = self.audio_buffer.qsize()
        mqueuelen = self.message_queue.qsize()
        playing_audio = not self.currently_playing.is_set()

        return f"<TTSVoicePlayer: {c=} {playing_audio=} {mqueuelen=} {abufferlen=}>"


    async def disconnect(self, *, force: bool) -> None:
        await super().disconnect(force=force)
        self.fill_audio_buffer.cancel()
        self.play_audio.cancel()


    async def get_embed(self):
        prefix = self.prefix or await self.bot.settings.get(self.guild, "prefix")
        return discord.Embed(
            title="TTS Bot has been blocked by Google",
            description=cleandoc(f"""
            During this temporary block, voice has been swapped to a worse quality voice.
            If you want to avoid this, consider TTS Bot Premium, which you can get by donating via Patreon: `{prefix}donate`
            """)
        ).set_footer(text="You can join the support server for more info: discord.gg/zWPWwQC")

    async def queue(self, message: discord.Message, text: str, lang: str, linked_channel: int, prefix: str, max_length: int = 30) -> None:
        self.prefix = prefix
        self.max_length = max_length
        self.linked_channel = linked_channel

        await self.message_queue.put((message, text, lang))
        if not self.fill_audio_buffer.is_running:
            self.fill_audio_buffer.start()

    def skip(self):
        self.message_queue = asyncio.Queue()
        self.audio_buffer = asyncio.Queue(maxsize=5)

        self.stop()
        self.play_audio.restart()
        self.fill_audio_buffer.restart()


    @tasks.loop()
    @handle_errors
    async def play_audio(self):
        self.currently_playing.clear()
        audio, length = await self.audio_buffer.get()

        try:
            self.play(
                FFmpegPCMAudio(audio, pipe=True, options='-loglevel "quiet"'),
                after=lambda _: self.currently_playing.set()
            )
        except discord.ClientException:
            self.currently_playing.set()

        try:
            await asyncio.wait_for(self.currently_playing.wait(), timeout=length+5)
        except asyncio.TimeoutError:
            await self.bot.channels["errors"].send(cleandoc(f"""
                ```asyncio.TimeoutError```
                `{self.guild.id}`'s vc.play didn't finish audio!
            """))

    @tasks.loop()
    @handle_errors
    async def fill_audio_buffer(self):
        message, text, lang = await self.message_queue.get()
        ret_values = await self.get_tts(message, text, lang)
        if not ret_values or len(ret_values) == 1:
            return

        audio, file_length = ret_values
        if not audio or file_length > self.max_length:
            return

        await self.audio_buffer.put((audio, file_length))
        if not self.play_audio.is_running():
            self.play_audio.start()


    async def get_tts(self, message: discord.Message, text: str, lang: str) -> Optional[Tuple[bytes, int]]:
        lang = lang.split("-")[0]
        if self.bot.blocked:
            make_espeak_func = make_func(make_espeak, text, lang)
            return await self.bot.loop.run_in_executor(self.bot.executor, make_espeak_func)

        cached_mp3 = await self.bot.cache.get(text, lang, message.id) # type: ignore
        if cached_mp3:
            return cached_mp3, int(mutagen.MP3(BytesIO(cached_mp3)).info.length)

        try:
            audio = await self.bot.gtts.get(text=text, lang=lang)
        except asyncgTTS.RatelimitException:
            if self.bot.blocked:
                return

            self.bot.blocked = True
            if await self.bot.check_gtts() is not True:
                await self.handle_rl()
            else:
                self.bot.blocked = False

            return await self.get_tts(message, text, lang)

        except asyncgTTS.easygttsException as e:
            if str(e)[:3] != "400":
                raise

            return

        file_length = int(mutagen.MP3(BytesIO(audio)).info.length)
        await self.bot.cache.set(text, lang, message.id, audio)
        return audio, file_length


    # easygTTS -> espeak handling
    async def handle_rl(self):
        await self.bot.channels["logs"].send("**Swapping to espeak**")

        asyncio.create_task(self.handle_rl_reset())
        if not self.bot.sent_fallback:
            self.bot.sent_fallback = True

            send_fallback_coros = [vc.send_fallback() for vc in self.bot.voice_clients]
            await asyncio.gather(*(send_fallback_coros))
            await self.bot.channels["logs"].send("**Fallback/RL messages have been sent.**")

    async def handle_rl_reset(self):
        await asyncio.sleep(3601)
        while True:
            ret = await self.bot.check_gtts()
            if ret:
                break
            elif isinstance(ret, Exception):
                await self.bot.channels["logs"].send("**Failed to connect to easygTTS for unknown reason.**")
            else:
                await self.bot.channels["logs"].send("**Rate limit still in place, waiting another hour.**")

            await asyncio.sleep(3601)

        await self.bot.channels["logs"].send("**Swapping back to easygTTS**")
        self.bot.blocked = False

    @handle_errors
    async def send_fallback(self):
        guild = self.guild
        if not guild or guild.unavailable:
            return

        channel = cast(discord.TextChannel, guild.get_channel(self.linked_channel))
        if not channel:
            return

        permissions = channel.permissions_for(guild.me)
        if permissions.send_messages and permissions.embed_links:
            await channel.send(embed=await self.get_embed())
