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
use itertools::Itertools;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use rand::prelude::SliceRandom;
use lavalink_rs::LavalinkClient;

use poise::serenity_prelude as serenity;

use crate::constants::*;
use crate::database::DatabaseHandler;

#[serenity::async_trait]
pub trait PoiseContextAdditions {
    async fn send_error(&self, error: &str, fix: Option<String>) -> Result<Option<poise::ReplyHandle<'_>>, Error>;
}
#[serenity::async_trait]
pub trait SerenityContextAdditions {
    async fn user_from_dm(&self, dm_name: &str) -> Option<serenity::User>;
    async fn join_vc(
        &self,
        lavalink: &LavalinkClient,
        guild_id: &serenity::GuildId,
        channel_id: &serenity::ChannelId,
    ) -> Result<(), &'static str>;
}

#[serenity::async_trait]
impl PoiseContextAdditions for Context<'_> {
    async fn send_error(&self, error: &str, fix: Option<String>) -> Result<Option<poise::ReplyHandle<'_>>, Error> {
        let author = self.author();
        let fix =
            fix.unwrap_or_else(|| String::from("get in contact with us via the support server"));

        let ctx_discord = self.discord();
        let (name, avatar_url) = match self.channel_id().to_channel(ctx_discord).await? {
            serenity::Channel::Guild(channel) => {
                let permissions = channel
                    .permissions_for_user(ctx_discord, ctx_discord.cache.current_user_id())?;

                if !permissions.send_messages() {
                    return Ok(None);
                };

                if !permissions.embed_links() {
                    self.send(|b| {
                        b.ephemeral(true);
                        b.content("An Error Occurred! Please give me embed links permissions so I can tell you more!")
                    }).await?;
                    return Ok(None);
                };

                match channel.guild_id.member(ctx_discord, author.id).await {
                    Ok(member) => (member.display_name().into_owned(), member.face()),
                    Err(_) => (author.name.clone(), author.face()),
                }
            }
            serenity::Channel::Private(_) => (author.name.clone(), author.face()),
            _ => unreachable!(),
        };

        Ok(
            self.send(|b| {
                b.ephemeral(true);
                b.embed(|e| {
                    e.colour(RED);
                    e.title("An Error Occurred!");
                    e.description(format!("Sorry but {}, to fix this, please {}!", error, fix));
                    e.author(|a| {
                        a.name(name);
                        a.icon_url(avatar_url)
                    });
                    e.footer(|f| f.text(format!(
                        "Support Server: {}", self.data().config.server_invite
                    )))
                })
            })
            .await?
        )
    }
}

#[serenity::async_trait]
impl SerenityContextAdditions for serenity::Context {
    async fn user_from_dm(&self, dm_name: &str) -> Option<serenity::User> {
        lazy_static! {
            static ref ID_IN_BRACKETS_REGEX: Regex = Regex::new(r"\((\d+)\)").unwrap();
        }

        let re_match = ID_IN_BRACKETS_REGEX.captures(dm_name)?;
        let user_id: u64 = re_match.get(1)?.as_str().parse().ok()?;
        self.http.get_user(user_id).await.ok()
    }

    async fn join_vc(
        &self,
        lavalink: &LavalinkClient,
        guild_id: &serenity::GuildId,
        channel_id: &serenity::ChannelId,
    ) -> Result<(), &'static str> {
        let manager = songbird::get(self).await.unwrap();
        let (_, handler) = manager.join_gateway(guild_id.0, channel_id.0).await;

        match handler {
            Ok(connection_info) => {
                match lavalink
                    .create_session_with_songbird(&connection_info)
                    .await
                {
                    Ok(_) => Ok(()),
                    _ => Err("lavalink failed"),
                }
            }
            Err(_) => Err("Error joining the channel"),
        }
    }
}

pub async fn parse_lang(
    settings: &DatabaseHandler<u64>,
    userinfo: &DatabaseHandler<u64>,
    author_id: serenity::UserId,
    guild_id: Option<serenity::GuildId>,
) -> Result<String, Error> {
    let user_lang: Option<String> = userinfo.get(author_id.into()).await?.get("lang");

    Ok(match guild_id {
        Some(guild_id) => {
            let settings = settings.get(guild_id.into()).await?;
            user_lang
                .or_else(|| settings.get("default_lang"))
                .unwrap_or_else(|| String::from("en"))
        }
        None => user_lang.unwrap_or_else(|| String::from("en")),
    })
}

pub fn parse_url(content: &str, lang: String) -> Vec<url::Url> {
    content.chars().chunks(200).into_iter().map(|c| c.collect::<String>()).map(|chunk| {
        let mut url = url::Url::parse("https://translate.google.com/translate_tts?ie=UTF-8&total=1&idx=0&client=tw-ob").unwrap();
        url.query_pairs_mut()
            .append_pair("tl", &lang)
            .append_pair("q", &chunk)
            .append_pair("textlen", &chunk.len().to_string())
            .finish();
        url
    }).collect()
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

pub async fn run_checks(
    ctx: &serenity::Context,
    message: &serenity::Message,
    lavalink: &LavalinkClient,

    channel: u64,
    prefix: String,
    autojoin: bool,
    bot_ignore: bool,
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

    let bot_user_id = cache.current_user_id();
    let bot_voice_state = guild.voice_states.get(&bot_user_id);
    let voice_state = guild.voice_states.get(&message.author.id);
    if message.author.bot {
        if bot_ignore || voice_state.is_none() {
            return Ok(None); // Err(Error::DebugLog("Failed check: Is bot"))
        }
    } else {
        let voice_state = voice_state.ok_or(Error::DebugLog("Failed check: user not in vc"))?;
        let bot_voice_state = match bot_voice_state {
            Some(vc) => vc,
            None => {
                if !autojoin {
                    return Err(Error::DebugLog("Failed check: Bot not in vc"));
                }

                ctx.join_vc(
                    lavalink,
                    &guild.id,
                    &voice_state.channel_id.ok_or("vc.channel_id is None")?,
                )
                .await?;
                guild
                    .voice_states
                    .get(&bot_user_id)
                    .ok_or("bot.vc still None")?
            }
        };

        if bot_voice_state.channel_id != voice_state.channel_id {
            return Err(Error::DebugLog("Failed check: Wrong vc"));
        }
    }

    Ok(Some(content))
}
pub fn clean_msg(
    content: String,

    member: serenity::Member,
    attachments: &[serenity::Attachment],

    lang: &str,
    xsaid: bool,
    repeated_limit: usize,
    nickname: Option<String>
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
            .any(|ls| w.starts_with(ls))
        )
        .join(" ");

    let contained_url = content != with_urls;

    if xsaid {
        let said_name = nickname.unwrap_or_else(|| member.nick.unwrap_or_else(|| member.user.name.clone()));
        let file_format = attachments_to_format(attachments);

        if contained_url {
            write!(content, " {}",
                if content.is_empty() {"a link."}
                else {"and sent a link"}
            ).unwrap();
        }

        content = match file_format {
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

    let mut removed_chars_content = content.clone();
    removed_chars_content.retain(|c| !" ?.)'!\":".contains(c));

    if repeated_limit != 0 {
        content = remove_repeated_chars(&content, repeated_limit as usize)
    }

    Ok(content)
}
