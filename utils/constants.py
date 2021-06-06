from re import compile


_image_files = ("bmp", "gif", "ico", "png", "psd", "svg", "jpg")
_audio_files = ("mid", "midi", "mp3", "ogg", "wav", "wma")
_video_files = ("avi", "mp4", "wmv", "m4v", "mpg", "mpeg")
_document_files = ("doc", "docx", "txt", "odt", "rtf")
_compressed_files = ("zip", "7z", "rar", "gz", "xz")
_script_files = ("bat", "sh", "jar", "py", "php")
_program_files = ("apk", "exe", "msi", "deb")
_disk_images = ("dmg", "iso", "img", "ima")

ANIMATED_EMOJI_REGEX = compile(r'<a\:.+:\d+>')
EMOJI_REGEX = compile(r'<:.+:\d+\d+>')

READABLE_TYPE = {
    _compressed_files: "a compressed file",
    _document_files: "a documment file",
    _script_files: "a script file",
    _audio_files: "an audio file",
    _image_files: "an image file",
    _disk_images: "a disk image",
    _video_files: "a video file",
    _program_files: "a program",
}

FOOTER_MSGS = (
    "If you find a bug or want to ask a question, join the support server: discord.gg/zWPWwQC",
    "If you want to support the development and hosting of TTS Bot, check out -donate!",
    "You can vote for me or review me on top.gg!\nhttps://top.gg/bot/513423712582762502",
    "There are loads of customizable settings, check out -settings help",
)

GTTS_ESPEAK_DICT = {
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
