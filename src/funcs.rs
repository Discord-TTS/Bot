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
use std::collections::BTreeMap;

use itertools::Itertools;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use rand::prelude::SliceRandom;
use lavalink_rs::LavalinkClient;

use poise::serenity_prelude as serenity;
use serenity::json::prelude as json;

use crate::structs::{Data, SerenityContextAdditions, Error, LastToXsaidTracker, TTSMode, PremiumVoices, Gender, GoogleVoice, OptionTryUnwrap};
use crate::constants::{FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};

pub fn refresh_kind() -> sysinfo::RefreshKind {
    sysinfo::RefreshKind::new()
        .with_processes(sysinfo::ProcessRefreshKind::new())
        .with_memory()
        .with_cpu()
}

pub async fn generate_status(framework: &poise::Framework<Data, Error>) -> (String, bool) {
    let shard_manager_lock = framework.shard_manager();
    let shard_manager = shard_manager_lock.lock().await;
    let shards = shard_manager.runners.lock().await;

    let mut status: Vec<_> = shards
        .iter()
        .map(|(id, shard)| format!("Shard {id}: `{}`", shard.stage))
        .collect();

    status.sort_unstable();

    (status.join("\n"), shards.values().any(|shard| shard.stage.is_connecting()))
}

pub const fn default_voice(mode: TTSMode) -> &'static str {
    match mode {
        TTSMode::Gtts => "en",
        TTSMode::Espeak => "en1",
        TTSMode::Premium => "en-US A",
    }
}

pub async fn parse_user_or_guild(
    data: &Data,
    author_id: serenity::UserId,
    guild_id: Option<serenity::GuildId>,
) -> Result<(Cow<'static, str>, TTSMode), Error> {
    let user_row = data.userinfo_db.get(author_id.into()).await?;
    let mode =
        if let Some(mode) = user_row.get("voice_mode") {
            mode
        } else if let Some(guild_id) = guild_id {
            let settings = data.guilds_db.get(guild_id.into()).await?;
            settings.get("voice_mode")
        } else {
            TTSMode::Gtts
        };

    let user_voice_row = data.user_voice_db.get((author_id.into(), mode)).await?;
    let voice =
        // Get user voice for user mode
        if user_voice_row.get::<_, i64>("user_id") != 0 {
            Some(Cow::Owned(user_voice_row.get("voice")))
        } else if let Some(guild_id) = guild_id {
            // Get default server voice for user mode
            let guild_voice_row = data.guild_voice_db.get((guild_id.into(), mode)).await?;
            if guild_voice_row.get::<_, i64>("guild_id") == 0 {
                None
            } else {
                Some(Cow::Owned(guild_voice_row.get("voice")))
            }
        } else {
            None
        }.unwrap_or_else(|| Cow::Borrowed(default_voice(mode)));

    Ok((voice, mode))
}


pub async fn fetch_audio(
    reqwest: &reqwest::Client,
    tts_service: &reqwest::Url,
    content: String,
    lang: &str,
    mode: &str,
    speaking_rate: f32
) -> Result<Vec<u8>, Error> {
    assert!(
        !((speaking_rate - 1.0).abs() > f32::EPSILON && mode != "premium"),
        "speaking_rate was set without premium mode"
    );

    let mut data = Vec::new();
    for url in fetch_url(tts_service, content, lang, mode, speaking_rate) {
        data.push(reqwest.get(url).send().await?.error_for_status()?.bytes().await?);
    }
    Ok(data.into_iter().flatten().collect())
}

pub fn fetch_url(tts_service: &reqwest::Url, content: String, lang: &str, mode: &str, speaking_rate: f32) -> Vec<reqwest::Url> {
    let fetch_url = |chunk: String| {
        let mut url = tts_service.clone();
        url.set_path("tts");
        url.query_pairs_mut()
            .append_pair("text", &chunk)
            .append_pair("lang", lang)
            .append_pair("mode", mode)
            .append_pair("speaking_rate", &speaking_rate.to_string())
            .finish();
        url
    };

    if mode == "gTTS" {
        content.chars().chunks(200).into_iter().map(std::iter::Iterator::collect::<String>).map(|chunk| {
            fetch_url(chunk)
        }).collect()
    } else{
        vec![fetch_url(content)]
    }
}


pub fn get_premium_voices() -> PremiumVoices {
    // {lang_accent: {variant: gender}}
    let mut cleaned_map = BTreeMap::new();
    let raw_map: Vec<GoogleVoice<'_>> = json::from_str(std::include_str!("data/langs-premium.json")).unwrap();    

    for gvoice in raw_map {
        let mode_variant: String = gvoice.name.split_inclusive('-').skip(2).collect();
        let (mode, variant) = mode_variant.split_once('-').unwrap();

        if mode == "Standard" {
            let [language] = gvoice.languageCodes;

            let inner_map = cleaned_map.entry(language).or_insert_with(BTreeMap::new);
            inner_map.insert(String::from(variant), match gvoice.ssmlGender {
                "MALE" => Gender::Male,
                "FEMALE" => Gender::Female,
                _ => unreachable!()
            });
        }
    }

    cleaned_map
}

pub async fn get_espeak_voices(reqwest: &reqwest::Client, mut tts_service: reqwest::Url) -> Result<Vec<String>, Error> {
    tts_service.set_path("voices");
    tts_service.query_pairs_mut()
        .append_pair("mode", "eSpeak")
        .finish();

    Ok(
        reqwest.get(tts_service)
            .send().await?
            .error_for_status()?
            .json().await?
        )
}

pub fn get_gtts_voices() -> BTreeMap<String, String> {
    json::from_str(std::include_str!("data/langs-free.json")).unwrap()
}

pub const fn netural_colour(premium: bool) -> u32{
    if premium {
        PREMIUM_NEUTRAL_COLOUR
    } else {
        FREE_NEUTRAL_COLOUR
    }
}

pub fn random_footer(prefix: &str, server_invite: &str, client_id: u64) -> String {
    [
        format!("If you want to support the development and hosting of TTS Bot, check out {}donate!", prefix),
        format!("There are loads of customizable settings, check out {}settings help", prefix),
        format!("If you find a bug or want to ask a question, join the support server: {}", server_invite),
        format!("You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{}", client_id)
    ].choose(&mut rand::thread_rng()).unwrap().clone()
}

fn parse_acronyms(original: &str) -> String {
    original.split(' ').map(|word|
        match word {
            "iirc" => "if I recall correctly",
            "afaik" => "as far as I know",
            "wdym" => "what do you mean",
            "imo" => "in my opinion",
            "brb" => "be right back",
            "irl" => "in real life",
            "jk" => "just kidding",
            "btw" => "by the way",
            ":)" => "smiley face",
            "gtg" => "got to go",
            "rn" => "right now",
            ":(" => "sad face",
            "ig" => "i guess",
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
    lavalink: &LavalinkClient,

    channel: u64,
    prefix: &str,
    autojoin: bool,
    bot_ignore: bool,
    require_voice: bool,
    audience_ignore: bool,
) -> Result<Option<String>, Error> {
    let cache = &ctx.cache;
    let guild = message
        .guild(cache)
        .expect("guild not in cache after check");

    if channel as u64 != message.channel_id.0 {
        if message.author.bot {
            return Ok(None)
        }

        return Ok(None)
    }

    let mut content = serenity::content_safe(cache, &message.content,
        &serenity::ContentSafeOptions::default()
            .clean_here(false)
            .clean_everyone(false)
            .show_discriminator(false)
            .display_as_member_from(&guild),
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

    let voice_state = guild.voice_states.get(&message.author.id);
    if message.author.bot {
        if bot_ignore || voice_state.is_none() {
            return Ok(None) // Is bot
        }
    } else {
        // If the bot is in vc
        if let Some(vc) = guild.voice_states.get(&cache.current_user_id()) {
            // If the user needs to be in the vc, and the user's voice channel is not the same as the bot's
            if require_voice && vc.channel_id != voice_state.and_then(|vs| vs.channel_id) {
                return Ok(None); // Wrong vc
            }
        // Else if the user is in the vc and autojoin is on TEMP DISABLED
        // } else if let Some(voice_state) = voice_state && autojoin {
        //     let voice_channel = voice_state.channel_id.try_unwrap()?;
        //     ctx.join_vc(lavalink, guild.id, voice_channel).await?;
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

    Ok(Some(content))
}

#[allow(clippy::too_many_arguments)]
pub fn clean_msg(
    content: &str,

    guild: &serenity::Guild,
    member: &serenity::Member,
    attachments: &[serenity::Attachment],

    lang: &str,
    xsaid: bool,
    repeated_limit: usize,
    nickname: Option<&str>,

    last_to_xsaid_tracker: &LastToXsaidTracker
) -> String {
    let contained_url;
    let mut content = if content == "?" {
        contained_url = false;
        String::from("what")
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


        if lang == "en" {
            content = parse_acronyms(&content);
        }

        let filtered_content: String = linkify::LinkFinder::new()
            .spans(&content)
            .filter(|span| span.kind().is_none())
            .map(|span| span.as_str())
            .collect();

        contained_url = content != filtered_content;
        filtered_content
    };

    // If xsaid is enabled, and the author has not been announced last (in one minute if more than 2 users in vc)
    let last_to_xsaid = last_to_xsaid_tracker.get(&guild.id);
    let voice_channel_id = guild.voice_states.get(&member.user.id).and_then(|vs| vs.channel_id);

    if xsaid && match last_to_xsaid.map(|i| *i) {
        Some((u_id, last_time)) => {
            if let Some(voice_channel_id) = voice_channel_id {
                (member.user.id != u_id) || ((last_time.elapsed().unwrap().as_secs() > 60) && {
                    // If more than 2 users in vc
                    guild.voice_states.values()
                        .filter(|vs| vs.channel_id.map_or(false, |vc| vc == voice_channel_id))
                        .filter_map(|vs| guild.members.get(&vs.user_id))
                        .filter(|member| !member.user.bot)
                        .count() > 2
                })
            } else {true}
        }, None => true
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


pub async fn translate(content: &str, target_lang: &str, data: &crate::structs::Data) -> Result<Option<String>, Error> {
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
