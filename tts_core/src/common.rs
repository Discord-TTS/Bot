use std::borrow::Cow;
use std::num::NonZeroU8;

use itertools::Itertools;
use rand::Rng as _;

use serenity::all as serenity;
use serenity::{CreateActionRow, CreateButton};
use to_arraystring::ToArrayString as _;

use crate::database::{GuildRow, UserRow};
use crate::opt_ext::OptionTryUnwrap as _;
use crate::require;
use crate::structs::{
    Context, Data, LastToXsaidTracker, LastXsaidInfo, RegexCache, Result, TTSMode, TTSServiceError,
};
use crate::translations::OptionGettext as _;

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

pub fn random_footer<'a>(
    server_invite: &str,
    client_id: serenity::UserId,
    catalog: Option<&'a gettext::Catalog>,
) -> Cow<'a, str> {
    let client_id_str = client_id.get().to_arraystring();
    match rand::thread_rng().gen_range(0..5) {
        0 => Cow::Owned(catalog.gettext("If you find a bug or want to ask a question, join the support server: {server_invite}").replace("{server_invite}", server_invite)),
        1 => Cow::Owned(catalog.gettext("You can vote for me or review me on wumpus.store!\nhttps://wumpus.store/bot/{client_id}?ref=tts").replace("{client_id}", &client_id_str)),
        2 => Cow::Owned(catalog.gettext("You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{client_id}").replace("{client_id}", &client_id_str)),
        3 => Cow::Borrowed(catalog.gettext("If you want to support the development and hosting of TTS Bot, check out `/premium`!")),
        4 => Cow::Borrowed(catalog.gettext("There are loads of customizable settings, check out `/help set`")),
        _ => unreachable!()
    }
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

pub async fn run_checks(
    ctx: &serenity::Context,
    message: &serenity::Message,
    guild_row: &GuildRow,
    user_row: &UserRow,
) -> Result<Option<(String, Option<serenity::ChannelId>)>> {
    let guild_id = require!(message.guild_id, Ok(None));
    if guild_row.channel != Some(message.channel_id) {
        // "Text in Voice" works by just sending messages in voice channels, so checking for it just takes
        // checking if the message's channel_id is the author's voice channel_id
        let guild = require!(message.guild(&ctx.cache), Ok(None));
        let author_vc = guild
            .voice_states
            .get(&message.author.id)
            .and_then(|c| c.channel_id);

        if author_vc.map_or(true, |author_vc| author_vc != message.channel_id) {
            return Ok(None);
        }
    }

    if user_row.bot_banned() {
        return Ok(None);
    }

    if let Some(required_role) = guild_row.required_role {
        let message_member = require!(message.member.as_ref(), Ok(None));
        if !message_member.roles.contains(&required_role) {
            let member = guild_id.member(ctx, message.author.id).await?;

            let guild = require!(message.guild(&ctx.cache), Ok(None));
            let channel = require!(guild.channels.get(&message.channel_id), Ok(None));

            let author_permissions = guild.user_permissions_in(channel, &member);
            if !author_permissions.administrator() {
                return Ok(None);
            }
        }
    }

    let guild = require!(message.guild(&ctx.cache), Ok(None));
    let mut content = serenity::content_safe(
        &guild,
        &message.content,
        serenity::ContentSafeOptions::default()
            .clean_here(false)
            .clean_everyone(false)
            .show_discriminator(false),
        &message.mentions,
    );

    if content.len() >= 1500 {
        return Ok(None);
    }

    content = content.to_lowercase();

    if let Some(required_prefix) = &guild_row.required_prefix {
        if let Some(stripped_content) = content.strip_prefix(required_prefix.as_str()) {
            content = String::from(stripped_content);
        } else {
            return Ok(None);
        }
    }

    if content.starts_with(guild_row.prefix.as_str()) {
        return Ok(None);
    }

    let voice_state = guild.voice_states.get(&message.author.id);
    let bot_voice_state = guild.voice_states.get(&ctx.cache.current_user().id);

    let mut to_autojoin = None;
    if message.author.bot() {
        if guild_row.bot_ignore() || bot_voice_state.is_none() {
            return Ok(None); // Is bot
        }
    } else {
        // If the bot is in vc
        if let Some(vc) = bot_voice_state {
            // If the user needs to be in the vc, and the user's voice channel is not the same as the bot's
            if guild_row.require_voice()
                && vc.channel_id != voice_state.and_then(|vs| vs.channel_id)
            {
                return Ok(None); // Wrong vc
            }
        // Else if the user is in the vc and autojoin is on
        } else if let Some(voice_state) = voice_state
            && guild_row.auto_join()
        {
            to_autojoin = Some(voice_state.channel_id.try_unwrap()?);
        } else {
            return Ok(None); // Bot not in vc
        };

        if guild_row.require_voice() {
            let voice_channel = voice_state.unwrap().channel_id.try_unwrap()?;
            let channel = guild.channels.get(&voice_channel).try_unwrap()?;

            if channel.kind == serenity::ChannelType::Stage
                && voice_state.is_some_and(serenity::VoiceState::suppress)
                && guild_row.audience_ignore()
            {
                return Ok(None); // Is audience
            }
        }
    }

    Ok(Some((content, to_autojoin)))
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
    repeated_limit: Option<NonZeroU8>,
    nickname: Option<&str>,
    use_new_formatting: bool,

    regex_cache: &RegexCache,
    last_to_xsaid_tracker: &LastToXsaidTracker,
) -> String {
    let (contained_url, mut content) = if content == "?" {
        (false, String::from("what"))
    } else {
        let mut content: String = regex_cache
            .emoji
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
            .into_owned();

        for (regex, replacement) in &regex_cache.replacements {
            content = regex.replace_all(&content, *replacement).into_owned();
        }

        if voice.starts_with("en") {
            content = parse_acronyms(&content);
        }

        let filtered_content: String = linkify::LinkFinder::new()
            .spans(&content)
            .filter(|span| span.kind().is_none())
            .map(|span| span.as_str())
            .collect();

        (content != filtered_content, filtered_content)
    };

    let announce_name = xsaid
        && last_to_xsaid_tracker.get(&guild_id).map_or(true, |state| {
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
