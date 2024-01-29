// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{borrow::Cow, collections::BTreeMap, fmt::Write};

use itertools::Itertools as _;
use rand::Rng as _;

use poise::serenity_prelude as serenity;
use serenity::{
    builder::*,
    small_fixed_array::{FixedString, TruncatingInto},
};

use crate::{
    database::GuildRow,
    opt_ext::{OptionGettext, OptionTryUnwrap},
    require,
    structs::{
        Context, Data, GoogleGender, GoogleVoice, LastToXsaidTracker, RegexCache, Result, TTSMode,
        TTSServiceError,
    },
};

pub async fn remove_premium(data: &Data, guild_id: serenity::GuildId) -> Result<()> {
    data.guilds_db
        .set_one(guild_id.into(), "premium_user", None::<i64>)
        .await?;
    data.guilds_db
        .set_one(guild_id.into(), "voice_mode", TTSMode::default())
        .await
}

pub async fn dm_generic<'ctx, 'a>(
    ctx: &'ctx serenity::Context,
    author: &serenity::User,
    target: serenity::UserId,
    mut target_tag: String,
    attachment_url: Option<impl Into<Cow<'a, str>>>,
    message: String,
) -> Result<(String, serenity::Embed)> {
    let dm_channel = target.create_dm_channel(ctx).await?;
    let sent = dm_channel
        .send_message(
            &ctx.http,
            CreateMessage::default().embed({
                let mut embed = CreateEmbed::default();
                if let Some(url) = attachment_url {
                    embed = embed.image(url);
                };

                embed
                    .title("Message from the developers:")
                    .description(message)
                    .author(CreateEmbedAuthor::new(author.tag()).icon_url(author.face()))
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
) -> reqwest::Url {
    tts_service.set_path("tts");
    tts_service
        .query_pairs_mut()
        .append_pair("text", content)
        .append_pair("lang", lang)
        .append_pair("mode", mode.into())
        .append_pair("max_length", max_length)
        .append_pair("preferred_format", "mp3")
        .append_pair("speaking_rate", speaking_rate)
        .finish();
    tts_service
}

pub async fn get_translation_langs(
    reqwest: &reqwest::Client,
    url: Option<&reqwest::Url>,
    token: Option<&str>,
) -> Result<BTreeMap<FixedString, FixedString>> {
    #[derive(serde::Deserialize)]
    pub struct DeeplVoice {
        pub name: FixedString,
        pub language: String,
    }

    #[derive(serde::Serialize)]
    struct DeeplVoiceRequest {
        #[serde(rename = "type")]
        kind: &'static str,
    }

    let (Some(url), Some(token)) = (url, token) else {
        return Ok(BTreeMap::new());
    };

    let languages: Vec<DeeplVoice> = reqwest
        .get(format!("{url}/languages"))
        .query(&DeeplVoiceRequest { kind: "target" })
        .header("Authorization", format!("DeepL-Auth-Key {token}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(languages
        .into_iter()
        .map(|v| (v.language.to_lowercase().trunc_into(), v.name))
        .collect())
}

pub fn prepare_gcloud_voices(
    raw_map: Vec<GoogleVoice>,
) -> BTreeMap<FixedString, BTreeMap<FixedString, GoogleGender>> {
    // {lang_accent: {variant: gender}}
    let mut cleaned_map = BTreeMap::new();
    for gvoice in raw_map {
        let variant = gvoice
            .name
            .splitn(3, '-')
            .nth(2)
            .and_then(|mode_variant| mode_variant.split_once('-'))
            .filter(|(mode, _)| *mode == "Standard")
            .map(|(_, variant)| variant);

        if let Some(variant) = variant {
            let [language] = gvoice.language_codes;
            cleaned_map
                .entry(language)
                .or_insert_with(BTreeMap::new)
                .insert(FixedString::from_str_trunc(variant), gvoice.ssml_gender);
        }
    }

    cleaned_map
}

pub fn random_footer<'a>(
    server_invite: &str,
    client_id: serenity::UserId,
    catalog: Option<&'a gettext::Catalog>,
) -> Cow<'a, str> {
    match rand::thread_rng().gen_range(0..4) {
        0 => Cow::Owned(catalog.gettext("If you find a bug or want to ask a question, join the support server: {server_invite}").replace("{server_invite}", server_invite)),
        1 => Cow::Owned(catalog.gettext("You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{client_id}").replace("{client_id}", &client_id.to_string())),
        2 => Cow::Borrowed(catalog.gettext("If you want to support the development and hosting of TTS Bot, check out `/premium`!")),
        3 => Cow::Borrowed(catalog.gettext("There are loads of customizable settings, check out `/help set`")),
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

fn remove_repeated_chars(content: &str, limit: usize) -> String {
    content
        .chars()
        .group_by(|&c| c)
        .into_iter()
        .map(|(key, group)| {
            let group: String = group.collect();
            if group.chars().count() > limit {
                key.to_string().repeat(limit)
            } else {
                group
            }
        })
        .collect()
}

pub async fn run_checks(
    ctx: &serenity::Context,
    message: &serenity::Message,
    guild_row: &GuildRow,
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

    if let Some(required_role) = guild_row.required_role {
        let message_member = require!(message.member.as_ref(), Ok(None));
        if !message_member.roles.contains(&required_role) {
            let member = guild_id.member(ctx, message.author.id).await?;
            let channel = require!(message.channel_id.to_channel(ctx).await?.guild(), Ok(None));

            let author_permissions = require!(message.guild(&ctx.cache), Ok(None))
                .user_permissions_in(&channel, &member);
            if !author_permissions.administrator() {
                return Ok(None);
            }
        }
    }

    let mut content = {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            return Ok(None);
        };

        serenity::content_safe(
            &guild,
            &message.content,
            serenity::ContentSafeOptions::default()
                .clean_here(false)
                .clean_everyone(false)
                .show_discriminator(false),
            &message.mentions,
        )
    };

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

    let guild = require!(message.guild(&ctx.cache), Ok(None));
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
                && voice_state.map_or(false, serenity::VoiceState::suppress)
                && guild_row.audience_ignore()
            {
                return Ok(None); // Is audience
            }
        }
    }

    let mut removed_chars_content = content.clone();
    removed_chars_content.retain(|c| !" ?.)'!\":".contains(c));
    if removed_chars_content.is_empty() {
        return Ok(None);
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
    repeated_limit: usize,
    nickname: Option<&str>,

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

    // If xsaid is enabled, and the author has not been announced last (in one minute if more than 2 users in vc)
    let last_to_xsaid = last_to_xsaid_tracker.get(&guild_id);

    if xsaid
        && match last_to_xsaid.map(|i| *i) {
            None => true,
            Some((u_id, last_time)) => cache
                .guild(guild_id)
                .map(|guild| {
                    guild
                        .voice_states
                        .get(&user.id)
                        .and_then(|vs| vs.channel_id)
                        .map_or(true, |voice_channel_id| {
                            (user.id != u_id)
                                || ((last_time.elapsed().unwrap().as_secs() > 60) &&
                    // If more than 2 users in vc
                    guild.voice_states.values()
                        .filter(|vs| vs.channel_id.map_or(false, |vc| vc == voice_channel_id))
                        .filter_map(|vs| guild.members.get(&vs.user_id))
                        .filter(|member| !member.user.bot())
                        .count() > 2)
                        })
                })
                .unwrap(),
        }
    {
        if contained_url {
            write!(
                content,
                " {}",
                if content.is_empty() {
                    "a link."
                } else {
                    "and sent a link"
                }
            )
            .unwrap();
        }

        let said_name = nickname
            .or(member_nick)
            .or(user.global_name.as_deref())
            .unwrap_or(&user.name);

        content = match attachments_to_format(attachments) {
            Some(file_format) if content.is_empty() => format!("{said_name} sent {file_format}"),
            Some(file_format) => format!("{said_name} sent {file_format} and said {content}"),
            None => format!("{said_name} said: {content}"),
        }
    } else if contained_url {
        write!(
            content,
            "{}",
            if content.is_empty() {
                "a link."
            } else {
                ". This message contained a link"
            }
        )
        .unwrap();
    }

    if xsaid {
        last_to_xsaid_tracker.insert(guild_id, (user.id, std::time::SystemTime::now()));
    }

    if repeated_limit != 0 {
        content = remove_repeated_chars(&content, repeated_limit);
    }

    content
}

pub async fn translate(
    reqwest: &reqwest::Client,
    translation_url: &reqwest::Url,
    translation_token: &str,
    content: &str,
    target_lang: &str,
) -> Result<Option<String>> {
    #[derive(serde::Deserialize)]
    pub struct DeeplTranslateResponse {
        pub translations: Vec<DeeplTranslation>,
    }

    #[derive(serde::Deserialize)]
    pub struct DeeplTranslation {
        pub text: String,
        pub detected_source_language: String,
    }

    #[derive(serde::Serialize)]
    struct DeeplTranslateRequest<'a> {
        text: &'a str,
        target_lang: &'a str,
        preserve_formatting: u8,
    }

    let request = DeeplTranslateRequest {
        target_lang,
        text: content,
        preserve_formatting: 1,
    };

    let response: DeeplTranslateResponse = reqwest
        .get(format!("{translation_url}/translate"))
        .query(&request)
        .header(
            "Authorization",
            format!("DeepL-Auth-Key {translation_token}"),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(translation) = response.translations.into_iter().next() {
        if translation.detected_source_language != target_lang {
            return Ok(Some(translation.text));
        }
    }

    Ok(None)
}

pub fn confirm_dialog_components<'ctx>(
    positive: impl Into<Cow<'ctx, str>>,
    negative: impl Into<Cow<'ctx, str>>,
) -> Cow<'ctx, [CreateActionRow<'ctx>]> {
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

pub async fn confirm_dialog<'ctx>(
    ctx: Context<'ctx>,
    prompt: &'ctx str,
    positive: impl Into<Cow<'ctx, str>>,
    negative: impl Into<Cow<'ctx, str>>,
) -> Result<Option<bool>> {
    let builder = poise::CreateReply::default()
        .content(prompt)
        .ephemeral(true)
        .components(confirm_dialog_components(positive, negative));

    let reply = ctx.send(builder).await?;
    let message = reply.message().await?;

    confirm_dialog_wait(ctx.serenity_context(), &message, ctx.author().id).await
}
