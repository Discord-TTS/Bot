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
    content: &mut String,
    said_name: Option<&str>,
    contained_url: bool,
    attached_file_format: Option<&str>,
) {
    match (
        said_name,
        content.trim(),
        contained_url,
        attached_file_format,
    ) {
        (Some(said_name), "", true, Some(format)) => {
            *content = format!("{said_name} sent a link and attached {format}");
        }
        (Some(said_name), "", true, None) => {
            *content = format!("{said_name} sent a link");
        }
        (Some(said_name), "", false, Some(format)) => {
            *content = format!("{said_name} sent {format}");
        }
        // Fallback, this shouldn't occur
        (Some(said_name), "", false, None) => {
            *content = format!("{said_name} sent a message");
        }
        (Some(said_name), msg, true, Some(format)) => {
            *content = format!("{said_name} sent a link, attached {format}, and said {msg}");
        }
        (Some(said_name), msg, true, None) => {
            *content = format!("{said_name} sent a link and said {msg}");
        }
        (Some(said_name), msg, false, Some(format)) => {
            *content = format!("{said_name} sent {format} and said {msg}");
        }
        (Some(said_name), msg, false, None) => {
            *content = format!("{said_name} said: {msg}");
        }
        (None, "", true, Some(format)) => {
            *content = format!("A link and {format}");
        }
        (None, "", true, None) => {
            "A link".clone_into(content);
        }
        (None, "", false, Some(format)) => {
            format.clone_into(content);
        }
        // Again, fallback, there is nothing to say
        (None, "", false, None) => {}
        (None, msg, true, Some(format)) => {
            *content = format!("{msg} with {format} and a link");
        }
        (None, msg, true, None) => {
            *content = format!("{msg} with a link");
        }
        (None, msg, false, Some(format)) => {
            *content = format!("{msg} with {format}");
        }
        (None, _msg, false, None) => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub fn clean(
    content: &str,

    user: &serenity::User,
    cache: &serenity::Cache,
    guild_id: serenity::GuildId,
    member_nick: Option<&str>,
    attachments: &[serenity::Attachment],

    voice: &str,
    xsaid: bool,
    skip_emoji: bool,
    repeated_limit: Option<NonZeroU8>,
    nickname: Option<&str>,

    regex_cache: &RegexCache,
    last_to_xsaid_tracker: &LastToXsaidTracker,
) -> String {
    let (contained_url, mut content) = if content == "?" {
        (false, String::from("what"))
    } else {
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

        if voice.starts_with("en") {
            content = Cow::Owned(parse_acronyms(&content));
        }

        let filtered_content: String = linkify::LinkFinder::new()
            .spans(&content)
            .filter(|span| span.kind().is_none())
            .map(|span| span.as_str())
            .collect();

        (content != filtered_content, filtered_content)
    };

    let announce_name = xsaid
        && last_to_xsaid_tracker.get(&guild_id).is_none_or(|state| {
            let guild = cache.guild(guild_id).unwrap();
            state.should_announce_name(&guild, user.id)
        });

    let attached_file_format = attachments_to_format(attachments);
    let said_name = announce_name.then(|| {
        nickname
            .or(member_nick)
            .or(user.global_name.as_deref())
            .unwrap_or(&user.name)
    });

    format_message(&mut content, said_name, contained_url, attached_file_format);

    if xsaid {
        last_to_xsaid_tracker.insert(guild_id, LastXsaidInfo::new(user.id));
    }

    if let Some(repeated_limit) = repeated_limit {
        content = remove_repeated_chars(&content, repeated_limit.get());
    }

    content
}
