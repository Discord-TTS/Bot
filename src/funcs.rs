// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::borrow::Cow;
use std::fmt::Write;
use std::collections::{BTreeMap, HashMap};

use itertools::Itertools as _;
use rand::Rng as _;
use regex::{Captures, Regex};
use lazy_static::lazy_static;

use poise::serenity_prelude as serenity;
use gnomeutils::{OptionGettext, OptionTryUnwrap};

use crate::structs::{Context, Data, Error, LastToXsaidTracker, TTSMode, GoogleGender, GoogleVoice, Result, TTSServiceError};
use crate::require;

pub fn generate_status(shards: &HashMap<serenity::ShardId, serenity::ShardRunnerInfo>) -> String {
    let mut status: Vec<_> = shards.iter()
        .map(|(id, shard)| (id, format!("Shard {id}: `{}`", shard.stage)))
        .collect();

    status.sort_unstable_by_key(|(id, _)| *id);
    status.into_iter().map(|(_, status)| status).join("\n")
}


pub async fn fetch_audio(reqwest: &reqwest::Client, url: reqwest::Url, auth_key: Option<&str>) -> Result<Option<reqwest::Response>> {
    let resp = reqwest
        .get(url)
        .header("Authorization", auth_key.unwrap_or(""))
        .send().await?;

    match resp.error_for_status_ref() {
        Ok(_) => Ok(Some(resp)),
        Err(backup_err) => {
            match resp.json::<TTSServiceError>().await {
                Ok(err) => {
                    if err.code.should_ignore() {
                        Ok(None)
                    } else {
                        Err(anyhow::anyhow!("Error fetching audio: {}", err.display))
                    }
                }
                Err(_) => Err(backup_err.into())
            }
        }
    }
}

pub fn prepare_url(mut tts_service: reqwest::Url, content: &str, lang: &str, mode: TTSMode, speaking_rate: &str, max_length: &str) -> reqwest::Url {
    tts_service.set_path("tts");
    tts_service.query_pairs_mut()
        .append_pair("text", content)
        .append_pair("lang", lang)
        .append_pair("mode", mode.into())
        .append_pair("max_length", max_length)
        .append_pair("speaking_rate", speaking_rate)
        .finish();
    tts_service
}


pub fn prepare_gcloud_voices(raw_map: Vec<GoogleVoice>) -> BTreeMap<String, BTreeMap<String, GoogleGender>> {
    // {lang_accent: {variant: gender}}
    let mut cleaned_map = BTreeMap::new();
    for gvoice in raw_map {
        let variant = gvoice.name
            .splitn(3, '-').nth(2)
            .and_then(|mode_variant| {mode_variant.split_once('-')})
            .filter(|(mode, _)| *mode == "Standard")
            .map(|(_, variant)| variant);

        if let Some(variant) = variant {
            let [language] = gvoice.languageCodes;
            cleaned_map
                .entry(language)
                .or_insert_with(BTreeMap::new)
                .insert(String::from(variant), gvoice.ssmlGender);
        }
    }

    cleaned_map
}

pub fn random_footer<'a>(server_invite: &str, client_id: serenity::UserId, catalog: Option<&'a gettext::Catalog>) -> Cow<'a, str> {
    match rand::thread_rng().gen_range(0..4) {
        0 => Cow::Owned(catalog.gettext("If you find a bug or want to ask a question, join the support server: {server_invite}").replace("{server_invite}", server_invite)),
        1 => Cow::Owned(catalog.gettext("You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{client_id}").replace("{client_id}", &client_id.to_string())),
        2 => Cow::Borrowed(catalog.gettext("If you want to support the development and hosting of TTS Bot, check out `/premium`!")),
        3 => Cow::Borrowed(catalog.gettext("There are loads of customizable settings, check out `/help set`")),
        _ => unreachable!()
    }
}

fn parse_acronyms(original: &str) -> String {
    original.split(' ').map(|word|
        match word {
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
        }
    ).join(" ")
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
    content.chars().group_by(|&c| c).into_iter().map(|(key, group)| {
        let group: String = group.collect();
        if group.len() > limit {
            key.to_string().repeat(limit)
        } else {
            group
        }
    }).collect()
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn run_checks(
    ctx: &serenity::Context,
    message: &serenity::Message,

    channel: u64,
    prefix: &str,
    autojoin: bool,
    bot_ignore: bool,
    require_voice: bool,
    audience_ignore: bool,
) -> Result<Option<(String, Option<serenity::ChannelId>)>> {
    let guild_id = require!(message.guild_id, Ok(None));
    if channel as u64 != message.channel_id.get() {
        // "Text in Voice" works by just sending messages in voice channels, so checking for it just takes
        // checking if the message's channel_id is the author's voice channel_id
        let guild = require!(message.guild(&ctx.cache), Ok(None));
        let author_vc = guild.voice_states.get(&message.author.id).and_then(|c| c.channel_id);

        if author_vc.map_or(true, |author_vc| author_vc != message.channel_id) {
            return Ok(None)
        }
    }

    let mut content = serenity::content_safe(&ctx.cache, &message.content,
        &serenity::ContentSafeOptions::default()
            .clean_here(false)
            .clean_everyone(false)
            .show_discriminator(false)
            .display_as_member_from(&guild_id),
        &message.mentions
    );

    if content.len() >= 1500 {
        return Ok(None);
    }

    content = content.to_lowercase();
    content = String::from(
        content
            .strip_prefix(&format!("{}{}", &prefix, "tts"))
            .unwrap_or(&content),
    ); // remove -tts if starts with

    if content.starts_with(&prefix) {
        return Ok(None)
    }

    let guild = require!(message.guild(&ctx.cache), Ok(None));
    let voice_state = guild.voice_states.get(&message.author.id);

    let mut to_autojoin = None;
    if message.author.bot {
        if bot_ignore || voice_state.is_none() {
            return Ok(None) // Is bot
        }
    } else {
        // If the bot is in vc
        if let Some(vc) = guild.voice_states.get(&ctx.cache.current_user_id()) {
            // If the user needs to be in the vc, and the user's voice channel is not the same as the bot's
            if require_voice && vc.channel_id != voice_state.and_then(|vs| vs.channel_id) {
                return Ok(None); // Wrong vc
            }
        // Else if the user is in the vc and autojoin is on
        } else if let Some(voice_state) = voice_state && autojoin {
            to_autojoin = Some(voice_state.channel_id.try_unwrap()?);
        } else {
            return Ok(None); // Bot not in vc
        };

        if require_voice {
            let voice_channel = voice_state.unwrap().channel_id.try_unwrap()?;
            if let serenity::Channel::Guild(channel) = guild.channels.get(&voice_channel).try_unwrap()? {
                if channel.kind == serenity::ChannelType::Stage && voice_state.map_or(false, |vs| vs.suppress) && audience_ignore {
                    return Ok(None); // Is audience
                }
            }
        }
    }

    let mut removed_chars_content = content.clone();
    removed_chars_content.retain(|c| !" ?.)'!\":".contains(c));
    if removed_chars_content.is_empty() {
        return Ok(None)
    }

    Ok(Some((content, to_autojoin)))
}

#[allow(clippy::too_many_arguments)]
pub fn clean_msg(
    content: &str,

    cache: &serenity::Cache,
    member: &serenity::Member,
    attachments: &[serenity::Attachment],

    voice: &str,
    xsaid: bool,
    repeated_limit: usize,
    nickname: Option<&str>,

    last_to_xsaid_tracker: &LastToXsaidTracker
) -> String {
    let (contained_url, mut content) = if content == "?" {
        (false, String::from("what"))
    } else {
        // Regex
        lazy_static! {
            static ref EMOJI_REGEX: Regex = Regex::new(r"<(a?):(.+):\d+>").unwrap();
            static ref REGEX_REPLACEMENTS: [(Regex, &'static str); 3] = {
                [
                    (Regex::new(r"\|\|(?s:.)*?\|\|").unwrap(), ". spoiler avoided."),
                    (Regex::new(r"```(?s:.)*?```").unwrap(), ". code block."),
                    (Regex::new(r"`(?s:.)*?`").unwrap(), ". code snippet."),
                ]
            };
        }

        let mut content: String = EMOJI_REGEX.replace_all(content, |re_match: &Captures<'_>| {
            let is_animated = re_match.get(1).unwrap().as_str();
            let emoji_name = re_match.get(2).unwrap().as_str();

            let emoji_prefix = if is_animated.is_empty() {
                "emoji"
            } else {
                "animated emoji"
            };

            format!("{} {}", emoji_prefix, emoji_name)
        }).into_owned();

        for (regex, replacement) in REGEX_REPLACEMENTS.iter() {
            content = regex.replace_all(&content, *replacement).into_owned();
        };


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
    let guild_id = member.guild_id;
    let last_to_xsaid = last_to_xsaid_tracker.get(&guild_id);

    if xsaid && match last_to_xsaid.map(|i| *i) {
        None => true,
        Some((u_id, last_time)) => cache.guild(guild_id).map(|guild|
            guild.voice_states.get(&member.user.id).and_then(|vs| vs.channel_id).map_or(true, |voice_channel_id|
                (member.user.id != u_id) || ((last_time.elapsed().unwrap().as_secs() > 60) &&
                    // If more than 2 users in vc
                    guild.voice_states.values()
                        .filter(|vs| vs.channel_id.map_or(false, |vc| vc == voice_channel_id))
                        .filter_map(|vs| guild.members.get(&vs.user_id))
                        .filter(|member| !member.user.bot)
                        .count() > 2
                )
            )
        ).unwrap()
    } {
        if contained_url {
            write!(content, " {}",
                if content.is_empty() {"a link."}
                else {"and sent a link"}
            ).unwrap();
        }

        let said_name = nickname.unwrap_or_else(|| member.nick.as_ref().unwrap_or(&member.user.name));
        content = match attachments_to_format(attachments) {
            Some(file_format) if content.is_empty() => format!("{} sent {}", said_name, file_format),
            Some(file_format) => format!("{} sent {} and said {}", said_name, file_format, content),
            None => format!("{} said: {}", said_name, content),
        }
    } else if contained_url {
        write!(content, "{}",
            if content.is_empty() {"a link."}
            else {". This message contained a link"}
        ).unwrap();
    }

    if xsaid {
        last_to_xsaid_tracker.insert(member.guild_id, (member.user.id, std::time::SystemTime::now()));
    }

    if repeated_limit != 0 {
        content = remove_repeated_chars(&content, repeated_limit as usize);
    }

    content
}


pub async fn translate(content: &str, target_lang: &str, data: &Data) -> Result<Option<String>> {
    let url = format!("{}/translate", crate::constants::TRANSLATION_URL);
    let response: crate::structs::DeeplTranslateResponse = data.reqwest.get(url)
        .query(&serenity::json::prelude::json!({
            "text": content,
            "target_lang": target_lang,
            "preserve_formatting": 1u8,
            "auth_key": &data.config.translation_token.as_ref().expect("Tried to do translation without token set in config!")
        }))
        .send().await?.error_for_status()?
        .json().await?;

    if let Some(translation) = response.translations.into_iter().next() {
        if translation.detected_source_language != target_lang {
            return Ok(Some(translation.text))
        }
    }

    Ok(None)
}

pub async fn confirm_dialog(ctx: Context<'_>, prompt: &str, positive: &str, negative: &str) -> Result<Option<bool>, Error> {
    let message = ctx.send(|b| b
        .content(prompt)
        .ephemeral(true)
        .components(|c| c
            .create_action_row(|r| r
                .create_button(|b| b
                    .style(serenity::ButtonStyle::Success)
                    .custom_id("True")
                    .label(positive)
                )
                .create_button(|b| b
                    .style(serenity::ButtonStyle::Danger)
                    .custom_id("False")
                    .label(negative)
                )
            )
        )
    ).await?.message().await?;

    let ctx_discord = ctx.discord();
    let interaction = message
        .await_component_interaction(&ctx_discord.shard)
        .timeout(std::time::Duration::from_secs(60 * 5))
        .author_id(ctx.author().id)
        .collect_limit(1)
        .await;

    if let Some(interaction) = interaction {
        interaction.defer(&ctx_discord.http).await?;
        match &*interaction.data.custom_id {
            "True" => Ok(Some(true)),
            "False" => Ok(Some(false)),
            _ => unreachable!()
        }
    } else {
        Ok(None)
    }
}
