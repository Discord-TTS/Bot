import re as _re

NETURAL_COLOUR = 0xcaa652
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

_PRE_REGEX_REPLACEMENTS = {
    r"\|\|.*?\|\|": ". spoiler avoided.",
    r"```.*?```": ". code block.",
    r"`.*?`": ". code snippet.",
}
REGEX_REPLACEMENTS = {
    _re.compile(key, _re.DOTALL): value
    for key, value in _PRE_REGEX_REPLACEMENTS.items()
}

OPTION_SEPERATORS = (
    ":small_orange_diamond:",
    ":small_blue_diamond:",
    ":small_red_triangle:"
)

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
    "You can vote for me or review me on top.gg!\nhttps://top.gg/bot/513423712582762502",
    "There are loads of customizable settings, check out -settings help",
)

GUILDS_CREATE = """
    CREATE TABLE guilds (
        guild_id       bigint     PRIMARY KEY,
        channel        bigint     DEFAULT 0,
        xsaid          bool       DEFAULT True,
        bot_ignore     bool       DEFAULT True,
        auto_join      bool       DEFAULT False,
        msg_length     smallint   DEFAULT 30,
        repeated_chars smallint   DEFAULT 0,
        prefix         varchar(6) DEFAULT 'p-',
        default_lang   varchar(3)
    );"""
USERINFO_CREATE = """
    CREATE TABLE userinfo (
        user_id  bigint     PRIMARY KEY,
        blocked  bool       DEFAULT False,
        lang     varchar(5),
        variant  varchar(1)
    );"""
NICKNAMES_CREATE = """
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
    );"""
ANALYTICS_CREATE = """
    CREATE TABLE analytics (
        event          text  NOT NULL,
        count          int   NOT NULL,
        is_command     bool  NOT NULL,
        date_collected date  NOT NULL DEFAULT CURRENT_DATE,

        PRIMARY KEY (event, is_command, date_collected)
    );"""
# cyrus01337: I know nothing about foreign keys so this is as copy/paste as it gets
DONATORS_CREATE = """
    CREATE TABLE donators (
        guild_id bigint,
        user_id  bigint,

        PRIMARY KEY (guild_id, user_id),

        FOREIGN KEY         (guild_id)
        REFERENCES guilds   (guild_id)
        ON DELETE CASCADE,

        FOREIGN KEY         (user_id)
        REFERENCES userinfo (user_id)
        ON DELETE CASCADE
    );"""

DB_SETUP_QUERY = "\n".join((
    GUILDS_CREATE,
    USERINFO_CREATE,
    NICKNAMES_CREATE,
    ANALYTICS_CREATE,
    DONATORS_CREATE
))
