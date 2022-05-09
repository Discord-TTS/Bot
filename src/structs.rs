use std::collections::{HashMap, BTreeMap};
use std::{sync::Arc, borrow::Cow};

use strum_macros::IntoStaticStr;
use lazy_static::lazy_static;
use regex::Regex;

use poise::serenity_prelude as serenity;

use crate::{database, analytics};
use crate::constants::{RED, FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};

pub use anyhow::{Error, Result};

#[derive(serde::Deserialize)]
pub struct Config {
    #[serde(rename="Main")] pub main: MainConfig,
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
    pub max_connections: Option<u32>
}

pub struct JoinVCToken (pub serenity::GuildId);
impl JoinVCToken {
    pub fn acquire(data: &Data, guild_id: serenity::GuildId) -> Arc<tokio::sync::Mutex<Self>> {
        data.join_vc_tokens.entry(guild_id).or_insert_with(|| {
            Arc::new(tokio::sync::Mutex::new(Self(guild_id)))
        }).clone()
    }
}

pub struct Data {
    pub analytics: Arc<analytics::Handler>,
    pub guilds_db: database::Handler<i64, database::GuildRow>,
    pub userinfo_db: database::Handler<i64, database::UserRow>,
    pub nickname_db: database::Handler<[i64; 2], database::NicknameRow>,
    pub user_voice_db: database::Handler<(i64, TTSMode), database::UserVoiceRow>,
    pub guild_voice_db: database::Handler<(i64, TTSMode), database::GuildVoiceRow>,

    pub join_vc_tokens: dashmap::DashMap<serenity::GuildId, Arc<tokio::sync::Mutex<JoinVCToken>>>,
    pub system_info: parking_lot::Mutex<sysinfo::System>,
    pub translations: HashMap<String, gettext::Catalog>,
    pub webhooks: HashMap<String, serenity::Webhook>,
    pub last_to_xsaid_tracker: LastToXsaidTracker,
    pub startup_message: serenity::MessageId,
    pub start_time: std::time::SystemTime,
    pub premium_avatar_url: String,
    pub reqwest: reqwest::Client,
    pub pool: sqlx::PgPool,
    pub config: MainConfig,

    pub espeak_voices: Vec<String>,
    pub gtts_voices: BTreeMap<String, String>,
    pub polly_voices: BTreeMap<String, PollyVoice>,
    pub premium_voices: BTreeMap<String, BTreeMap<String, Gender>>,
}

impl Data {
    pub fn default_catalog(&self) -> Option<&gettext::Catalog> {
        self.translations.get("en-US")
    }
}


#[derive(
    IntoStaticStr, sqlx::Type,
    Debug, Hash, PartialEq, Eq, Copy, Clone,
)]
#[allow(non_camel_case_types)]
#[sqlx(rename_all="lowercase")]
#[sqlx(type_name="ttsmode")]
pub enum TTSMode {
    gTTS,
    Polly,
    eSpeak,
    gCloud,
}

impl TTSMode {
    pub async fn fetch_voices(self, mut tts_service: reqwest::Url, reqwest: &reqwest::Client) -> Result<reqwest::Response> {
        tts_service.set_path("voices");
        tts_service.query_pairs_mut()
            .append_pair("mode", self.into())
            .append_pair("raw", "true")
            .finish();

        reqwest
            .get(tts_service).send().await?
            .error_for_status().map_err(Into::into)
    }

    pub const fn is_premium(self) -> bool {
        match self {
            Self::gTTS | Self::eSpeak => false,
            Self::Polly | Self::gCloud => true,
        }
    }

    pub const fn default_voice(self) -> &'static str {
        match self {
            Self::gTTS => "en",
            Self::eSpeak => "en1",
            Self::Polly => "Brian",
            Self::gCloud => "en-US A",
        }
    }

    // min default max kind
    pub const fn speaking_rate_info(self) -> Option<(f32, f32, f32, &'static str)> {
        match self {
            Self::gTTS => None,
            Self::gCloud => Some((0.25, 1.0, 4.0, "x")),
            Self::Polly  => Some((10.0, 100.0, 500.0, "%")),
            Self::eSpeak => Some((100.0, 175.0, 400.0, " words per minute")),
        }
    }
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
pub enum TTSModeChoice {
    // Name to show in slash command invoke           Aliases for prefix
    #[name="Google Translate TTS (female) (default)"] #[name="gtts"]       gTTS,
    #[name="eSpeak TTS (male)"]                       #[name="espeak"]     eSpeak,
    #[name="gCloud TTS (changable)"]                  #[name="gcloud"]     gCloud,
    #[name="Amazon Polly TTS (changable)"]            #[name="polly"]      Polly,
}

impl From<TTSModeChoice> for TTSMode {
    fn from(mode: TTSModeChoice) -> Self {
        match mode {
            TTSModeChoice::gTTS => Self::gTTS,
            TTSModeChoice::Polly => Self::Polly,
            TTSModeChoice::eSpeak => Self::eSpeak,
            TTSModeChoice::gCloud => Self::gCloud,
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

#[derive(serde::Deserialize, Debug, Copy, Clone)]
pub enum Gender {
    Male,
    Female
}

impl std::fmt::Display for Gender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Male => "Male",
            Self::Female => "Female"
        })
    }
}

#[derive(serde::Deserialize)]
pub struct PollyVoice {
    pub additional_language_codes: Option<Vec<String>>,
    pub language_code: String,
    pub language_name: String,
    pub gender: Gender,
    pub name: String,
    pub id: String,
}


fn deserialize_error_code<'de, D: serde::Deserializer<'de>>(to_deserialize: D) -> Result<TTSServiceErrorCode, D::Error> {
    struct IntVisitor {}
    impl<'de> serde::de::Visitor<'de> for IntVisitor {
        type Value = u8;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a number between 0 and 255")
        }

        #[allow(clippy::cast_possible_truncation)]
        fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
            if value > 255 {
                Err(E::custom(format!("{value} is too large")))
            } else {
                Ok(value as u8)
            }
        }
    }

    Ok(match to_deserialize.deserialize_u8(IntVisitor {})? {
        1 => TTSServiceErrorCode::UnknownVoice,
        2 => TTSServiceErrorCode::AudioTooLong,
        3 => TTSServiceErrorCode::InvalidSpeakingRate,
        _ => TTSServiceErrorCode::Unknown,
    })
}

#[derive(Debug)]
pub enum TTSServiceErrorCode {
    Unknown,
    UnknownVoice,
    AudioTooLong,
    InvalidSpeakingRate,
}

impl TTSServiceErrorCode {
    pub fn should_ignore(self) -> bool {
        matches!(self, Self::AudioTooLong)
    }
}

#[must_use]
#[derive(serde::Deserialize)]
pub struct TTSServiceError {
    pub display: String,
    #[serde(deserialize_with = "deserialize_error_code")]
    pub code: TTSServiceErrorCode,
}

impl std::fmt::Display for TTSServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display)
    }
}


pub type Command = poise::Command<Data, CommandError>;
pub type Framework = poise::Framework<Data, CommandError>;
pub type Context<'a> = poise::Context<'a, Data, CommandError>;
pub type ApplicationContext<'a> = poise::ApplicationContext<'a, Data, CommandError>;

pub type CommandError = Error;
pub type CommandResult<E=Error> = Result<(), E>;
pub type LastToXsaidTracker = dashmap::DashMap<serenity::GuildId, (serenity::UserId, std::time::SystemTime)>;

pub trait OptionTryUnwrap<T> {
    fn try_unwrap(self) -> Result<T>;
}
pub trait OptionGettext<'a> {
    fn gettext(self, translate: &'a str) -> &'a str;
}

#[serenity::async_trait]
pub trait PoiseContextExt {
    async fn neutral_colour(&self) -> u32;
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str;
    fn current_catalog(&self) -> Option<&gettext::Catalog>;
    async fn author_permissions(&self) -> Result<serenity::Permissions>;
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>>;
}
#[serenity::async_trait]
pub trait SerenityContextExt {
    async fn user_from_dm(&self, dm_name: &str) -> Option<serenity::User>;
    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>>;
}

#[serenity::async_trait]
impl PoiseContextExt for Context<'_> {
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str {
        self.current_catalog().gettext(translate)
    }

    fn current_catalog(&self) -> Option<&gettext::Catalog> {
        let catalog = if let poise::Context::Application(ctx) = self {
            ctx.data.translations.get(match ctx.interaction.unwrap().locale.as_str() {
                "ko" => "ko-KR",
                "pt-BR" => "pt",
                l => l
            })
        } else {
            None
        };

        catalog.or_else(|| self.data().default_catalog())
    }

    async fn neutral_colour(&self) -> u32 {
        if let Some(guild_id) = self.guild_id() {
            let row = self.data().guilds_db.get(guild_id.0 as i64).await;
            if row.map(|row| row.voice_mode).map_or(false, TTSMode::is_premium) {
                return PREMIUM_NEUTRAL_COLOUR
            }
        }

        FREE_NEUTRAL_COLOUR
    }

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

        let m;
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
                    Ok(member) => {
                        m = member;
                        (m.display_name(), m.face())
                    },
                    Err(_) => (Cow::Borrowed(&author.name), author.face()),
                }
            }
            serenity::Channel::Private(_) => (Cow::Borrowed(&author.name), author.face()),
            _ => unreachable!(),
        };


        match self.send(|b| {b
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
        }).await {
            Ok(handle) => Ok(Some(handle)),
            Err(_) => Ok(None)
        }
    }
}

#[serenity::async_trait]
impl SerenityContextExt for serenity::Context {
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
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>> {
        let manager = songbird::get(self).await.unwrap();
        let (call, r) = manager.join(guild_id.0, channel_id).await;
        r?;
        Ok(call)
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

impl<'a> OptionGettext<'a> for Option<&'a gettext::Catalog> {
    fn gettext(self, translate: &'a str) -> &'a str {
        self.map_or(translate, |c| c.gettext(translate))
    }
}
