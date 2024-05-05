use std::num::NonZeroU8;

use arrayvec::ArrayString;
use typesize::derive::TypeSize;

use poise::serenity_prelude::{ChannelId, GuildId, RoleId, UserId};

use crate::structs::{IsPremium, TTSMode};

const MAX_VOICE_LENGTH: usize = 20;

fn truncate_convert<const MAX_SIZE: usize>(
    mut s: String,
    field_name: &'static str,
) -> ArrayString<MAX_SIZE> {
    if s.len() > MAX_SIZE {
        tracing::warn!("Max size of database field {field_name} reached!");
        s.truncate(MAX_SIZE);
    }

    ArrayString::from(&s).expect("Truncate to shrink to below the max size!")
}

pub trait Compact {
    type Compacted;
    fn compact(self) -> Self::Compacted;
}

#[allow(clippy::struct_excessive_bools)]
#[derive(sqlx::FromRow)]
pub struct GuildRowRaw {
    pub channel: i64,
    pub premium_user: Option<i64>,
    pub required_role: Option<i64>,
    pub xsaid: bool,
    pub auto_join: bool,
    pub bot_ignore: bool,
    pub to_translate: bool,
    pub require_voice: bool,
    pub audience_ignore: bool,
    pub msg_length: i16,
    pub repeated_chars: i16,
    pub prefix: String,
    pub target_lang: Option<String>,
    pub required_prefix: Option<String>,
    pub voice_mode: TTSMode,
}

#[bool_to_bitflags::bool_to_bitflags(owning_setters)]
#[derive(Debug, typesize::derive::TypeSize)]
pub struct GuildRow {
    pub channel: Option<ChannelId>,
    pub premium_user: Option<UserId>,
    pub required_role: Option<RoleId>,
    pub xsaid: bool,
    pub auto_join: bool,
    pub bot_ignore: bool,
    pub to_translate: bool,
    pub require_voice: bool,
    pub audience_ignore: bool,
    pub msg_length: u16,
    pub repeated_chars: Option<NonZeroU8>,
    pub prefix: ArrayString<8>,
    pub target_lang: Option<ArrayString<8>>,
    pub required_prefix: Option<ArrayString<8>>,
    pub voice_mode: TTSMode,
}

impl GuildRow {
    pub fn target_lang(&self, is_premium: IsPremium) -> Option<&str> {
        if let Some(target_lang) = &self.target_lang
            && self.to_translate()
            && is_premium.into()
        {
            Some(target_lang.as_str())
        } else {
            None
        }
    }
}

impl Compact for GuildRowRaw {
    type Compacted = GuildRow;
    fn compact(self) -> Self::Compacted {
        Self::Compacted {
            __generated_flags: GuildRowGeneratedFlags::empty(),
            channel: (self.channel != 0).then(|| ChannelId::new(self.channel as u64)),
            premium_user: self.premium_user.map(|id| UserId::new(id as u64)),
            required_role: self.required_role.map(|id| RoleId::new(id as u64)),
            msg_length: self.msg_length as u16,
            repeated_chars: NonZeroU8::new(self.repeated_chars as u8),
            prefix: truncate_convert(self.prefix, "guild.prefix"),
            target_lang: self
                .target_lang
                .map(|t| truncate_convert(t, "guild.target_lang")),
            required_prefix: self
                .required_prefix
                .map(|t| truncate_convert(t, "guild.required_prefix")),
            voice_mode: self.voice_mode,
        }
        .set_xsaid(self.xsaid)
        .set_auto_join(self.auto_join)
        .set_bot_ignore(self.bot_ignore)
        .set_to_translate(self.to_translate)
        .set_require_voice(self.require_voice)
        .set_audience_ignore(self.audience_ignore)
    }
}

#[derive(sqlx::FromRow)]
pub struct UserRowRaw {
    pub dm_blocked: bool,
    pub dm_welcomed: bool,
    pub bot_banned: bool,
    pub use_new_formatting: bool,
    pub voice_mode: Option<TTSMode>,
    pub premium_voice_mode: Option<TTSMode>,
}

#[bool_to_bitflags::bool_to_bitflags(owning_setters)]
#[derive(Debug, typesize::derive::TypeSize)]
pub struct UserRow {
    pub dm_blocked: bool,
    pub dm_welcomed: bool,
    pub bot_banned: bool,
    pub use_new_formatting: bool,
    pub voice_mode: Option<TTSMode>,
    pub premium_voice_mode: Option<TTSMode>,
}

impl Compact for UserRowRaw {
    type Compacted = UserRow;
    fn compact(self) -> Self::Compacted {
        Self::Compacted {
            voice_mode: self.voice_mode,
            premium_voice_mode: self.premium_voice_mode,
            __generated_flags: UserRowGeneratedFlags::empty(),
        }
        .set_dm_blocked(self.dm_blocked)
        .set_dm_welcomed(self.dm_welcomed)
        .set_bot_banned(self.bot_banned)
        .set_use_new_formatting(self.use_new_formatting)
    }
}

#[derive(sqlx::FromRow)]
pub struct GuildVoiceRowRaw {
    pub guild_id: i64,
    pub mode: TTSMode,
    pub voice: String,
}

#[derive(Debug, TypeSize)]

pub struct GuildVoiceRow {
    pub guild_id: Option<GuildId>,
    pub mode: TTSMode,
    pub voice: ArrayString<MAX_VOICE_LENGTH>,
}

impl Compact for GuildVoiceRowRaw {
    type Compacted = GuildVoiceRow;
    fn compact(self) -> Self::Compacted {
        Self::Compacted {
            guild_id: (self.guild_id != 0).then(|| GuildId::new(self.guild_id as u64)),
            mode: self.mode,
            voice: truncate_convert(self.voice, "guildvoicerow.voice"),
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct UserVoiceRowRaw {
    pub user_id: i64,
    pub mode: TTSMode,
    pub voice: Option<String>,
    pub speaking_rate: Option<f32>,
}

#[derive(Debug, TypeSize)]
pub struct UserVoiceRow {
    pub user_id: Option<UserId>,
    pub mode: TTSMode,
    pub voice: Option<ArrayString<MAX_VOICE_LENGTH>>,
    pub speaking_rate: Option<f32>,
}

impl Compact for UserVoiceRowRaw {
    type Compacted = UserVoiceRow;
    fn compact(self) -> Self::Compacted {
        Self::Compacted {
            user_id: (self.user_id != 0).then(|| UserId::new(self.user_id as u64)),
            mode: self.mode,
            voice: self
                .voice
                .map(|v| truncate_convert(v, "uservoicerow.voice")),
            speaking_rate: self.speaking_rate,
        }
    }
}

#[derive(Debug, TypeSize, sqlx::FromRow)]
pub struct NicknameRow {
    pub name: Option<String>,
}

pub type NicknameRowRaw = NicknameRow;

impl Compact for NicknameRowRaw {
    type Compacted = NicknameRow;
    fn compact(self) -> Self::Compacted {
        self
    }
}
