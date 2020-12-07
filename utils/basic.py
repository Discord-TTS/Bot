import os
from re import compile

image_files = ("bmp", "gif", "ico", "png", "psd", "svg", "jpg")
audio_files = ("mid", "midi", "mp3", "ogg", "wav", "wma")
video_files = ("avi", "mp4", "wmv", "m4v", "mpg", "mpeg")
document_files = ("doc", "docx", "txt", "odt", "rtf")
compressed_files = ("zip", "7z", "rar", "gz", "xz")
script_files = ("bat", "sh", "jar", "py", "php")
program_files = ("apk", "exe", "msi", "deb")
disk_images = ("dmg", "iso", "img", "ima")

full_dict = {
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

async def ensure_webhook(channel, name="TTS-Webhook"):
    webhooks = await channel.webhooks()
    if len(webhooks) == 0:  webhook = await channel.create_webhook(name)
    else:   webhook = webhooks[0]

    return webhook

def get_value(dictionary, *nested_values, default_value = None):
    try:
        for value in nested_values:
            dictionary = dictionary[value]
    except (TypeError, AttributeError, KeyError):
        return default_value

    return dictionary

def remove_chars(remove_from, *chars):
    input_string = str(remove_from)
    for char in chars:  input_string = input_string.replace(char, "")

    return input_string

def get_size(start_path='.'):
    total_size = 0
    for dirpath, dirnames, filenames in os.walk(start_path):
        for f in filenames:
            fp = os.path.join(dirpath, f)
            if not os.path.islink(fp):
                total_size += os.path.getsize(fp)

    return total_size

def sort_dict(dict_to_sort):
    keys = list(dict_to_sort.keys())
    keys.sort()
    newdict = {}
    for x in keys:
        newdict[x] = dict_to_sort[x]

    return newdict

def emojitoword(text):
    emojiAniRegex = compile(r'<a\:.+:\d+>')
    emojiRegex = compile(r'<:.+:\d+\d+>')
    words = text.split(' ')
    output = []

    for a in words:
        for x in a.split('><'):
            x = f'<{remove_chars(x, "<", ">")}>'
            if emojiAniRegex.match(x):
                output.append(f"animated emoji {x.split(':')[1]}")
            elif emojiRegex.match(x):
                output.append(f"emoji {x.split(':')[1]}")
            else:
                output.append(x)

    return ' '.join([str(x) for x in output])

def exts_to_format(attachments):
    if len(attachments) >= 2:   return "multiple files"
    if len(attachments) == 0:   return False

    ext = attachments[0].filename.split(".")[-1]
    returned_format = False

    for file_exts, format in full_dict.items():
        if ext in file_exts:
            returned_format = format
            break

    if not returned_format: returned_format = "a file"
    return returned_format
