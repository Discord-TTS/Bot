""" TEMP FIX FOR DISCORD.PY BUG
Orignal Source = https://github.com/Rapptz/discord.py/issues/5192
Currently fixes `io.UnsupportedOperation: fileno` when piping a file-like object into FFmpegPCMAudio
If this bug is fixed, notify me via Discord (Gnome!#6669) or PR to remove this file with a link to the discord.py commit that fixes this. 
"""

from io import BytesIO
from shlex import split
from subprocess import PIPE, Popen, SubprocessError

from discord import AudioSource, ClientException
from discord.opus import Encoder 


class FFmpegPCMAudio(AudioSource):
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
            raise ClientException(executable + ' was not found.') from None
        except SubprocessError as exc:
            raise ClientException('Popen failed: {0.__class__.__name__}: {0}'.format(exc)) from exc
    
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
