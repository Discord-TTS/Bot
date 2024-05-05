use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
};

pub use anyhow::{Error, Result};
use parking_lot::Mutex;
use serde::Deserialize as _;
use strum_macros::IntoStaticStr;
use tracing::warn;
use typesize::derive::TypeSize;

use poise::serenity_prelude::{self as serenity};
use serenity::small_fixed_array::{FixedArray, FixedString};

use crate::{analytics, bool_enum, database};

macro_rules! into_static_display {
    ($struct:ident) => {
        impl std::fmt::Display for $struct {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.into())
            }
        }
    };
}

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
    pub bot_list_tokens: Option<BotListTokens>,
}

#[derive(serde::Deserialize)]
pub struct MainConfig {
    pub announcements_channel: serenity::ChannelId,
    pub tts_service_auth_key: Option<FixedString>,
    pub patreon_service: Option<reqwest::Url>,
    pub invite_channel: serenity::ChannelId,
    pub website_url: Option<reqwest::Url>,
    pub main_server_invite: FixedString,
    pub main_server: serenity::GuildId,
    pub ofs_role: serenity::RoleId,
    pub tts_service: reqwest::Url,
    pub token: Option<FixedString>,

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

#[derive(serde::Deserialize)]
pub struct BotListTokens {
    pub top_gg: FixedString,
    pub discord_bots_gg: FixedString,
    pub bots_on_discord: FixedString,
}

pub struct WebhookConfig {
    pub logs: serenity::Webhook,
    pub errors: serenity::Webhook,
    pub dm_logs: serenity::Webhook,
}

pub struct JoinVCToken(pub serenity::GuildId, pub Arc<tokio::sync::Mutex<()>>);
impl JoinVCToken {
    pub fn acquire(data: &Data, guild_id: serenity::GuildId) -> Self {
        let lock = data
            .join_vc_tokens
            .entry(guild_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        Self(guild_id, lock)
    }
}

bool_enum!(IsPremium(No | Yes));

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
    pub bot_mention: OnceLock<regex::Regex>,
    pub id_in_brackets: regex::Regex,
    pub emoji: regex::Regex,
}

impl RegexCache {
    pub fn new() -> Result<Self> {
        Ok(Self {
            replacements: [
                (
                    regex::Regex::new(r"\|\|(?s:.)*?\|\|")?,
                    ". spoiler avoided.",
                ),
                (regex::Regex::new(r"```(?s:.)*?```")?, ". code block."),
                (regex::Regex::new(r"`(?s:.)*?`")?, ". code snippet."),
            ],
            id_in_brackets: regex::Regex::new(r"\((\d+)\)")?,
            emoji: regex::Regex::new(r"<(a?):([^<>]+):\d+>")?,
            bot_mention: OnceLock::new(),
        })
    }
}

pub struct LastXsaidInfo(serenity::UserId, std::time::SystemTime);

impl LastXsaidInfo {
    fn get_vc_member_count(guild: &serenity::Guild, channel_id: serenity::ChannelId) -> usize {
        guild
            .voice_states
            .iter()
            .filter(|vs| vs.channel_id.is_some_and(|vc| vc == channel_id))
            .filter_map(|vs| guild.members.get(&vs.user_id))
            .filter(|member| !member.user.bot())
            .count()
    }

    pub fn new(user: serenity::UserId) -> Self {
        Self(user, std::time::SystemTime::now())
    }

    pub fn should_announce_name(&self, guild: &serenity::Guild, user: serenity::UserId) -> bool {
        let Some(voice_channel_id) = guild.voice_states.get(&user).and_then(|v| v.channel_id)
        else {
            return true;
        };

        if user != self.0 {
            return true;
        }

        let has_been_min = self.1.elapsed().unwrap().as_secs() > 60;
        let is_only_author = Self::get_vc_member_count(guild, voice_channel_id) <= 1;

        has_been_min || is_only_author
    }
}

pub struct Data {
    pub analytics: Arc<analytics::Handler>,
    pub guilds_db: database::Handler<i64, database::GuildRowRaw>,
    pub userinfo_db: database::Handler<i64, database::UserRowRaw>,
    pub nickname_db: database::Handler<[i64; 2], database::NicknameRowRaw>,
    pub user_voice_db: database::Handler<(i64, TTSMode), database::UserVoiceRowRaw>,
    pub guild_voice_db: database::Handler<(i64, TTSMode), database::GuildVoiceRowRaw>,

    pub join_vc_tokens: dashmap::DashMap<serenity::GuildId, Arc<tokio::sync::Mutex<()>>>,
    pub translations: HashMap<FixedString<u8>, gettext::Catalog>,
    pub last_to_xsaid_tracker: LastToXsaidTracker,
    pub startup_message: serenity::MessageId,
    pub premium_avatar_url: FixedString<u16>,
    pub system_info: Mutex<sysinfo::System>,
    pub start_time: std::time::SystemTime,
    pub songbird: Arc<songbird::Songbird>,
    pub reqwest: reqwest::Client,
    pub regex_cache: RegexCache,
    pub webhooks: WebhookConfig,
    pub config: MainConfig,
    pub pool: sqlx::PgPool,

    // Startup information
    pub website_info: Mutex<Option<WebsiteInfo>>,
    pub bot_list_tokens: Mutex<Option<BotListTokens>>,
    pub fully_started: std::sync::atomic::AtomicBool,
    pub update_startup_lock: tokio::sync::Mutex<()>,

    pub espeak_voices: FixedArray<FixedString<u8>>,
    pub gtts_voices: BTreeMap<FixedString<u8>, FixedString<u8>>,
    pub polly_voices: BTreeMap<FixedString<u8>, PollyVoice>,
    pub gcloud_voices: BTreeMap<FixedString<u8>, BTreeMap<FixedString<u8>, GoogleGender>>,

    pub translation_languages: BTreeMap<FixedString<u8>, FixedString<u8>>,
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
                Cow::Borrowed(
                    mode.speaking_rate_info()
                        .map(|info| info.default)
                        .unwrap_or("1.0"),
                )
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
            let resp = req.send().await?.error_for_status()?.json().await?;
            Ok(resp)
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

    pub async fn is_premium_simple(&self, guild_id: serenity::GuildId) -> Result<bool> {
        let guild_id = Some(guild_id);
        self.premium_check(guild_id).await.map(|o| o.is_none())
    }

    pub async fn parse_user_or_guild(
        &self,
        author_id: serenity::UserId,
        guild_id: Option<serenity::GuildId>,
    ) -> Result<(Cow<'static, str>, TTSMode)> {
        let info = if let Some(guild_id) = guild_id {
            Some((guild_id, self.is_premium_simple(guild_id).await?))
        } else {
            None
        };

        self.parse_user_or_guild_with_premium(author_id, info).await
    }

    pub async fn parse_user_or_guild_with_premium(
        &self,
        author_id: serenity::UserId,
        guild_info: Option<(serenity::GuildId, bool)>,
    ) -> Result<(Cow<'static, str>, TTSMode)> {
        let user_row = self.userinfo_db.get(author_id.into()).await?;
        let (guild_id, guild_is_premium) =
            guild_info.map_or((None, false), |(id, p)| (Some(id), p));

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

            if user_row.voice_mode.is_some_and(TTSMode::is_premium) {
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
    pub default: &'static str,
    pub kind: &'static str,
}

impl SpeakingRateInfo {
    #[allow(clippy::unnecessary_wraps)]
    const fn new(min: f32, default: &'static str, max: f32, kind: &'static str) -> Option<Self> {
        Some(Self {
            min,
            max,
            default,
            kind,
        })
    }
}

#[derive(IntoStaticStr, sqlx::Type, TypeSize, Debug, Default, Hash, PartialEq, Eq, Copy, Clone)]
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
            Self::gCloud => SpeakingRateInfo::new(0.25, "1.0", 4.0, "x"),
            Self::Polly => SpeakingRateInfo::new(10.0, "100.0", 500.0, "%"),
            Self::eSpeak => SpeakingRateInfo::new(100.0, "175.0", 400.0, " words per minute"),
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
    pub name: FixedString<u8>,
    #[serde(default)]
    pub ssml_gender: GoogleGender,
    pub language_codes: [FixedString<u8>; 1],
}

#[derive(serde::Deserialize)]
pub struct PollyVoice {
    pub additional_language_codes: Option<FixedArray<FixedString>>,
    pub language_code: FixedString,
    pub language_name: FixedString,
    pub gender: PollyGender,
    pub name: FixedString<u8>,
    pub id: FixedString<u8>,
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
pub type PartialContext<'a> = poise::PartialContext<'a, Data, CommandError>;
pub type ApplicationContext<'a> = poise::ApplicationContext<'a, Data, CommandError>;

pub type CommandError = Error;
pub type CommandResult<E = Error> = Result<(), E>;
pub type FrameworkContext<'a> = poise::FrameworkContext<'a, Data, CommandError>;
pub type LastToXsaidTracker = dashmap::DashMap<serenity::GuildId, LastXsaidInfo>;
