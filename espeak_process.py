from io import BytesIO
from functools import wraps

from pydub import AudioSegment
from voxpopuli import Voice

from utils.basic import gtts_to_espeak


def make_espeak(text, lang, max_length):
    voice = Voice(lang=gtts_to_espeak[lang], speed=130, volume=2) if lang in gtts_to_espeak else Voice(lang="en",speed=130)
    wav = voice.to_audio(text)

    pydub_wav = AudioSegment.from_file_using_temporary_files(BytesIO(wav))
    audio_length = len(pydub_wav)/1000

    return wav, audio_length

if __name__ == "__main__":
    print("Not for running directly, this file handles making espeak")
