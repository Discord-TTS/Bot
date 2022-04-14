use std::{sync::Arc, borrow::Cow};

use postgres_types::{ToSql, FromSql};
use strum_macros::IntoStaticStr;
use lazy_static::lazy_static;
use regex::Regex;

use lavalink_rs::LavalinkClient;
use poise::serenity_prelude as serenity;

use crate::constants::RED;
pub use anyhow::{Error, Result};

#[derive(serde::Deserialize)]
pub struct Config {
    #[serde(rename="Main")] pub main: MainConfig,
    #[serde(rename="Lavalink")] pub lavalink: LavalinkConfig,
    #[serde(rename="Webhook-Info")] pub webhooks: toml::value::Table,
}

#[derive(serde::Deserialize)]
pub struct MainConfig {
    pub translation_token: Option<String>,
    pub patreon_role: serenity::RoleId,
    pub main_server: serenity::GuildId,
    pub main_server_invite: String,
    pub tts_service: reqwest::Url,
    pub token: Option<String>,
    pub invite_channel: u64,
    pub log_level: String,
    pub ofs_role: u64,
}

#[derive(serde::Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub user: String,
    pub database: String,
    pub password: String,
}

#[derive(serde::Deserialize)]
pub struct LavalinkConfig {
    pub password: String,
    pub host: String,
    pub port: u16,
    pub ssl: bool
}


pub struct Data {
    pub analytics: Arc<crate::analytics::Handler>,
    pub guilds_db: crate::database::Handler<i64>,
    pub userinfo_db: crate::database::Handler<i64>,
    pub nickname_db: crate::database::Handler<[i64; 2]>,
    pub user_voice_db: crate::database::Handler<(i64, TTSMode)>,
    pub guild_voice_db: crate::database::Handler<(i64, TTSMode)>,

    pub webhooks: std::collections::HashMap<String, serenity::Webhook>,
    pub system_info: parking_lot::Mutex<sysinfo::System>,
    pub last_to_xsaid_tracker: LastToXsaidTracker,
    pub startup_message: serenity::MessageId,
    pub start_time: std::time::SystemTime,
    pub premium_avatar_url: String,
    pub lavalink: LavalinkClient,
    pub reqwest: reqwest::Client,
    pub config: MainConfig,

    pub premium_voices: PremiumVoices,
    pub pool: Arc<deadpool_postgres::Pool>,
}


#[derive(
    IntoStaticStr, ToSql, FromSql,
    Debug, Hash, PartialEq, Eq, Copy, Clone,
)]
#[allow(non_camel_case_types)]
#[postgres(name="ttsmode")]
pub enum TTSMode {
    #[postgres(name="gtts")] gTTS,
    #[postgres(name="espeak")] eSpeak,
    #[postgres(name="premium")] Premium
}

impl std::fmt::Display for TTSMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.into())
    }
}

impl Default for TTSMode {
    fn default() -> Self {
        Self::gTTS
    }
}

#[derive(poise::ChoiceParameter)]
#[allow(non_camel_case_types)]
pub enum TTSModeServerChoice {
    // Name to show in slash command invoke           Aliases for prefix
    #[name="Google Translate TTS (female) (default)"] #[name="gtts"]       gTTS,
    #[name="eSpeak TTS (male)"]                       #[name="espeak"]     eSpeak,
    #[name="Premium TTS (changable)"]                 #[name="premium"]    Premium,
}

#[derive(poise::ChoiceParameter)]
#[allow(non_camel_case_types)]
pub enum TTSModeChoice {
    // Name to show in slash command invoke           Aliases for prefix
    #[name="Google Translate TTS (female) (default)"] #[name="gtts"]       gTTS,
    #[name="eSpeak TTS (male)"]                       #[name="espeak"]     eSpeak,
}

impl From<TTSModeServerChoice> for TTSMode {
    fn from(mode: TTSModeServerChoice) -> Self {
        match mode {
            TTSModeServerChoice::gTTS => Self::gTTS,
            TTSModeServerChoice::eSpeak => Self::eSpeak,
            TTSModeServerChoice::Premium => Self::Premium
        }
    }
}

impl From<TTSModeChoice> for TTSMode {
    fn from(mode: TTSModeChoice) -> Self {
        match mode {
            TTSModeChoice::gTTS => Self::gTTS,
            TTSModeChoice::eSpeak => Self::eSpeak,
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct DeeplTranslateResponse {
    pub translations: Vec<DeeplTranslation>
}

#[derive(serde::Deserialize, Debug)]
pub struct DeeplTranslation {
    pub text: String,
    pub detected_source_language: String
}

#[derive(serde::Deserialize, Debug)]
pub struct DeeplVoice {
    pub language: String,
}

#[allow(non_snake_case)]
#[derive(serde::Deserialize, Debug)]
pub struct GoogleVoice<'a> {
    pub name: String,
    pub ssmlGender: &'a str,
    pub languageCodes: [String; 1],
}

#[derive(serde::Serialize, Debug, Copy, Clone)]
pub enum Gender {
    Male,
    Female
}

impl std::fmt::Display for Gender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Male => "Male",
            Self::Female => "Female"
        })
    }
}


pub type Command = poise::Command<Data, CommandError>;
pub type Framework = poise::Framework<Data, CommandError>;
pub type Context<'a> = poise::Context<'a, Data, CommandError>;

pub type CommandError = Error;
pub type CommandResult<E=Error> = Result<(), E>;
pub type PremiumVoices = std::collections::BTreeMap<String, std::collections::BTreeMap<String, Gender>>;
pub type LastToXsaidTracker = dashmap::DashMap<serenity::GuildId, (serenity::UserId, std::time::SystemTime)>;

pub trait OptionTryUnwrap<T> {
    fn try_unwrap(self) -> Result<T>;
}

#[serenity::async_trait]
pub trait PoiseContextAdditions {
    async fn author_permissions(&self) -> Result<serenity::Permissions>;
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>>;
}
#[serenity::async_trait]
pub trait SerenityContextAdditions {
    async fn user_from_dm(&self, dm_name: &str) -> Option<serenity::User>;
    async fn join_vc(
        &self,
        lavalink: &LavalinkClient,
        guild_id: serenity::GuildId,
        channel_id: serenity::ChannelId,
    ) -> Result<()>;
}

#[serenity::async_trait]
impl PoiseContextAdditions for Context<'_> {
    async fn author_permissions(&self) -> Result<serenity::Permissions> {
        let ctx_discord = self.discord();

        match ctx_discord.cache.channel(self.channel_id()).try_unwrap()? {
            serenity::Channel::Guild(channel) => {
                let guild = channel.guild(&ctx_discord.cache).try_unwrap()?;
                let member = guild.member(ctx_discord, self.author()).await?;

                Ok(guild.user_permissions_in(&channel, &member)?)
            }
            _ => {
                Ok(((serenity::Permissions::from_bits_truncate(0b111110010000000000001111111100001000000)
                    | serenity::Permissions::SEND_MESSAGES)
                    - serenity::Permissions::SEND_TTS_MESSAGES)
                    - serenity::Permissions::MANAGE_MESSAGES)
            }
        }
    }
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>> {
        let author = self.author();
        let ctx_discord = self.discord();

        let (name, avatar_url) = match self.channel_id().to_channel(ctx_discord).await? {
            serenity::Channel::Guild(channel) => {
                let permissions = channel.permissions_for_user(ctx_discord, ctx_discord.cache.current_user_id())?;

                if !permissions.send_messages() {
                    return Ok(None);
                };

                if !permissions.embed_links() {
                    return self.send(|b| {b
                        .ephemeral(true)
                        .content("An Error Occurred! Please give me embed links permissions so I can tell you more!")
                    }).await.map(Some).map_err(Into::into)
                };

                match channel.guild_id.member(ctx_discord, author.id).await {
                    Ok(member) => (Cow::Owned(member.display_name().into_owned()), member.face()),
                    Err(_) => (Cow::Borrowed(&author.name), author.face()),
                }
            }
            serenity::Channel::Private(_) => (Cow::Borrowed(&author.name), author.face()),
            _ => unreachable!(),
        };


        let send_ret = self.send(|b| {b
            .ephemeral(true)
            .embed(|e| {e
                .colour(RED)
                .title("An Error Occurred!")
                .description(format!(
                    "Sorry but {}, to fix this, please {}!", error,
                    fix.unwrap_or("get in contact with us via the support server"),
                ))
                .author(|a| {a
                    .name(name)
                    .icon_url(avatar_url)
                })
                .footer(|f| f.text(format!(
                    "Support Server: {}", self.data().config.main_server_invite
                )))
            })
        }).await;

        match send_ret {
            Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => Ok(None),
            Ok(reply_handle) => Ok(Some(reply_handle)),
            Err(err) => Err(err.into()),
        }
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
        guild_id: serenity::GuildId,
        channel_id: serenity::ChannelId,
    ) -> Result<()> {
        let manager = songbird::get(self).await.unwrap();

        let (_, handler) = manager.join_gateway(guild_id.0, channel_id.0).await;
        Ok(lavalink.create_session_with_songbird(&handler?).await?)
    }
}

impl<T> OptionTryUnwrap<T> for Option<T> {
    #[track_caller]
    fn try_unwrap(self) -> Result<T> {
        match self {
            Some(v) => Ok(v),
            None => Err({
                let location = std::panic::Location::caller();
                anyhow::anyhow!("Unexpected None value on line {} in {}", location.line(), location.file())
            })
        }
    }
}
