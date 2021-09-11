from __future__ import annotations

import asyncio
from functools import partial
from inspect import cleandoc
from io import BytesIO
from shlex import split
from subprocess import PIPE, Popen
from typing import TYPE_CHECKING, Optional, cast

import asyncgTTS
import discord
from discord.ext import tasks
from discord.opus import Encoder

import utils

if TYPE_CHECKING:
    from main import TTSBot

class FFmpegPCMAudio(discord.AudioSource):
    """Reimplementation of discord.FFmpegPCMAudio with source: bytes support
    Original Source: https://github.com/Rapptz/discord.py/issues/5192"""

    def __init__(self, source, *, executable="ffmpeg", pipe=False, stderr=None, before_options=None, options=None):
        args = [executable]
        if isinstance(before_options, str):
            args.extend(split(before_options))

        args.append("-i")
        args.append("-" if pipe else source)
        args.extend(("-f", "s16le", "-ar", "48000", "-ac", "2", "-loglevel", "warning"))

        if isinstance(options, str):
            args.extend(split(options))

        args.append("pipe:1")

        self._stdout = None
        self._process = None
        self._stderr = stderr
        self._process_args = args
        self._stdin = source if pipe else None

    def _create_process(self) -> BytesIO:
        stdin, stderr, args = self._stdin, self._stderr, self._process_args
        self._process = Popen(args, stdin=PIPE, stdout=PIPE, stderr=stderr)
        return BytesIO(self._process.communicate(input=stdin)[0])

    def read(self) -> bytes:
        if self._stdout is None:
            # This function runs in a voice thread, so we can afford to block
            # it and make the process now instead of in the main thread
            self._stdout = self._create_process()

        ret = self._stdout.read(Encoder.FRAME_SIZE)
        return ret if len(ret) == Encoder.FRAME_SIZE else b""

    def cleanup(self):
        process = self._process
        if process is None:
            return

        process.kill()
        if process.poll() is None:
            process.communicate()

        self._process = None


_MessageQueue = tuple[str, str]
class TTSVoiceClient(discord.VoiceClient, utils.TTSAudioMaker):
    bot: TTSBot
    guild: discord.Guild
    channel: utils.VoiceChannel

    def __init__(self, bot: TTSBot, channel: discord.VoiceChannel):
        super().__init__(bot, channel)

        self.bot = bot
        self.prefix = None
        self.linked_channel = 0

        self.audio_buffer = utils.ClearableQueue[utils.AUDIODATA](maxsize=5)
        self.message_queue = utils.ClearableQueue[_MessageQueue]()

        self.fill_audio_buffer.start()

    def __repr__(self):
        c = self.channel.id
        is_playing = self.is_playing()
        is_connected = self.is_connected()

        abufferlen = self.audio_buffer.qsize()
        mqueuelen = self.message_queue.qsize()

        return f"<TTSVoiceClient: {c=} {is_connected=} {is_playing=} {mqueuelen=} {abufferlen=}>"


    async def disconnect(self, *, force: bool = False) -> None:
        await super().disconnect(force=force)
        self.fill_audio_buffer.cancel()
        self.play_audio.cancel()

    def play(self, source: discord.AudioSource) -> asyncio.Future[None]:
        future: asyncio.Future[None] = self.bot.loop.create_future()
        def _after_play(exception: Optional[Exception]) -> None:
            if exception is None:
                future.set_result(None)
            else:
                future.set_exception(exception)

        super().play(source, after=partial(self.bot.loop.call_soon_threadsafe, _after_play))
        return future


    async def queue(self, text: str, lang: str, linked_channel: int, prefix: str, max_length: int = 30) -> None:
        self.prefix = prefix
        self.max_length = max_length
        self.linked_channel = linked_channel

        await self.message_queue.put((text, lang))
        if not self.fill_audio_buffer.is_running():
            self.fill_audio_buffer.start()

    def skip(self):
        self.audio_buffer.clear()
        self.message_queue.clear()

        self.stop()
        self.play_audio.restart()
        self.fill_audio_buffer.restart()


    @tasks.loop()
    @utils.decos.handle_errors
    async def play_audio(self):
        audio, length = await self.audio_buffer.get()
        if not self.is_connected():
            self.play_audio.stop()
            return await self.disconnect(force=True)

        source = FFmpegPCMAudio(audio, pipe=True, options='-loglevel "quiet"')

        try:
            await asyncio.wait_for(self.play(source), timeout=length+5)
        except asyncio.TimeoutError:
            self.bot.log("on_play_timeout")
            self.bot.logger.debug(f"`{self.guild.id}`'s vc.play didn't finish audio!")
        except Exception as exception:
            await self.bot.on_error("play_audio", exception, self)

    @tasks.loop()
    @utils.decos.handle_errors
    async def fill_audio_buffer(self):
        sleep_timeout = asyncio.create_task(asyncio.sleep(30*60))
        fetch_queue = asyncio.create_task(self.message_queue.get())
        _, pending = await asyncio.wait((fetch_queue, sleep_timeout), return_when=asyncio.FIRST_COMPLETED)

        if fetch_queue in pending:
            fetch_queue.cancel()
            return await self.disconnect()
        else:
            sleep_timeout.cancel()

        text, lang = fetch_queue.result()
        try:
            audio, length = await self.get_tts(text, lang, self.max_length)
        except asyncio.TimeoutError:
            error = f"`{self.guild.id}`'s `{len(text)}` character long message was cancelled!"
            return self.bot.logger.debug(error)

        if audio is None or length is None:
            return

        await self.audio_buffer.put((audio, length))
        if not self.play_audio.is_running():
            self.play_audio.start()

    async def get_gtts(self, text: str, lang: str):
        try:
            return await super().get_gtts(text, lang)
        except asyncgTTS.RatelimitException:
            if self.bot.blocked:
                return

            self.bot.blocked = True
            if await self.bot.check_gtts() is not True:
                self.bot.create_task(self._handle_rl())
            else:
                self.bot.blocked = False

            return (await self.get_tts(text, lang, self.max_length))[0]
        except asyncgTTS.easygttsException as error:
            error_message = str(error)
            response_code = error_message[:3]
            if response_code in {"400", "500"}:
                return

            raise

    # easygTTS -> espeak handling
    async def _handle_rl(self):
        self.bot.logger.warning("Swapping to espeak")
        self.bot.create_task(self._handle_rl_reset())
        if self.bot.sent_fallback:
            return

        self.bot.sent_fallback = True

        await asyncio.gather(*(vc._send_fallback() for vc in self.bot.voice_clients))
        self.bot.logger.info("Fallback/RL messages have been sent.")

    async def _handle_rl_reset(self):
        await asyncio.sleep(3601)
        while True:
            ret = await self.bot.check_gtts()
            if ret:
                break
            elif isinstance(ret, Exception):
                self.bot.logger.warning("**Failed to connect to easygTTS for unknown reason.**")
            else:
                self.bot.logger.info("**Rate limit still in place, waiting another hour.**")

            await asyncio.sleep(3601)

        self.bot.logger.info("**Swapping back to easygTTS**")
        self.bot.blocked = False

    @utils.decos.handle_errors
    async def _send_fallback(self):
        guild = self.guild
        if not guild or guild.unavailable:
            return

        channel = cast(discord.TextChannel, guild.get_channel(self.linked_channel))
        if not channel:
            return

        permissions: discord.Permissions = channel.permissions_for(guild.me)
        if permissions.send_messages and permissions.embed_links:
            await channel.send(embed=await self._get_embed())

    async def _get_embed(self):
        prefix = self.prefix or self.bot.settings[self.guild.id].get("prefix", "-")

        return discord.Embed(
            title="TTS Bot has been blocked by Google",
            description=cleandoc(f"""
            During this temporary block, voice has been swapped to a worse quality voice.
            If you want to avoid this, consider TTS Bot Premium, which you can get by donating via Patreon: `{prefix}donate`
            """)
        ).set_footer(text="You can join the support server for more info: discord.gg/zWPWwQC")
