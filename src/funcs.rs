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

use std::fmt::Write;
use std::collections::BTreeMap;

use itertools::Itertools;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use rand::prelude::SliceRandom;
use lavalink_rs::LavalinkClient;

use poise::serenity_prelude as serenity;
use serenity::json::prelude as json;

use crate::structs::{SerenityContextAdditions, Error, LastToXsaidTracker, OptionTryUnwrap};
use crate::database::DatabaseHandler;

pub async fn parse_voice(
    guilds_db: &DatabaseHandler<i64>,
    userinfo_db: &DatabaseHandler<i64>,
    author_id: serenity::UserId,
    guild_id: Option<serenity::GuildId>,
) -> Result<String, Error> {
    let user_voice: Option<String> = userinfo_db.get(author_id.into()).await?.get("voice");

    Ok(
        match guild_id {
            Some(guild_id) => {
                let settings = guilds_db.get(guild_id.into()).await?;
                user_voice.or_else(|| settings.get("default_voice"))
            }
            None => user_voice
        }.unwrap_or_else(|| String::from(if cfg!(feature="premium") {"en-us a"} else {"en"}))
    )
}

#[cfg(feature="premium")]
pub async fn fetch_audio(data: &crate::structs::Data, content: String, lang: &str, speaking_rate: f32) -> Result<String, Error> {
    let jwt_token = {
        let mut jwt_token = data.jwt_token.lock();
        let mut expire_time = data.jwt_expire.lock();

        if let Some((new_token, new_expire)) = generate_jwt(
            &data.service_acc,
            &expire_time,
        )? {
            *expire_time = new_expire;
            *jwt_token = new_token;
        };

        jwt_token.clone()
    };

    let resp = data.reqwest.post("https://texttospeech.googleapis.com/v1/text:synthesize")
        .header("Authorization", format!("Bearer {jwt_token}"))
        .json(&generate_google_json(&content, lang, speaking_rate)?)
    .send().await?;

    if resp.status().as_u16() == 200 {
        let data: json::Value = resp.json().await?;
        Ok(String::from(data["audioContent"].as_str().unwrap()))
    } else {
        Err(Error::Tts(resp))
    }
}

#[cfg(feature="premium")]
pub fn generate_google_json(content: &str, lang: &str, speaking_rate: f32) -> Result<serenity::json::Value, Error> {
    let (lang, variant) = lang.split_once(' ').ok_or_else(|| 
        format!("{} cannot be parsed into lang and variant", lang)
    )?;

    Ok(
        serenity::json::prelude::json!({
            "input": {
                "text": content
            },
            "voice": {
                "languageCode": lang,
                "name": format!("{}-Standard-{}", lang, variant),
            },
            "audioConfig": {
                "audioEncoding": "OGG_OPUS",
                "speakingRate": speaking_rate
            }
        })
    )
}


#[cfg(feature="premium")]
fn generate_jwt(service_account: &crate::structs::ServiceAccount, expire_time: &std::time::SystemTime) -> Result<Option<(String, std::time::SystemTime)>, Error> {
    let current_time = std::time::SystemTime::now();
    if &current_time > expire_time  {
        let private_key_raw = &service_account.private_key;
        let private_key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key_raw.as_bytes())?;

        let mut headers = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        headers.kid = Some(private_key_raw.clone());

        let new_expire_time = current_time + std::time::Duration::from_secs(3600);
        let payload = serenity::json::prelude::json!({
            "exp": new_expire_time.duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "iat": current_time.duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "aud": "https://texttospeech.googleapis.com/",
            "iss": service_account.client_email,
            "sub": service_account.client_email,
        });

        Ok(Some((jsonwebtoken::encode(&headers, &payload, &private_key)?, new_expire_time)))
    } else {
        Ok(None)
    }
}


#[cfg(not(feature="premium"))]
pub async fn fetch_audio(reqwest: &reqwest::Client, proxy: &Option<String>, content: String, lang: &str) -> Result<Vec<u8>, Error> {
    let mut audio_buf = Vec::new();
    for url in parse_url(proxy, &content, lang) {
        let resp = reqwest.get(url).send().await?;
        let status = resp.status();
        if status == 200 {
            audio_buf.append(&mut resp.bytes().await?.to_vec())
        } else {    
            return Err(Error::Tts(resp))
        }
    }

    Ok(audio_buf)
}

#[cfg(not(feature="premium"))]
pub fn parse_url(proxy: &Option<String>, content: &str, lang: &str) -> Vec<reqwest::Url> {
    content.chars().chunks(200).into_iter().map(|c| c.collect::<String>()).map(|chunk| {
        let mut url = reqwest::Url::parse("https://translate.google.com/translate_tts?ie=UTF-8&total=1&idx=0&client=tw-ob").unwrap();
        url.query_pairs_mut()
            .append_pair("tl", lang)
            .append_pair("q", &chunk)
            .append_pair("textlen", &chunk.len().to_string())
            .finish();

        if let Some(proxy_url) = proxy {
            let mut temp_url = reqwest::Url::parse(proxy_url).unwrap();
            temp_url.set_path(url.as_str());
            temp_url
        } else {
            url
        }
    }).collect()
}



#[cfg(feature="premium")]
pub fn get_supported_languages() -> crate::structs::VoiceData { 
    use crate::structs::{GoogleVoice, Gender};

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

#[cfg(not(feature="premium"))]
pub fn get_supported_languages() -> BTreeMap<String, String> {
    json::from_str(std::include_str!("data/langs-free.json")).unwrap()
}


pub fn random_footer(prefix: Option<&str>, server_invite: Option<&str>, client_id: Option<u64>) -> String {
    let mut footers = Vec::with_capacity(4);
    if let Some(prefix) = prefix {
        footers.extend([
            format!("If you want to support the development and hosting of TTS Bot, check out {}donate!", prefix),
            format!("There are loads of customizable settings, check out {}settings help", prefix),
        ])
    }
    if let Some(server_invite) = server_invite {
        footers.push(format!("If you find a bug or want to ask a question, join the support server: {}", server_invite))
    }
    if let Some(client_id) = client_id {
        footers.push(format!("You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{}", client_id))
    }

    footers.choose(&mut rand::thread_rng()).unwrap().clone()
}

fn parse_acronyms(original: &str) -> String {
    let mut new_string = String::new();
    for word in original.split(' ') {
        write!(new_string, "{} ",
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
        )
        .unwrap();
    }

    String::from(new_string.strip_prefix(' ').unwrap_or(&new_string))
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

#[allow(clippy::too_many_arguments)]
pub async fn run_checks(
    ctx: &serenity::Context,
    message: &serenity::Message,
    lavalink: &LavalinkClient,

    channel: u64,
    prefix: String,
    autojoin: bool,
    bot_ignore: bool,
    audience_ignore: bool,
) -> Result<Option<String>, Error> {
    let cache = &ctx.cache;
    let guild = message
        .guild(cache)
        .expect("guild not in cache after check");

    if channel as u64 != message.channel_id.0 {
        if message.author.bot {
            return Ok(None)
        } else {
            return Err(Error::DebugLog("Failed check: Wrong channel"))
        }
    }

    let mut content = serenity::content_safe(cache, &message.content,
        &serenity::ContentSafeOptions::default()
            .clean_here(false)
            .clean_everyone(false)
            .show_discriminator(false)
            .display_as_member_from(&guild),
    );

    if content.len() >= 1500 {
        return Err(Error::DebugLog("Failed check: Message too long!"));
    }

    content = content.to_lowercase();
    content = String::from(
        content
            .strip_prefix(&format!("{}{}", &prefix, "tts"))
            .unwrap_or(&content),
    ); // remove -tts if starts with

    if content.starts_with(&prefix) {
        return Err(Error::DebugLog(
            "Failed check: Starts with prefix",
        ));
    }

    let voice_state = guild.voice_states.get(&message.author.id);
    if message.author.bot {
        if bot_ignore || voice_state.is_none() {
            return Ok(None); // Err(Error::DebugLog("Failed check: Is bot"))
        }
    } else {
        let voice_state = voice_state.ok_or(Error::DebugLog("Failed check: user not in vc"))?;
        let voice_channel = voice_state.channel_id.ok_or("vc.channel_id is None")?;

        match guild.voice_states.get(&cache.current_user_id()) {
            Some(vc) => {
                if vc.channel_id != voice_state.channel_id {
                    return Err(Error::DebugLog("Failed check: Wrong vc"));
                }
            },
            None => {
                if !autojoin {
                    return Err(Error::DebugLog("Failed check: Bot not in vc"));
                }

                ctx.join_vc(lavalink, &guild.id, &voice_channel).await?;
            }
        };

        if let serenity::Channel::Guild(channel) = guild.channels.get(&voice_channel).ok_or("channel is None")? {
            if channel.kind == serenity::ChannelType::Stage && voice_state.suppress && audience_ignore {
                return Err(Error::DebugLog("Failed check: Is audience"));
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
pub async fn clean_msg(
    content: String,

    guild: &serenity::Guild,
    member: serenity::Member,
    attachments: &[serenity::Attachment],

    lang: &str,
    xsaid: bool,
    repeated_limit: usize,
    nickname: Option<String>,

    last_to_xsaid_tracker: &LastToXsaidTracker
) -> Result<String, Error> {
    // Regex
    lazy_static! {
        static ref EMOJI_REGEX: Regex = Regex::new(r"<(a?):(.+):\d+>").unwrap();
    }
    let mut content = EMOJI_REGEX
        .replace_all(&content, |re_match: &Captures<'_>| {
            let is_animated = re_match.get(1).unwrap().as_str();
            let emoji_name = re_match.get(2).unwrap().as_str();

            let emoji_prefix = if is_animated.is_empty() {
                "emoji"
            } else {
                "animated emoji"
            };

            format!("{} {}", emoji_prefix, emoji_name)
        })
        .into_owned();

    if content == "?" {
        content = String::from("what")
    } else {
        if lang == "en" {
            content = parse_acronyms(&content)
        }

        // Speeeeeeeeeeeeeeeeeeeeeed
        lazy_static! {
            static ref REGEX_REPLACEMENTS: [(Regex, &'static str); 3] = {
                [
                    (Regex::new(r"\|\|(?s:.)*?\|\|").unwrap(), ". spoiler avoided."),
                    (Regex::new(r"```(?s:.)*?```").unwrap(), ". code block."),
                    (Regex::new(r"`(?s:.)*?`").unwrap(), ". code snippet."),
                ]
            };
        }

        for (regex, replacement) in REGEX_REPLACEMENTS.iter() {
            content = regex.replace_all(&content, *replacement).into_owned();
        }
    }

    // TODO: Regex url stuff?
    let with_urls = content.split(' ').join(" ");
    content = content
        .split(' ')
        .filter(|w|
            ["https://", "http://", "www."].iter()
            .all(|ls| !w.starts_with(ls))
        )
        .join(" ");

    let contained_url = content != with_urls;

    let last_to_xsaid = last_to_xsaid_tracker.get(&member.guild_id);

    // If xsaid is enabled, and the author has not been announced last (in one minute if more than 2 users in vc)
    if xsaid && match last_to_xsaid.map(|i| *i) {
        Some((u_id, last_time)) => {
            (member.user.id != u_id) || ((last_time.elapsed().unwrap().as_secs() > 60) && {
                // If more than 2 users in vc
                let voice_channel_id = guild.voice_states
                    .get(&member.user.id).try_unwrap()?
                    .channel_id.try_unwrap()?;

                guild.voice_states.values().filter_map(|vs| {
                    if Some(voice_channel_id) == vs.channel_id  {
                        Some(!guild.members.get(&vs.user_id)?.user.bot)
                    } else {
                        None
                    }
                }).count() > 2
            })
        },
        None => true
    } {
        if contained_url {
            write!(content, " {}",
                if content.is_empty() {"a link."}
                else {"and sent a link"}
            ).unwrap();
        }

        let said_name = nickname.unwrap_or_else(|| member.nick.unwrap_or_else(|| member.user.name.clone()));
        content = match attachments_to_format(attachments) {
            Some(file_format) if content.is_empty() => format!("{} sent {}", said_name, file_format),
            Some(file_format) => format!("{} sent {} and said {}", said_name, file_format, content),
            None => format!("{} said: {}", said_name, content),
        }
    } else if contained_url {
        write!(content, "{}",
            if content.is_empty() {"a link."}
            else {". This message contained a link"}
        ).unwrap()
    }

    if xsaid {
        last_to_xsaid_tracker.insert(member.guild_id, (member.user.id, std::time::SystemTime::now()));
    }

    if repeated_limit != 0 {
        content = remove_repeated_chars(&content, repeated_limit as usize)
    }

    Ok(content)
}

#[cfg(feature="premium")]
pub async fn translate(content: &str, target_lang: &str, data: &crate::structs::Data) -> Result<Option<String>, Error> {
    let url = format!("{}/translate", crate::constants::TRANSLATION_URL);
    let response: crate::structs::DeeplTranslateResponse = data.reqwest.get(url)
        .query(&serenity::json::prelude::json!({
            "text": content,
            "target_lang": target_lang,
            "preserve_formatting": 1u8,
            "auth_key": &data.config.translation_token,
        }))
        .send().await?.error_for_status()?
        .json().await?;

    if let Some(translation) = response.translations.first() {
        if translation.detected_source_language != target_lang {
            return Ok(Some(translation.text.clone()))
        }
    }

    Ok(None)
}
