use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use parking_lot::RwLock;
use serde::Deserialize as _;
use strum_macros::IntoStaticStr;

use poise::serenity_prelude::{self as serenity, json};
use tracing::warn;

use crate::{analytics, database, into_static_display};

pub use anyhow::{Error, Result};

#[derive(serde::Deserialize)]
pub struct Config {
    #[serde(rename = "Main")]
    pub main: MainConfig,
    #[serde(rename = "Webhook-Info")]
    pub webhooks: WebhookConfigRaw,
    #[serde(rename = "Website-Info")]
    pub website_info: Option<WebsiteInfo>,
    #[serde(rename = "Bot-List-Tokens")]
    #[serde(default)]
    pub bot_list_tokens: BotListTokens,
}

#[derive(serde::Deserialize)]
pub struct MainConfig {
    pub announcements_channel: serenity::ChannelId,
    pub patreon_service: Option<reqwest::Url>,
    pub translation_url: Option<reqwest::Url>,
    pub tts_service_auth_key: Option<String>,
    pub invite_channel: serenity::ChannelId,
    pub website_url: Option<reqwest::Url>,
    pub translation_token: Option<String>,
    pub main_server: serenity::GuildId,
    pub ofs_role: serenity::RoleId,
    pub main_server_invite: String,
    pub tts_service: reqwest::Url,
    pub token: Option<String>,
    pub log_level: String,

    // Only for situations where gTTS has broken
    #[serde(default)]
    pub gtts_disabled: bool,
}

#[derive(serde::Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub user: String,
    pub database: String,
    pub password: String,
    pub max_connections: Option<u32>,
}

#[derive(serde::Deserialize)]
pub struct WebsiteInfo {
    pub url: reqwest::Url,
    pub stats_key: String,
}

#[derive(serde::Deserialize)]
pub struct WebhookConfigRaw {
    pub logs: reqwest::Url,
    pub errors: reqwest::Url,
    pub dm_logs: reqwest::Url,
}

#[derive(serde::Deserialize, Clone, Default)]
pub struct BotListTokens {
    pub top_gg: Option<String>,
    pub discord_bots_gg: Option<String>,
    pub bots_on_discord: Option<String>,
}

pub struct WebhookConfig {
    pub logs: serenity::Webhook,
    pub dm_logs: serenity::Webhook,
    pub errors: Option<serenity::Webhook>,
}

pub struct JoinVCToken(pub serenity::GuildId);
impl JoinVCToken {
    pub fn acquire(data: &Data, guild_id: serenity::GuildId) -> Arc<tokio::sync::Mutex<Self>> {
        data.join_vc_tokens
            .entry(guild_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(Self(guild_id))))
            .clone()
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

pub struct RegexCache {
    pub replacements: [(regex::Regex, &'static str); 3],
    pub id_in_brackets: regex::Regex,
    pub emoji: regex::Regex,
}

#[derive(Clone)]
pub struct Data(pub Arc<DataInner>);

impl std::ops::Deref for Data {
    type Target = DataInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct DataInner {
    pub analytics: Arc<analytics::Handler>,
    pub guilds_db: database::Handler<i64, database::GuildRowRaw>,
    pub userinfo_db: database::Handler<i64, database::UserRowRaw>,
    pub nickname_db: database::Handler<[i64; 2], database::NicknameRowRaw>,
    pub user_voice_db: database::Handler<(i64, TTSMode), database::UserVoiceRowRaw>,
    pub guild_voice_db: database::Handler<(i64, TTSMode), database::GuildVoiceRowRaw>,

    pub join_vc_tokens: dashmap::DashMap<serenity::GuildId, Arc<tokio::sync::Mutex<JoinVCToken>>>,
    pub system_info: parking_lot::Mutex<sysinfo::System>,
    pub currently_purging: std::sync::atomic::AtomicBool,
    pub translations: HashMap<String, gettext::Catalog>,
    pub fully_started: std::sync::atomic::AtomicBool,
    pub last_to_xsaid_tracker: LastToXsaidTracker,
    pub website_info: RwLock<Option<WebsiteInfo>>,
    pub startup_message: serenity::MessageId,
    pub start_time: std::time::SystemTime,
    pub songbird: Arc<songbird::Songbird>,
    pub error_webhook: serenity::Webhook,
    pub bot_list_tokens: BotListTokens,
    pub premium_avatar_url: String,
    pub main_server_invite: String,
    pub reqwest: reqwest::Client,
    pub regex_cache: RegexCache,
    pub webhooks: WebhookConfig,
    pub config: MainConfig,
    pub pool: sqlx::PgPool,

    pub espeak_voices: Vec<String>,
    pub gtts_voices: BTreeMap<String, String>,
    pub polly_voices: BTreeMap<String, PollyVoice>,
    pub gcloud_voices: BTreeMap<String, BTreeMap<String, GoogleGender>>,

    pub translation_languages: BTreeMap<String, String>,
}

impl std::fmt::Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Data").finish()
    }
}

impl Data {
    pub fn default_catalog(&self) -> Option<&gettext::Catalog> {
        self.translations.get("en-US")
    }

    pub async fn speaking_rate(
        &self,
        user_id: serenity::UserId,
        mode: TTSMode,
    ) -> Result<Cow<'static, str>> {
        let row = self.user_voice_db.get((user_id.into(), mode)).await?;

        Ok(row.speaking_rate.map_or_else(
            || {
                mode.speaking_rate_info()
                    .map(|info| info.default.to_string())
                    .map_or(Cow::Borrowed("1.0"), Cow::Owned)
            },
            |r| Cow::Owned(r.to_string()),
        ))
    }

    pub async fn fetch_patreon_info(
        &self,
        user_id: serenity::UserId,
    ) -> Result<Option<PatreonInfo>> {
        if let Some(mut url) = self.config.patreon_service.clone() {
            url.set_path(&format!("/members/{user_id}"));

            let req = self.reqwest.get(url);
            let resp = req.send().await?.error_for_status()?.bytes().await?;
            json::from_slice(&resp).map_err(Into::into)
        } else {
            // Return fake PatreonInfo if `patreon_service` has not been set to simplify self-hosting.
            Ok(Some(PatreonInfo {
                tier: u8::MAX,
                entitled_servers: u8::MAX,
            }))
        }
    }

    pub async fn premium_check(
        &self,
        guild_id: Option<serenity::GuildId>,
    ) -> Result<Option<FailurePoint>> {
        let Some(guild_id) = guild_id else {
            return Ok(Some(FailurePoint::Guild));
        };

        let guild_row = self.guilds_db.get(guild_id.get() as i64).await?;
        if let Some(patreon_user_id) = guild_row.premium_user {
            if self.fetch_patreon_info(patreon_user_id).await?.is_some() {
                Ok(None)
            } else {
                Ok(Some(FailurePoint::NotSubscribed(patreon_user_id)))
            }
        } else {
            Ok(Some(FailurePoint::PremiumUser))
        }
    }

    pub async fn parse_user_or_guild(
        &self,
        author_id: serenity::UserId,
        guild_id: Option<serenity::GuildId>,
    ) -> Result<(Cow<'static, str>, TTSMode)> {
        let user_row = self.userinfo_db.get(author_id.into()).await?;
        let guild_is_premium = self.premium_check(guild_id).await?.is_none();
        let mut guild_row = None;

        let mut mode = {
            let user_mode = if guild_is_premium {
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

        if self.config.gtts_disabled && mode == TTSMode::gTTS {
            mode = TTSMode::eSpeak;
        }

        if mode.is_premium() && !guild_is_premium {
            mode = TTSMode::default();

            if user_row.voice_mode.map_or(false, TTSMode::is_premium) {
                warn!(
                    "User ID {author_id}'s normal voice mode is set to a premium mode! Resetting."
                );
                self.userinfo_db
                    .set_one(author_id.into(), "voice_mode", mode)
                    .await?;
            } else if let Some(guild_id) = guild_id
                && let Some(guild_row) = guild_row
                && guild_row.voice_mode.is_premium()
            {
                warn!("Guild ID {guild_id}'s voice mode is set to a premium mode without being premium! Resetting.");
                self.guilds_db
                    .set_one(guild_id.into(), "voice_mode", mode)
                    .await?;
            } else {
                warn!("Guild {guild_id:?} - User {author_id} has a mode set to premium without being premium!");
            }
        }

        let user_voice_row = self.user_voice_db.get((author_id.into(), mode)).await?;
        let voice =
            // Get user voice for user mode
            if user_voice_row.user_id.is_some() {
                user_voice_row.voice.map(|v| Cow::Owned(v.as_str().to_owned()))
            } else if let Some(guild_id) = guild_id {
                // Get default server voice for user mode
                let guild_voice_row = self.guild_voice_db.get((guild_id.into(), mode)).await?;
                if guild_voice_row.guild_id.is_some() {
                    Some(Cow::Owned(guild_voice_row.voice.as_str().to_owned()))
                } else {
                    None
                }
            } else {
                None
            }.unwrap_or_else(|| Cow::Borrowed(mode.default_voice()));

        Ok((voice, mode))
    }
}

#[derive(Clone, Copy)]
pub struct SpeakingRateInfo {
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub kind: &'static str,
}

impl SpeakingRateInfo {
    #[allow(clippy::unnecessary_wraps)]
    const fn new(min: f32, default: f32, max: f32, kind: &'static str) -> Option<Self> {
        Some(Self {
            min,
            max,
            default,
            kind,
        })
    }
}

#[derive(IntoStaticStr, sqlx::Type, Debug, Default, Hash, PartialEq, Eq, Copy, Clone)]
#[allow(non_camel_case_types)]
#[sqlx(rename_all = "lowercase")]
#[sqlx(type_name = "ttsmode")]
pub enum TTSMode {
    #[default]
    gTTS,
    Polly,
    eSpeak,
    gCloud,
}

impl TTSMode {
    pub async fn fetch_voices(
        self,
        mut tts_service: reqwest::Url,
        reqwest: &reqwest::Client,
        auth_key: Option<&str>,
    ) -> Result<reqwest::Response> {
        tts_service.set_path("voices");
        tts_service
            .query_pairs_mut()
            .append_pair("mode", self.into())
            .append_pair("raw", "true")
            .finish();

        reqwest
            .get(tts_service)
            .header("Authorization", auth_key.unwrap_or(""))
            .send()
            .await?
            .error_for_status()
            .map_err(Into::into)
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

    pub const fn speaking_rate_info(self) -> Option<SpeakingRateInfo> {
        match self {
            Self::gTTS => None,
            Self::gCloud => SpeakingRateInfo::new(0.25, 1.0, 4.0, "x"),
            Self::Polly => SpeakingRateInfo::new(10.0, 100.0, 500.0, "%"),
            Self::eSpeak => SpeakingRateInfo::new(100.0, 175.0, 400.0, " words per minute"),
        }
    }
}

into_static_display!(TTSMode);

#[derive(poise::ChoiceParameter, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum TTSModeChoice {
    // Name to show in slash command invoke               Aliases for prefix
    #[name = "Google Translate TTS (female) (default)"]
    #[name = "gtts"]
    gTTS,
    #[name = "eSpeak TTS (male)"]
    #[name = "espeak"]
    eSpeak,
    #[name = "⭐ gCloud TTS (changeable) ⭐"]
    #[name = "gcloud"]
    gCloud,
    #[name = "⭐ Amazon Polly TTS (changeable) ⭐"]
    #[name = "polly"]
    Polly,
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
#[serde(rename_all = "camelCase")]
pub struct GoogleVoice {
    pub name: String,
    #[serde(default)]
    pub ssml_gender: GoogleGender,
    pub language_codes: [String; 1],
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

#[derive(serde::Deserialize, IntoStaticStr, Copy, Clone, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum GoogleGender {
    Male,
    Female,
    #[serde(rename = "SSML_VOICE_GENDER_UNSPECIFIED")]
    #[default]
    Unspecified,
}

#[derive(serde::Deserialize, IntoStaticStr, Copy, Clone)]
pub enum PollyGender {
    Male,
    Female,
}

into_static_display!(GoogleGender);
into_static_display!(PollyGender);

fn deserialize_error_code<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TTSServiceErrorCode, D::Error> {
    u8::deserialize(deserializer).map(|code| match code {
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
pub type PrefixContext<'a> = poise::PrefixContext<'a, Data, CommandError>;
pub type ApplicationContext<'a> = poise::ApplicationContext<'a, Data, CommandError>;

pub type CommandError = Error;
pub type CommandResult<E = Error> = Result<(), E>;
pub type FrameworkContext<'a> = poise::FrameworkContext<'a, Data, CommandError>;
pub type LastToXsaidTracker =
    dashmap::DashMap<serenity::GuildId, (serenity::UserId, std::time::SystemTime)>;
