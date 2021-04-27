import asyncio
from inspect import cleandoc
from io import BytesIO
from shlex import split
from subprocess import PIPE, Popen, SubprocessError
from typing import Optional

import discord
from discord.ext import tasks
from discord.opus import Encoder
from mutagen import mp3 as mutagen


class FFmpegPCMAudio(discord.AudioSource):
    """TEMP FIX FOR DISCORD.PY BUG
    Orignal Source = https://github.com/Rapptz/discord.py/issues/5192
    Currently fixes `io.UnsupportedOperation: fileno` when piping a file-like object into FFmpegPCMAudio
    If this bug is fixed, notify me via Discord (Gnome!#6669) or PR to remove this file with a link to the discord.py commit that fixes this.
    """
    def __init__(self, source, *, executable='ffmpeg', pipe=False, stderr=None, before_options=None, options=None):
        stdin = None if not pipe else source
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
            self._stdout = BytesIO(
                self._process.communicate(input=stdin)[0]
            )
        except FileNotFoundError:
            raise discord.ClientException(executable + ' was not found.') from None
        except SubprocessError as exc:
            raise discord.ClientException('Popen failed: {0.__class__.__name__}: {0}'.format(exc)) from exc

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
    def __init__(self, client, channel):
        super().__init__(client, channel)
        self.bot = client

        self.currently_playing = asyncio.Event()
        self.message_queue = asyncio.Queue()
        self.audio_buffer = asyncio.Queue(maxsize=5)

        self.fill_audio_buffer.start()

    def __repr__(self):
        return f"<TTSVoicePlayer: c={self.channel.id} playing={not self.currently_playing.is_set()} mqueuelen={self.message_queue.qsize()} abufferlen={self.audio_buffer.qsize()}>"


    async def queue(self, message: discord.Message, text: str, lang: str, max_length: str):
        self.max_length = int(max_length)
        await self.message_queue.put((message, text, lang))

    def skip(self):
        self.message_queue = asyncio.Queue()
        self.audio_buffer = asyncio.Queue(maxsize=5)

        self.stop()
        self.play_audio.restart()
        self.fill_audio_buffer.restart()


    @tasks.loop()
    async def play_audio(self):
        self.currently_playing.clear()
        audio, length = await self.audio_buffer.get()

        try:
            self.play(
                FFmpegPCMAudio(audio, pipe=True, options='-loglevel "quiet"'),
                after=lambda error: self.currently_playing.set()
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
    async def fill_audio_buffer(self):
        message, text, lang = await self.message_queue.get()
        audio, file_length = await self.get_tts(message, text, lang)

        if not audio or file_length > self.max_length:
            return

        await self.audio_buffer.put((audio, file_length))
        if not self.play_audio.is_running():
            self.play_audio.start()

    @play_audio.error
    async def audio_play_error(self, error):
        await self.bot.on_error("audio_play", error)
        self.play_audio.start()

    @fill_audio_buffer.error
    async def message_to_tts_error(self, error):
        await self.bot.on_error("fill_buffer", error)
        self.fill_audio_buffer.start()


    async def get_tts(self, message: discord.Message, text: str, lang: str):
        lang = lang.split("-")[0]

        cached_mp3 = await self.bot.cache.get(text, lang, message.id)
        if cached_mp3:
            return cached_mp3, int(mutagen.MP3(BytesIO(cached_mp3)).info.length)

        if self.bot.blocked:
            make_espeak_func = make_func(make_espeak, text, lang, max_length)
            audio, file_length = await self.bot.loop.run_in_executor(self.bot.executor, make_espeak_func)
        else:
            try:
                audio = await self.bot.gtts.get(text=text, lang=lang)
            except asyncgTTS.RatelimitException:
                if self.bot.blocked:
                    return

                self.bot.blocked = True
                if await self.check_gtts() is True:
                    self.bot.blocked = False
                    return self.get_tts(message, text, lang)

                await self.handle_rl(prefix)
            except asyncgTTS.easygttsException as e:
                if str(e)[:3] == "400":
                    return
                raise

            file_length = int(mutagen.MP3(BytesIO(audio)).info.length)
            await self.bot.cache.set(text, lang, message.id, audio)

        return audio, file_length

    # easygTTS -> espeak handling
    async def check_gtts(self):
        try:
            await self.bot.gtts.get(text="RL Test", lang="en")
            return True
        except asyncgTTS.RatelimitException:
            return False
        except Exception as e:
            return e

    async def handle_rl(self, prefix):
        await self.bot.channels["logs"].send("**Swapping to espeak**")
        asyncio.create_task(self.handle_rl_reset())

        embed = discord.Embed(title="TTS Bot has been blocked by Google")
        embed.description = cleandoc(f"""
            During this temporary block, voice has been swapped to a worse quality voice.
            If you want to avoid this, consider TTS Bot Premium, which you can get by donating via Patreon: `{prefix}donate`
            """)
        embed.set_footer(text="You can join the support server for more info: discord.gg/zWPWwQC")

        for voice_client in self.bot.voice_clients:
            channel_id = await self.bot.settings.get(voice_client.guild, setting="channel")
            channel = voice_client.guild.get_channel(int(channel_id))

            if not channel:
                continue

            permissions = channel.permissions_for(voice_client.guild.me)
            if permissions.send_messages and permissions.embed_links:
                try:
                    await channel.send(embed=embed)
                    await asyncio.sleep(1)
                except:
                    pass

        await self.bot.channels["logs"].send("**Fallback/RL messages have been sent.**")

    async def handle_rl_reset(self):
        while True:
            ret = await self.check_gtts()
            if ret:
                break
            elif isinstance(e, Exception):
                await self.bot.channels["logs"].send("**Failed to connect to easygTTS for unknown reason.**")
            else:
                await self.bot.channels["logs"].send("**Rate limit still in place, waiting another hour.**")

            await asyncio.sleep(3601)

        await self.bot.channels["logs"].send("**Swapping back to easygTTS**")
        self.bot.blocked = False
