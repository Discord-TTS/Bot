"Useful functions used throughout the bot"

import os
import sys
from typing import Optional, Sequence

import discord

from utils.constants import ANIMATED_EMOJI_REGEX, EMOJI_REGEX, READABLE_TYPE


def get_size(start_path: str = ".") -> int:
    "Gets the recursive size of a directory"
    total_size = 0
    for dirpath, _, filenames in os.walk(start_path):
        for file in filenames:
            file_path = os.path.join(dirpath, file)
            total_size += os.path.getsize(file_path)

    return total_size

def emojitoword(text: str) -> str:
    "Replaces discord emojis with an alternates that can be spoken"
    output = []
    words = text.split(" ")

    for word in words:
        if EMOJI_REGEX.match(word):
            output.append(f"emoji {word.split(':')[1]}")
        elif ANIMATED_EMOJI_REGEX.match(word):
            output.append(f"animated emoji {word.split(':')[1]}")
        else:
            output.append(word)

    return " ".join(output)

def exts_to_format(attachments: Sequence[discord.Attachment]) -> Optional[str]:
    "Returns a description of the given attachment(s)"
    if not attachments:
        return None

    if len(attachments) >= 2:
        return "multiple files"

    ext = attachments[0].filename.split(".")[-1]
    returned_format_gen = (file_type for exts, file_type in READABLE_TYPE.items() if ext in exts)

    return next(returned_format_gen, "a file")

if sys.version_info >= (3, 9):
    removeprefix = str.removeprefix
else:
    def removeprefix(self: str, __prefix: str) -> str:
        "str.removeprefix but for older python versions"
        return self[len(__prefix):] if self.startswith(__prefix) else self
