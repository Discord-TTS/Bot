use std::{borrow::Cow, num::NonZeroU8};

use crate::structs::{LastToXsaidTracker, LastXsaidInfo, RegexCache};
use itertools::Itertools as _;
use poise::serenity_prelude as serenity;

fn strip_emoji<'c>(regex_cache: &RegexCache, content: &'c str) -> Cow<'c, str> {
    regex_cache.emoji_filter.replace_all(content, "")
}

fn make_emoji_readable<'c>(regex_cache: &RegexCache, content: &'c str) -> Cow<'c, str> {
    regex_cache
        .emoji_captures
        .replace_all(content, |re_match: &regex::Captures<'_>| {
            let is_animated = re_match.get(1).unwrap().as_str();
            let emoji_name = re_match.get(2).unwrap().as_str();

            let emoji_prefix = if is_animated.is_empty() {
                "emoji"
            } else {
                "animated emoji"
            };

            format!("{emoji_prefix} {emoji_name}")
        })
}

fn run_regex_replacements<'c>(
    regex_cache: &RegexCache,
    content: &'c str,
    skip_emoji: bool,
) -> Cow<'c, str> {
    let mut content = if skip_emoji {
        strip_emoji(regex_cache, content)
    } else {
        make_emoji_readable(regex_cache, content)
    };

    for (regex, replacement) in &regex_cache.replacements {
        if let Cow::Owned(replaced) = regex.replace_all(&content, *replacement) {
            content = Cow::Owned(replaced);
        }
    }

    content
}

fn parse_acronyms(original: &str) -> String {
    original
        .split(' ')
        .map(|word| match word {
            "iirc" => "if I recall correctly",
            "afaik" => "as far as I know",
            "wdym" => "what do you mean",
            "imo" => "in my opinion",
            "brb" => "be right back",
            "wym" => "what you mean",
            "irl" => "in real life",
            "jk" => "just kidding",
            "btw" => "by the way",
            ":)" => "smiley face",
            "gtg" => "got to go",
            "rn" => "right now",
            ":(" => "sad face",
            "ig" => "i guess",
            "ppl" => "people",
            "rly" => "really",
            "cya" => "see ya",
            "ik" => "i know",
            "@" => "at",
            "™️" => "tm",
            _ => word,
        })
        .join(" ")
}

fn attachments_to_format(attachments: &[serenity::Attachment]) -> Option<&'static str> {
    if attachments.len() >= 2 {
        return Some("multiple files");
    }

    let extension = attachments.first()?.filename.split('.').next_back()?;
    match extension {
        "bmp" | "gif" | "ico" | "png" | "psd" | "svg" | "jpg" => Some("an image file"),
        "mid" | "midi" | "mp3" | "ogg" | "wav" | "wma" => Some("an audio file"),
        "avi" | "mp4" | "wmv" | "m4v" | "mpg" | "mpeg" => Some("a video file"),
        "zip" | "7z" | "rar" | "gz" | "xz" => Some("a compressed file"),
        "doc" | "docx" | "txt" | "odt" | "rtf" => Some("a text file"),
        "bat" | "sh" | "jar" | "py" | "php" => Some("a script file"),
        "apk" | "exe" | "msi" | "deb" => Some("a program file"),
        "dmg" | "iso" | "img" | "ima" => Some("a disk image"),
        _ => Some("a file"),
    }
}

fn remove_repeated_chars(content: &str, limit: u8) -> String {
    let mut out = String::new();
    for (_, group) in &content.chars().chunk_by(|&c| c) {
        out.extend(group.take(usize::from(limit)));
    }

    out
}

fn format_message(
    content: &mut MessageContent<'_>,
    said_name: Option<&str>,
    contained_url: bool,
    attached_file_format: Option<&str>,
) {
    let new_content = match (
        said_name,
        content.text.trim(),
        contained_url,
        attached_file_format,
        content.kind,
    ) {
        (Some(said_name), "", true, Some(format), TTSMessageKind::Default) => {
            format!("{said_name} sent a link and attached {format}").into()
        }
        (Some(said_name), "", true, Some(format), TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message with a link and {format}").into()
        }
        (Some(said_name), "", true, None, TTSMessageKind::Default) => {
            format!("{said_name} sent a link").into()
        }
        (Some(said_name), "", true, None, TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message with a link").into()
        }
        (Some(said_name), "", false, Some(format), TTSMessageKind::Default) => {
            format!("{said_name} sent {format}").into()
        }
        (Some(said_name), "", false, Some(format), TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message with {format}").into()
        }
        // Fallback, this shouldn't occur
        (Some(said_name), "", false, None, TTSMessageKind::Default) => {
            format!("{said_name} sent a message").into()
        }
        (Some(said_name), "", false, None, TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message").into()
        }
        (Some(said_name), msg, true, Some(format), TTSMessageKind::Default) => {
            format!("{said_name} sent a link, attached {format}, and said {msg}").into()
        }
        (Some(said_name), msg, true, Some(format), TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message with a link, {format}, and that says {msg}")
                .into()
        }
        (Some(said_name), msg, true, None, TTSMessageKind::Default) => {
            format!("{said_name} sent a link and said {msg}").into()
        }
        (Some(said_name), msg, true, None, TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message with a link that says {msg}").into()
        }
        (Some(said_name), msg, false, Some(format), TTSMessageKind::Default) => {
            format!("{said_name} sent {format} and said {msg}").into()
        }
        (Some(said_name), msg, false, Some(format), TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message with {format} that says {msg}").into()
        }
        (Some(said_name), msg, false, None, TTSMessageKind::Default) => {
            format!("{said_name} said {msg}").into()
        }
        (Some(said_name), msg, false, None, TTSMessageKind::Forward) => {
            format!("{said_name} forwarded a message that says {msg}").into()
        }
        (None, "", true, Some(format), TTSMessageKind::Default) => {
            format!("a link and {format}").into()
        }
        (None, "", true, Some(format), TTSMessageKind::Forward) => {
            format!("forwarded message with a link and {format}").into()
        }
        (None, "", true, None, TTSMessageKind::Default) => Cow::Borrowed("a link"),
        (None, "", true, None, TTSMessageKind::Forward) => {
            Cow::Borrowed("forwarded message with a link")
        }
        (None, "", false, Some(format), TTSMessageKind::Default) => Cow::Borrowed(format),
        (None, "", false, Some(format), TTSMessageKind::Forward) => {
            format!("forwarded message with {format}").into()
        }
        // Again, fallback, there is nothing to say
        (None, "", false, None, TTSMessageKind::Default) => Cow::Borrowed(""),
        (None, "", false, None, TTSMessageKind::Forward) => Cow::Borrowed("forwarded message"),
        (None, msg, true, Some(format), TTSMessageKind::Default) => {
            format!("{msg} with {format} and a link").into()
        }
        (None, msg, true, Some(format), TTSMessageKind::Forward) => {
            format!("forwarded message that says {msg} with {format} and a link").into()
        }
        (None, msg, true, None, TTSMessageKind::Default) => format!("{msg} with a link").into(),
        (None, msg, true, None, TTSMessageKind::Forward) => {
            format!("forwarded message that says {msg} with a link").into()
        }
        (None, msg, false, Some(format), TTSMessageKind::Default) => {
            format!("{msg} with {format}").into()
        }
        (None, msg, false, Some(format), TTSMessageKind::Forward) => {
            format!("forwarded message that says {msg} with {format}").into()
        }
        (None, _msg, false, None, TTSMessageKind::Default) => return,
        (None, msg, false, None, TTSMessageKind::Forward) => {
            format!("forwarded message that says {msg}").into()
        }
    };

    match new_content {
        Cow::Owned(new_content) => new_content.maybe_clone_into(&mut content.text),
        Cow::Borrowed(new_content) => new_content.clone_into(&mut content.text),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TTSMessageKind {
    Forward,
    Default,
}

pub struct MessageContent<'a> {
    pub text: String,
    pub kind: TTSMessageKind,
    pub attachments: &'a [serenity::Attachment],
}

trait ToOwnedExt {
    fn maybe_clone_into(self, target: &mut Self);
}

impl ToOwnedExt for String {
    fn maybe_clone_into(self, target: &mut Self) {
        if self.capacity() > target.capacity() {
            *target = self;
        } else {
            self.clone_into(target);
        }
    }
}

#[expect(clippy::too_many_arguments)]
pub fn clean(
    content: &mut MessageContent<'_>,

    user: &serenity::User,
    cache: &serenity::Cache,
    guild_id: serenity::GuildId,
    member_nick: Option<&str>,

    voice: &str,
    xsaid: bool,
    skip_emoji: bool,
    repeated_limit: Option<NonZeroU8>,
    nickname: Option<&str>,

    regex_cache: &RegexCache,
    last_to_xsaid_tracker: &LastToXsaidTracker,
) {
    let contained_url;
    if content.text == "?" {
        "what".clone_into(&mut content.text);
        contained_url = false;
    } else {
        if let Cow::Owned(new_content) =
            run_regex_replacements(regex_cache, &content.text, skip_emoji)
        {
            new_content.maybe_clone_into(&mut content.text);
        }

        if voice.starts_with("en") {
            parse_acronyms(&content.text).maybe_clone_into(&mut content.text);
        }

        let filtered_content: String = linkify::LinkFinder::new()
            .spans(&content.text)
            .filter(|span| span.kind().is_none())
            .map(|span| span.as_str())
            .collect();

        contained_url = content.text != filtered_content;
        if contained_url {
            filtered_content.maybe_clone_into(&mut content.text);
        }
    }

    let announce_name = xsaid
        && last_to_xsaid_tracker.get(&guild_id).is_none_or(|state| {
            let guild = cache.guild(guild_id).unwrap();
            state.should_announce_name(&guild, user.id)
        });

    let attached_file_format = attachments_to_format(content.attachments);
    let said_name = announce_name.then(|| {
        nickname
            .or(member_nick)
            .or(user.global_name.as_deref())
            .unwrap_or(&user.name)
    });

    format_message(content, said_name, contained_url, attached_file_format);

    if xsaid {
        last_to_xsaid_tracker.insert(guild_id, LastXsaidInfo::new(user.id));
    }

    if let Some(repeated_limit) = repeated_limit {
        remove_repeated_chars(&content.text, repeated_limit.get())
            .maybe_clone_into(&mut content.text);
    }
}
