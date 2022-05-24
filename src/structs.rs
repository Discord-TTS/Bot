use std::collections::{HashMap, BTreeMap};
use std::{sync::Arc, borrow::Cow};

use strum_macros::IntoStaticStr;

use poise::serenity_prelude as serenity;
use tracing::warn;

use crate::{database, analytics, into_static_display};

pub use anyhow::{Error, Result};

#[derive(serde::Deserialize)]
pub struct Config {
    #[serde(rename="Main")] pub main: MainConfig,
    #[serde(rename="Webhook-Info")] pub webhooks: WebhookConfigRaw,
}

#[derive(serde::Deserialize)]
pub struct MainConfig {
    pub announcements_channel: serenity::ChannelId,
    pub tts_service_auth_key: Option<String>,
    pub invite_channel: serenity::ChannelId,
    pub translation_token: Option<String>,
    pub main_server: serenity::GuildId,
    pub patreon_service: reqwest::Url,
    pub ofs_role: serenity::RoleId,
    pub main_server_invite: String,
    pub tts_service: reqwest::Url,
    pub token: Option<String>,
    pub log_level: String,
}

#[derive(serde::Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub user: String,
    pub database: String,
    pub password: String,
    pub max_connections: Option<u32>
}

#[derive(serde::Deserialize)]
pub struct WebhookConfigRaw {
    pub logs: reqwest::Url,
    pub errors: reqwest::Url,
    pub servers: reqwest::Url,
    pub dm_logs: reqwest::Url,
    pub suggestions: reqwest::Url,
}

pub struct WebhookConfig {
    pub logs: serenity::Webhook,
    pub errors: serenity::Webhook,
    pub servers: serenity::Webhook,
    pub dm_logs: serenity::Webhook,
    pub suggestions: serenity::Webhook,
}


pub struct JoinVCToken (pub serenity::GuildId);
impl JoinVCToken {
    pub fn acquire(data: &Data, guild_id: serenity::GuildId) -> Arc<tokio::sync::Mutex<Self>> {
        data.join_vc_tokens.entry(guild_id).or_insert_with(|| {
            Arc::new(tokio::sync::Mutex::new(Self(guild_id)))
        }).clone()
    }
}

pub enum FailurePoint {
    NotSubscribed(serenity::UserId),
    PremiumUser,
    Guild,
}

#[derive(serde::Deserialize, Clone, Copy)]
pub struct PatreonInfo {
    pub tier: u8,
    pub entitled_servers: u8,
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
    pub last_to_xsaid_tracker: LastToXsaidTracker,
    pub startup_message: serenity::MessageId,
    pub start_time: std::time::SystemTime,
    pub premium_avatar_url: String,
    pub reqwest: reqwest::Client,
    pub webhooks: WebhookConfig,
    pub pool: sqlx::PgPool,
    pub config: MainConfig,

    pub espeak_voices: Vec<String>,
    pub gtts_voices: BTreeMap<String, String>,
    pub polly_voices: BTreeMap<String, PollyVoice>,
    pub gcloud_voices: BTreeMap<String, BTreeMap<String, GoogleGender>>,
}

impl Data {
    pub fn default_catalog(&self) -> Option<&gettext::Catalog> {
        self.translations.get("en-US")
    }

    pub async fn fetch_patreon_info(&self, user_id: serenity::UserId) -> Result<Option<PatreonInfo>> {
        let mut url = self.config.patreon_service.clone();
        url.set_path(&format!("/members/{user_id}"));

        self.reqwest.get(url)
            .send().await?
            .error_for_status()?
            .json().await
            .map_err(Into::into)
    }

    pub async fn premium_check(&self, guild_id: Option<serenity::GuildId>) -> Result<Option<FailurePoint>> {
        let guild_id = match guild_id {
            Some(guild) => guild,
            None => return Ok(Some(FailurePoint::Guild))
        };

        let guild_row = self.guilds_db.get(guild_id.0 as i64).await?;
        if let Some(raw_user_id) = guild_row.premium_user {
            let patreon_user_id = serenity::UserId(raw_user_id as u64);
            if self.fetch_patreon_info(patreon_user_id).await?.is_some() {
                Ok(None)
            } else {
                Ok(Some(FailurePoint::NotSubscribed(patreon_user_id)))
            }
        } else {
            Ok(Some(FailurePoint::PremiumUser))
        }
    }

    pub async fn parse_user_or_guild(&self, author_id: serenity::UserId, guild_id: Option<serenity::GuildId>) -> Result<(Cow<'static, str>, TTSMode)> {
        let user_row = self.userinfo_db.get(author_id.into()).await?;
        let guild_is_premium = self.premium_check(guild_id).await?.is_none();
        let mut guild_row = None;

        let mut mode = {
            let user_mode =
                if guild_is_premium {
                    user_row.premium_voice_mode
                } else {
                    user_row.voice_mode
                };

            if let Some(mode) = user_mode {
                mode
            } else if let Some(guild_id) = guild_id {
                guild_row = Some(self.guilds_db.get(guild_id.into()).await?);
                guild_row.as_ref().unwrap().voice_mode
            } else {
                TTSMode::gTTS
            }
        };

        let user_voice_row = self.user_voice_db.get((author_id.into(), mode)).await?;
        let voice =
            // Get user voice for user mode
            if user_voice_row.user_id != 0 {
                user_voice_row.voice.clone().map(Cow::Owned)
            } else if let Some(guild_id) = guild_id {
                // Get default server voice for user mode
                let guild_voice_row = self.guild_voice_db.get((guild_id.into(), mode)).await?;
                if guild_voice_row.guild_id == 0 {
                    None
                } else {
                    Some(Cow::Owned(guild_voice_row.voice.clone()))
                }
            } else {
                None
            }.unwrap_or_else(|| Cow::Borrowed(mode.default_voice()));

        if mode.is_premium() && !guild_is_premium {
            mode = TTSMode::default();

            if user_row.voice_mode.map_or(false, TTSMode::is_premium) {
                warn!("User ID {author_id}'s normal voice mode is set to a premium mode! Resetting.");
                self.userinfo_db.set_one(author_id.into(), "voice_mode", mode).await?;
            } else if let Some(guild_id) = guild_id && let Some(guild_row) = guild_row && guild_row.voice_mode.is_premium() {
                warn!("Guild ID {guild_id}'s voice mode is set to a premium mode without being premium! Resetting.");
                self.guilds_db.set_one(guild_id.into(), "voice_mode", mode).await?;
            } else {
                warn!("Guild {guild_id:?} - User {author_id} has a mode set to premium without being premium!");
            }
        }

        Ok((voice, mode))
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
    pub async fn fetch_voices(self, mut tts_service: reqwest::Url, reqwest: &reqwest::Client, auth_key: Option<&str>) -> Result<reqwest::Response> {
        tts_service.set_path("voices");
        tts_service.query_pairs_mut()
            .append_pair("mode", self.into())
            .append_pair("raw", "true")
            .finish();

        reqwest
            .get(tts_service)
            .header("Authorization", auth_key.unwrap_or(""))
            .send().await?.error_for_status().map_err(Into::into)
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

into_static_display!(TTSMode);

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

#[derive(serde::Deserialize)]
pub struct DeeplTranslateResponse {
    pub translations: Vec<DeeplTranslation>
}

#[derive(serde::Deserialize)]
pub struct DeeplTranslation {
    pub text: String,
    pub detected_source_language: String
}

#[derive(serde::Deserialize)]
pub struct DeeplVoice {
    pub language: String,
}

#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
pub struct GoogleVoice {
    pub name: String,
    pub ssmlGender: GoogleGender,
    pub languageCodes: [String; 1],
}

#[derive(serde::Deserialize)]
pub struct PollyVoice {
    pub additional_language_codes: Option<Vec<String>>,
    pub language_code: String,
    pub language_name: String,
    pub gender: PollyGender,
    pub name: String,
    pub id: String,
}

#[derive(serde::Deserialize, IntoStaticStr, Copy, Clone)]
pub enum GoogleGender {
    #[serde(rename="MALE")] Male,
    #[serde(rename="FEMALE")] Female
}

#[derive(serde::Deserialize, IntoStaticStr, Copy, Clone)]
pub enum PollyGender {
    Male,
    Female
}

into_static_display!(GoogleGender);
into_static_display!(PollyGender);


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
    pub const fn should_ignore(self) -> bool {
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
pub type Context<'a> = poise::Context<'a, Data, CommandError>;
pub type ApplicationContext<'a> = poise::ApplicationContext<'a, Data, CommandError>;

pub type CommandError = Error;
pub type CommandResult<E=Error> = Result<(), E>;
pub type Framework = poise::Framework<Data, CommandError>;
pub type FrameworkContext<'a> = poise::FrameworkContext<'a, Data, CommandError>;
pub type LastToXsaidTracker = dashmap::DashMap<serenity::GuildId, (serenity::UserId, std::time::SystemTime)>;


