import os
from re import compile
from typing import Optional

image_files = ("bmp", "gif", "ico", "png", "psd", "svg", "jpg")
audio_files = ("mid", "midi", "mp3", "ogg", "wav", "wma")
video_files = ("avi", "mp4", "wmv", "m4v", "mpg", "mpeg")
document_files = ("doc", "docx", "txt", "odt", "rtf")
compressed_files = ("zip", "7z", "rar", "gz", "xz")
script_files = ("bat", "sh", "jar", "py", "php")
program_files = ("apk", "exe", "msi", "deb")
disk_images = ("dmg", "iso", "img", "ima")

types_to_explaination = {
    compressed_files: "a compressed file",
    document_files: "a documment file",
    script_files: "a script file",
    audio_files: "an audio file",
    image_files: "an image file",
    disk_images: "a disk image",
    video_files: "a video file",
    program_files: "a program",
}

footer_messages = (
    "If you find a bug or want to ask a question, join the support server: discord.gg/zWPWwQC",
    "If you want to support the development and hosting of TTS Bot, check out -donate!",
    "You can vote for me or review me on top.gg!\nhttps://top.gg/bot/513423712582762502",
    "There are loads of customizable settings, check out -settings help",
)

gtts_to_espeak = {
    "af":"af",
    "ar":"ar",
    "cs":"cz",
    "de":"de",
    "en":"en",
    "el":"gr",
    "es":"es",
    "et":"ee",
    "fr":"fr",
    "hi":"in",
    "hr":"cr",
    "hu":"hu",
    "id":"id",
    "is":"ic",
    "it":"it",
    "ja":"jp",
    "ko":"hn",
    "la":"la",
    "nl":"nl",
    "pl":"pl",
    "pt":"pt",
    "ro":"ro",
    "sw":"sw",
    "te":"tl",
    "tr":"tr",
    "uk":"en",
    "en-us":"us",
    "en-ca":"us",
    "en-uk":"en",
    "en-gb":"en",
    "fr-ca":"fr",
    "fr-fr":"fr",
    "es-es":"es",
    "es-us":"es"
    }


def remove_chars(remove_from, *chars) -> str:
    input_string = str(remove_from)
    for char in chars:
        input_string = input_string.replace(char, "")

    return input_string


def get_size(start_path: str = '.') -> int:
    total_size = 0
    for dirpath, dirnames, filenames in os.walk(start_path):
        for f in filenames:
            fp = os.path.join(dirpath, f)
            if not os.path.islink(fp):
                total_size += os.path.getsize(fp)

    return total_size


def emojitoword(text: str) -> str:
    emojiAniRegex = compile(r'<a\:.+:\d+>')
    emojiRegex = compile(r'<:.+:\d+\d+>')
    words = text.split(' ')
    output = []

    for x in words:
        if emojiAniRegex.match(x):
            output.append(f"animated emoji {x.split(':')[1]}")
        elif emojiRegex.match(x):
            output.append(f"emoji {x.split(':')[1]}")
        else:
            output.append(x)

    return ' '.join(str(x) for x in output)


def exts_to_format(attachments) -> Optional[str]:
    if not attachments:
        return None

    if len(attachments) >= 2:
        return "multiple files"

    ext = attachments[0].filename.split(".")[-1]

    returned_format_gen = (file_type for exts, file_type in types_to_explaination.items() if ext in exts)
    return next(returned_format_gen, "a file")
