import os
from typing import List, Optional

import discord

from utils.constants import *


def remove_chars(remove_from, *chars) -> str:
    input_string = str(remove_from)
    for char in chars:
        input_string = input_string.replace(char, "")

    return input_string


def get_size(start_path: str = '.') -> int:
    total_size = 0
    for dirpath, _, filenames in os.walk(start_path):
        for f in filenames:
            fp = os.path.join(dirpath, f)
            total_size += os.path.getsize(fp)

    return total_size


def emojitoword(text: str) -> str:
    output = list()
    words = text.split(" ")

    for word in words:
        if EMOJI_REGEX.match(word):
            output.append(f"emoji {word.split(':')[1]}")
        elif ANIMATED_EMOJI_REGEX.match(word):
            output.append(f"animated emoji {word.split(':')[1]}")
        else:
            output.append(word)

    return " ".join(output)


def exts_to_format(attachments: List[discord.Attachment]) -> Optional[str]:
    if not attachments:
        return None

    if len(attachments) >= 2:
        return "multiple files"

    ext = attachments[0].filename.split(".")[-1]
    returned_format_gen = (file_type for exts, file_type in READABLE_TYPE.items() if ext in exts)

    return next(returned_format_gen, "a file")
