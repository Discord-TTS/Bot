use arrayvec::ArrayString;
use typesize::derive::TypeSize;

use poise::serenity_prelude::{ChannelId, GuildId, RoleId, UserId};

use crate::structs::TTSMode;

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

macro_rules! set_flag_if {
    ($flags:ident, $flag:path, $value:expr) => {
        if ($value) {
            $flags |= $flag;
        }
    };
}

macro_rules! named_bitflags {
    (pub struct $name:ident: $flag_size:ident {
        $(const $flag_name:ident = $flag_value:expr;)*
    }) => {
        #[derive(TypeSize)]
        pub struct $name($flag_size);

        bitflags::bitflags! {
            impl $name: $flag_size {
                $(const $flag_name = $flag_value;)*
            }
        }

        impl $name {
            paste::paste! {
                $(
                    pub fn [<$flag_name:lower>](&self) -> bool {
                        self.contains(Self::$flag_name)
                    }
                )*
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "("))?;
                bitflags::parser::to_writer(self, &mut f)?;
                write!(f, ")")?;
                Ok(())
            }
        }
    };
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

named_bitflags! {
    pub struct GuildRowFlags: u8 {
        const XSAID =           0b000001;
        const AUTO_JOIN =       0b000010;
        const BOT_IGNORE =      0b000100;
        const TO_TRANSLATE =    0b001000;
        const REQUIRE_VOICE =   0b010000;
        const AUDIENCE_IGNORE = 0b100000;
    }
}

#[derive(Debug, TypeSize)]
pub struct GuildRow {
    pub flags: GuildRowFlags,
    pub channel: Option<ChannelId>,
    pub premium_user: Option<UserId>,
    pub required_role: Option<RoleId>,
    pub msg_length: u16,
    pub repeated_chars: u16,
    pub prefix: ArrayString<8>,
    pub target_lang: Option<ArrayString<8>>,
    pub required_prefix: Option<ArrayString<8>>,
    pub voice_mode: TTSMode,
}

impl Compact for GuildRowRaw {
    type Compacted = GuildRow;
    fn compact(self) -> Self::Compacted {
        let mut flags = GuildRowFlags::empty();
        set_flag_if!(flags, GuildRowFlags::XSAID, self.xsaid);
        set_flag_if!(flags, GuildRowFlags::AUTO_JOIN, self.auto_join);
        set_flag_if!(flags, GuildRowFlags::BOT_IGNORE, self.bot_ignore);
        set_flag_if!(flags, GuildRowFlags::TO_TRANSLATE, self.to_translate);
        set_flag_if!(flags, GuildRowFlags::REQUIRE_VOICE, self.require_voice);
        set_flag_if!(flags, GuildRowFlags::AUDIENCE_IGNORE, self.audience_ignore);

        Self::Compacted {
            flags,
            channel: (self.channel != 0).then(|| ChannelId::new(self.channel as u64)),
            premium_user: self.premium_user.map(|id| UserId::new(id as u64)),
            required_role: self.required_role.map(|id| RoleId::new(id as u64)),
            msg_length: self.msg_length as u16,
            repeated_chars: self.repeated_chars as u16,
            prefix: truncate_convert(self.prefix, "guild.prefix"),
            target_lang: self
                .target_lang
                .map(|t| truncate_convert(t, "guild.target_lang")),
            required_prefix: self
                .required_prefix
                .map(|t| truncate_convert(t, "guild.required_prefix")),
            voice_mode: self.voice_mode,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct UserRowRaw {
    pub dm_blocked: bool,
    pub dm_welcomed: bool,
    pub voice_mode: Option<TTSMode>,
    pub premium_voice_mode: Option<TTSMode>,
}

named_bitflags! {
    pub struct UserRowFlags: u8 {
        const DM_BLOCKED =  0b01;
        const DM_WELCOMED = 0b10;
    }
}

#[derive(Debug, TypeSize)]
pub struct UserRow {
    pub flags: UserRowFlags,
    pub voice_mode: Option<TTSMode>,
    pub premium_voice_mode: Option<TTSMode>,
}

impl Compact for UserRowRaw {
    type Compacted = UserRow;
    fn compact(self) -> Self::Compacted {
        let mut flags = UserRowFlags::empty();
        set_flag_if!(flags, UserRowFlags::DM_BLOCKED, self.dm_blocked);
        set_flag_if!(flags, UserRowFlags::DM_WELCOMED, self.dm_welcomed);

        Self::Compacted {
            flags,
            voice_mode: self.voice_mode,
            premium_voice_mode: self.premium_voice_mode,
        }
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
