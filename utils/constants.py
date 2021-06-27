import re as _re

_image_files = ("bmp", "gif", "ico", "png", "psd", "svg", "jpg")
_audio_files = ("mid", "midi", "mp3", "ogg", "wav", "wma")
_video_files = ("avi", "mp4", "wmv", "m4v", "mpg", "mpeg")
_document_files = ("doc", "docx", "txt", "odt", "rtf")
_compressed_files = ("zip", "7z", "rar", "gz", "xz")
_script_files = ("bat", "sh", "jar", "py", "php")
_program_files = ("apk", "exe", "msi", "deb")
_disk_images = ("dmg", "iso", "img", "ima")

ANIMATED_EMOJI_REGEX = _re.compile(r"<a\:.+:\d+>")
EMOJI_REGEX = _re.compile(r"<:.+:\d+\d+>")

REGEX_REPLACEMENTS = {
    _re.compile(r"\|\|.*?\|\|"): ". spoiler avoided.",
    _re.compile(r"```.*?```"): ". code block.",
    _re.compile(r"`.*?`"): ". code snippet.",
}

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

DB_SETUP_QUERY = """
    CREATE TABLE guilds (
        guild_id       bigint     PRIMARY KEY,
        channel        bigint     DEFAULT 0,
        xsaid          bool       DEFAULT True,
        bot_ignore     bool       DEFAULT True,
        auto_join      bool       DEFAULT False,
        msg_length     smallint   DEFAULT 30,
        repeated_chars smallint   DEFAULT 0,
        prefix         varchar(6) DEFAULT '-',
        default_lang   varchar(3)
    );
    CREATE TABLE userinfo (
        user_id  bigint     PRIMARY KEY,
        blocked  bool       DEFAULT False,
        lang     varchar(4)
    );
    CREATE TABLE nicknames (
        guild_id bigint,
        user_id  bigint,
        name     text,

        PRIMARY KEY (guild_id, user_id),

        FOREIGN KEY       (guild_id)
        REFERENCES guilds (guild_id)
        ON DELETE CASCADE,

        FOREIGN KEY         (user_id)
        REFERENCES userinfo (user_id)
        ON DELETE CASCADE
    );
    CREATE TABLE cache_lookup (
        message    BYTEA  PRIMARY KEY,
        message_id bigint UNIQUE NOT NULL
    );"""
