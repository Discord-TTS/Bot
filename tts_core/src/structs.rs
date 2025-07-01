use std::{
    borrow::Cow,
    collections::BTreeMap,
    num::NonZeroU8,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};

use aformat::{aformat, ArrayString, CapStr};
pub use anyhow::{Error, Result};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::Deserialize as _;
use strum_macros::IntoStaticStr;
use tracing::warn;
use typesize::derive::TypeSize;

use poise::serenity_prelude::{
    self as serenity,
    small_fixed_array::{FixedArray, FixedString},
    ChannelId, GuildId, RoleId, SkuId, UserId,
};

use crate::{analytics, bool_enum, common::timestamp_in_future, database};

macro_rules! into_static_display {
    ($struct:ident, max_length($len:literal)) => {
        impl aformat::ToArrayString for $struct {
            const MAX_LENGTH: usize = $len;
            type ArrayString = arrayvec::ArrayString<$len>;

            fn to_arraystring(self) -> Self::ArrayString {
                arrayvec::ArrayString::from(self.into()).unwrap()
            }
        }

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
    #[serde(rename = "Premium-Info")]
    pub premium: Option<PremiumConfig>,
    #[serde(rename = "Bot-List-Tokens")]
    pub bot_list_tokens: Option<BotListTokens>,
}

#[derive(serde::Deserialize)]
pub struct MainConfig {
    pub tts_service_auth_key: Option<FixedString>,
    pub website_url: Option<reqwest::Url>,
    pub announcements_channel: ChannelId,
    pub main_server_invite: FixedString,
    pub proxy_url: Option<FixedString>,
    pub invite_channel: ChannelId,
    pub tts_service: reqwest::Url,
    pub token: serenity::Token,
    pub main_server: GuildId,
    pub ofs_role: RoleId,

    // Only for situations where gTTS has broken
    #[serde(default)]
    pub gtts_disabled: AtomicBool,
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

#[derive(serde::Deserialize)]
pub struct PremiumConfig {
    pub discord_monetisation_enabled: Option<bool>,
    pub patreon_page_url: ArrayString<64>,
    pub patreon_service: reqwest::Url,
    pub basic_sku: SkuId,
    pub extra_sku: SkuId,
}

pub struct WebhookConfig {
    pub logs: serenity::Webhook,
    pub errors: serenity::Webhook,
    pub dm_logs: serenity::Webhook,
}

pub struct JoinVCToken(pub GuildId, pub Arc<tokio::sync::Mutex<()>>);
impl JoinVCToken {
    pub fn acquire(data: &Data, guild_id: GuildId) -> Self {
        let lock = data
            .join_vc_tokens
            .entry(guild_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        Self(guild_id, lock)
    }
}

bool_enum!(IsPremium(No | Yes));

#[derive(Clone, Copy)]
pub enum FailurePoint {
    NotSubscribed(UserId),
    PremiumUser,
    Guild,
}

#[derive(serde::Deserialize, Clone, Copy)]
pub struct PremiumInfo {
    pub entitled_servers: NonZeroU8,
}

impl PremiumInfo {
    fn fake() -> Self {
        Self {
            entitled_servers: NonZeroU8::MAX,
        }
    }
}

#[derive(Clone, Copy)]
pub enum CachedEntitlement {
    Normal(PremiumInfo, serenity::Timestamp),
    NoExpiry(PremiumInfo),
    None,
}

impl CachedEntitlement {
    fn as_premium_info(self) -> Option<PremiumInfo> {
        match self {
            CachedEntitlement::Normal(premium_info, timestamp)
                if timestamp_in_future(timestamp) =>
            {
                Some(premium_info)
            }
            CachedEntitlement::NoExpiry(premium_info) => Some(premium_info),
            CachedEntitlement::Normal(..) | CachedEntitlement::None => None,
        }
    }
}

pub struct RegexCache {
    pub replacements: [(regex::Regex, &'static str); 3],
    pub bot_mention: OnceLock<regex::Regex>,
    pub id_in_brackets: regex::Regex,
    pub emoji_captures: regex::Regex,
    pub emoji_filter: regex::Regex,
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
            emoji_captures: regex::Regex::new(r"<(a?):([^<>]+):\d+>")?,
            emoji_filter: regex::Regex::new(r"(?s:<a?:[^<>]+:\d+>)|\p{Emoji}")?,
            bot_mention: OnceLock::new(),
        })
    }
}

#[derive(Clone, Copy)]
pub struct LastXsaidInfo(UserId, std::time::SystemTime);

impl LastXsaidInfo {
    fn get_vc_member_count(guild: &serenity::Guild, channel_id: ChannelId) -> usize {
        guild
            .voice_states
            .iter()
            .filter(|vs| vs.channel_id.is_some_and(|vc| vc == channel_id))
            .filter_map(|vs| guild.members.get(&vs.user_id))
            .filter(|member| !member.user.bot())
            .count()
    }

    #[must_use]
    pub fn new(user: UserId) -> Self {
        Self(user, std::time::SystemTime::now())
    }

    #[must_use]
    pub fn should_announce_name(&self, guild: &serenity::Guild, user: UserId) -> bool {
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

    pub entitlement_cache: mini_moka::sync::Cache<UserId, CachedEntitlement>,
    pub join_vc_tokens: DashMap<GuildId, Arc<tokio::sync::Mutex<()>>>,
    pub last_to_xsaid_tracker: LastToXsaidTracker,
    pub startup_message: serenity::MessageId,
    pub premium_avatar_url: FixedString<u16>,
    pub system_info: Mutex<sysinfo::System>,
    pub start_time: std::time::SystemTime,
    pub reqwest: reqwest::Client,
    pub regex_cache: RegexCache,
    pub webhooks: WebhookConfig,
    pub pool: sqlx::PgPool,

    pub songbird: Arc<songbird::Songbird>,

    pub config: MainConfig,
    pub premium_config: Option<PremiumConfig>,

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
    #[expect(clippy::disallowed_methods, reason = "We are Data::leave_vc")]
    pub async fn leave_vc(&self, guild_id: serenity::GuildId) -> songbird::error::JoinResult<()> {
        self.last_to_xsaid_tracker.remove(&guild_id);
        self.songbird.remove(guild_id).await
    }

    pub async fn speaking_rate(&self, user_id: UserId, mode: TTSMode) -> Result<Cow<'static, str>> {
        let row = self.user_voice_db.get((user_id.into(), mode)).await?;

        Ok(match row.speaking_rate {
            Some(r) => Cow::Owned(r.to_string()),
            None => Cow::Borrowed(
                mode.speaking_rate_info()
                    .map(|info| info.default)
                    .unwrap_or("1.0"),
            ),
        })
    }

    async fn fetch_patreon_info(&self, user_id: UserId) -> Result<Option<PremiumInfo>> {
        if let Some(config) = &self.premium_config {
            let mut url = config.patreon_service.clone();
            url.set_path(&aformat!("/members/{user_id}"));

            let req = self.reqwest.get(url);
            let resp = req.send().await?.error_for_status()?.json().await?;
            Ok(resp)
        } else {
            // Return fake PatreonInfo if `patreon_service` has not been set to simplify self-hosting.
            Ok(Some(PremiumInfo::fake()))
        }
    }

    fn get_cached_discord_premium(&self, user_id: UserId) -> Option<PremiumInfo> {
        if let Some(cached_entitlement) = self.entitlement_cache.get(&user_id) {
            if let Some(premium_info) = cached_entitlement.as_premium_info() {
                return Some(premium_info);
            }
        }

        None
    }

    async fn fetch_entitlement_for_user(
        &self,
        http: &serenity::Http,
        user_id: UserId,
    ) -> Result<Option<PremiumInfo>> {
        let Some(premium_config) = &self.premium_config else {
            // Should not be reached, but return fake anyway to simplify self-hosting.
            return Ok(Some(PremiumInfo::fake()));
        };

        let entitlements = http
            .get_entitlements(Some(user_id), None, None, None, None, None, Some(true))
            .await?;

        let cached_entitlement = if let Some(entitlement) = entitlements
            .into_iter()
            .find(|entitlement| entitlement.ends_at.is_none_or(timestamp_in_future))
        {
            let entitled_servers = if entitlement.sku_id == premium_config.extra_sku {
                NonZeroU8::new(5).unwrap()
            } else if entitlement.sku_id == premium_config.basic_sku {
                NonZeroU8::new(2).unwrap()
            } else {
                anyhow::bail!("Found unknown entitlement sku: {}", entitlement.sku_id);
            };

            let premium_info = PremiumInfo { entitled_servers };
            match entitlement.ends_at {
                Some(expiry) => CachedEntitlement::Normal(premium_info, expiry),
                None => CachedEntitlement::NoExpiry(premium_info),
            }
        } else {
            CachedEntitlement::None
        };

        self.entitlement_cache.insert(user_id, cached_entitlement);
        Ok(cached_entitlement.as_premium_info())
    }

    pub async fn fetch_premium_info(
        &self,
        http: &serenity::Http,
        user_id: UserId,
    ) -> Result<Option<PremiumInfo>> {
        if let Some(premium_info) = self.get_cached_discord_premium(user_id) {
            return Ok(Some(premium_info));
        }

        if let Some(premium_info) = self.fetch_patreon_info(user_id).await? {
            return Ok(Some(premium_info));
        }

        if let Some(premium_info) = self.fetch_entitlement_for_user(http, user_id).await? {
            return Ok(Some(premium_info));
        }

        Ok(None)
    }

    pub async fn premium_check(
        &self,
        http: &serenity::Http,
        guild_id: Option<GuildId>,
    ) -> Result<Option<FailurePoint>> {
        let Some(guild_id) = guild_id else {
            return Ok(Some(FailurePoint::Guild));
        };

        let guild_row = self.guilds_db.get(guild_id.get() as i64).await?;
        let Some(premium_user) = guild_row.premium_user else {
            return Ok(Some(FailurePoint::PremiumUser));
        };

        if self.fetch_premium_info(http, premium_user).await?.is_some() {
            Ok(None)
        } else {
            Ok(Some(FailurePoint::PremiumUser))
        }
    }

    pub async fn is_premium_simple(
        &self,
        http: &serenity::Http,
        guild_id: GuildId,
    ) -> Result<bool> {
        let guild_id = Some(guild_id);
        self.premium_check(http, guild_id)
            .await
            .map(|o| o.is_none())
    }

    pub async fn parse_user_or_guild(
        &self,
        http: &serenity::Http,
        author_id: UserId,
        guild_id: Option<GuildId>,
    ) -> Result<(Cow<'static, str>, TTSMode)> {
        let info = if let Some(guild_id) = guild_id {
            Some((guild_id, self.is_premium_simple(http, guild_id).await?))
        } else {
            None
        };

        self.parse_user_or_guild_with_premium(author_id, info).await
    }

    pub async fn parse_user_or_guild_with_premium(
        &self,
        author_id: UserId,
        guild_info: Option<(GuildId, bool)>,
    ) -> Result<(Cow<'static, str>, TTSMode)> {
        let user_row = self.userinfo_db.get(author_id.into()).await?;
        let (guild_id, guild_is_premium) = match guild_info {
            Some((id, p)) => (Some(id), p),
            None => (None, false),
        };

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

        if self.config.gtts_disabled.load(Ordering::Relaxed) && mode == TTSMode::gTTS {
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
    #[expect(clippy::unnecessary_wraps)]
    const fn new(min: f32, default: &'static str, max: f32, kind: &'static str) -> Option<Self> {
        Some(Self {
            min,
            max,
            default,
            kind,
        })
    }

    #[must_use]
    pub fn kind(&self) -> CapStr<'static, 32> {
        CapStr(self.kind)
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
    #[must_use]
    pub const fn is_premium(self) -> bool {
        match self {
            Self::gTTS | Self::eSpeak => false,
            Self::Polly | Self::gCloud => true,
        }
    }

    #[must_use]
    pub const fn default_voice(self) -> &'static str {
        match self {
            Self::gTTS => "en",
            Self::eSpeak => "en1",
            Self::Polly => "Brian",
            Self::gCloud => "en-US A",
        }
    }

    #[must_use]
    pub const fn speaking_rate_info(self) -> Option<SpeakingRateInfo> {
        match self {
            Self::gTTS => None,
            Self::gCloud => SpeakingRateInfo::new(0.25, "1.0", 4.0, "x"),
            Self::Polly => SpeakingRateInfo::new(10.0, "100.0", 500.0, "%"),
            Self::eSpeak => SpeakingRateInfo::new(100.0, "175.0", 400.0, " words per minute"),
        }
    }
}

into_static_display!(TTSMode, max_length(6));

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

into_static_display!(GoogleGender, max_length(6));
into_static_display!(PollyGender, max_length(11));

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

#[derive(Debug, Clone, Copy)]
pub enum TTSServiceErrorCode {
    Unknown,
    UnknownVoice,
    AudioTooLong,
    InvalidSpeakingRate,
}

impl TTSServiceErrorCode {
    #[must_use]
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
pub type LastToXsaidTracker = DashMap<GuildId, LastXsaidInfo>;
pub type FrameworkContext<'a> = poise::FrameworkContext<'a, Data, CommandError>;
