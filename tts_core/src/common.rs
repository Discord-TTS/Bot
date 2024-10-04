use std::borrow::Cow;
use std::num::NonZeroU8;

use itertools::Itertools;
use rand::Rng as _;

use serenity::all as serenity;
use serenity::{CreateActionRow, CreateButton};

use crate::structs::{
    Context, Data, LastToXsaidTracker, LastXsaidInfo, RegexCache, Result, TTSMode, TTSServiceError,
};

pub(crate) fn timestamp_in_future(ts: serenity::Timestamp) -> bool {
    *ts > chrono::Utc::now()
}

pub fn push_permission_names(buffer: &mut String, permissions: serenity::Permissions) {
    let permission_names = permissions.get_permission_names();
    for (i, permission) in permission_names.iter().enumerate() {
        buffer.push_str(permission);
        if i != permission_names.len() - 1 {
            buffer.push_str(", ");
        }
    }
}

pub async fn remove_premium(data: &Data, guild_id: serenity::GuildId) -> Result<()> {
    tokio::try_join!(
        data.guilds_db
            .set_one(guild_id.into(), "premium_user", None::<i64>),
        data.guilds_db
            .set_one(guild_id.into(), "voice_mode", TTSMode::default()),
    )?;

    Ok(())
}

pub async fn dm_generic<'ctx, 'a>(
    ctx: &'ctx serenity::Context,
    author: &serenity::User,
    target: serenity::UserId,
    mut target_tag: String,
    attachment_url: Option<&'a str>,
    message: &str,
) -> Result<(String, serenity::Embed)> {
    let sent = target
        .dm(
            &ctx.http,
            serenity::CreateMessage::default().embed({
                let mut embed = serenity::CreateEmbed::default();
                if let Some(url) = attachment_url {
                    embed = embed.image(url);
                };

                embed
                    .title("Message from the developers:")
                    .description(message)
                    .author(serenity::CreateEmbedAuthor::new(author.tag()).icon_url(author.face()))
            }),
        )
        .await?;

    target_tag.insert_str(0, "Sent message to: ");
    Ok((target_tag, sent.embeds.into_iter().next().unwrap()))
}

pub async fn fetch_audio(
    reqwest: &reqwest::Client,
    url: reqwest::Url,
    auth_key: Option<&str>,
) -> Result<Option<reqwest::Response>> {
    let resp = reqwest
        .get(url)
        .header(reqwest::header::AUTHORIZATION, auth_key.unwrap_or(""))
        .send()
        .await?;

    match resp.error_for_status_ref() {
        Ok(_) => Ok(Some(resp)),
        Err(backup_err) => match resp.json::<TTSServiceError>().await {
            Ok(err) => {
                if err.code.should_ignore() {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("Error fetching audio: {}", err.display))
                }
            }
            Err(_) => Err(backup_err.into()),
        },
    }
}

pub fn prepare_url(
    mut tts_service: reqwest::Url,
    content: &str,
    lang: &str,
    mode: TTSMode,
    speaking_rate: &str,
    max_length: &str,
    translation_lang: Option<&str>,
) -> reqwest::Url {
    {
        let mut params = tts_service.query_pairs_mut();
        params.append_pair("text", content);
        params.append_pair("lang", lang);
        params.append_pair("mode", mode.into());
        params.append_pair("max_length", max_length);
        params.append_pair("preferred_format", "mp3");
        params.append_pair("speaking_rate", speaking_rate);

        if let Some(translation_lang) = translation_lang {
            params.append_pair("translation_lang", translation_lang);
        }

        params.finish();
    }

    tts_service.set_path("tts");
    tts_service
}

pub fn random_footer(server_invite: &str, client_id: serenity::UserId) -> Cow<'static, str> {
    match rand::thread_rng().gen_range(0..5) {
        0 => Cow::Owned(format!("If you find a bug or want to ask a question, join the support server: {server_invite}")),
        1 => Cow::Owned(format!("You can vote for me or review me on wumpus.store!\nhttps://wumpus.store/bot/{client_id}?ref=tts")),
        2 => Cow::Owned(format!("You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{client_id}")),
        3 => Cow::Borrowed("If you want to support the development and hosting of TTS Bot, check out `/premium`!"),
        4 => Cow::Borrowed("There are loads of customizable settings, check out `/help set`"),
        _ => unreachable!()
    }
}

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

    let extension = attachments.first()?.filename.split('.').last()?;
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

#[allow(clippy::too_many_arguments)]
pub fn clean_msg(
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
    use_new_formatting: bool,

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

    if use_new_formatting {
        format_message(&mut content, said_name, contained_url, attached_file_format);
    } else {
        format_message_legacy(&mut content, said_name, contained_url, attached_file_format);
    }

    if xsaid {
        last_to_xsaid_tracker.insert(guild_id, LastXsaidInfo::new(user.id));
    }

    if let Some(repeated_limit) = repeated_limit {
        content = remove_repeated_chars(&content, repeated_limit.get());
    }

    content
}

pub fn format_message_legacy(
    content: &mut String,
    said_name: Option<&str>,
    contained_url: bool,
    attached_file_format: Option<&str>,
) {
    use std::fmt::Write;

    if let Some(said_name) = said_name {
        if contained_url {
            let suffix = if content.is_empty() {
                "a link."
            } else {
                "and sent a link"
            };

            write!(content, " {suffix}",).unwrap();
        }

        *content = match attached_file_format {
            Some(file_format) if content.is_empty() => format!("{said_name} sent {file_format}"),
            Some(file_format) => format!("{said_name} sent {file_format} and said {content}"),
            None => format!("{said_name} said: {content}"),
        }
    } else if contained_url {
        let suffix = if content.is_empty() {
            " a link."
        } else {
            ". This message contained a link"
        };

        write!(content, "{suffix}",).unwrap();
    }
}

pub fn format_message(
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

pub fn confirm_dialog_components<'a>(
    positive: &'a str,
    negative: &'a str,
) -> Cow<'a, [CreateActionRow<'a>]> {
    Cow::Owned(vec![CreateActionRow::Buttons(vec![
        CreateButton::new("True")
            .style(serenity::ButtonStyle::Success)
            .label(positive),
        CreateButton::new("False")
            .style(serenity::ButtonStyle::Danger)
            .label(negative),
    ])])
}

pub async fn confirm_dialog_wait(
    ctx: &serenity::Context,
    message: &serenity::Message,
    author_id: serenity::UserId,
) -> Result<Option<bool>> {
    let interaction = message
        .await_component_interaction(ctx.shard.clone())
        .timeout(std::time::Duration::from_secs(60 * 5))
        .author_id(author_id)
        .await;

    if let Some(interaction) = interaction {
        interaction.defer(&ctx.http).await?;
        match &*interaction.data.custom_id {
            "True" => Ok(Some(true)),
            "False" => Ok(Some(false)),
            _ => unreachable!(),
        }
    } else {
        Ok(None)
    }
}

pub async fn confirm_dialog(
    ctx: Context<'_>,
    prompt: &str,
    positive: &str,
    negative: &str,
) -> Result<Option<bool>> {
    let builder = poise::CreateReply::default()
        .content(prompt)
        .ephemeral(true)
        .components(confirm_dialog_components(positive, negative));

    let reply = ctx.send(builder).await?;
    let message = reply.message().await?;

    confirm_dialog_wait(ctx.serenity_context(), &message, ctx.author().id).await
}
