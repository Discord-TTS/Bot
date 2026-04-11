use serde::Serialize;
use small_fixed_array::FixedString;

use serenity::{all as serenity, small_fixed_array};

use crate::structs::TTSMode;

macro_rules! make_serializers {
    ($(fn $fn_name:ident($id:ty);)*) => {$(
        fn $fn_name<S: serde::Serializer>(val: &$id, serializer: S) -> Result<S::Ok, S::Error> {
            <$id>::get(*val).serialize(serializer)
        }
    )*};
}

make_serializers! {
    fn serialize_channel_id(serenity::ChannelId);
    fn serialize_guild_id(serenity::GuildId);
    fn serialize_user_id(serenity::UserId);
}

#[derive(serde::Serialize)]
pub struct WSMessageFrame<'a> {
    #[serde(serialize_with = "serialize_guild_id")]
    pub guild_id: serenity::GuildId,
    pub inner: WSMessage<'a>,
}

#[derive(serde::Serialize)]
pub enum WSMessage<'a> {
    QueueTTS(GetTTS),
    MoveVC(&'a WSConnectionInfo),
    ClearQueue,
    Leave,
}

#[derive(Debug, serde::Serialize)]
pub struct GetTTS {
    pub text: String,
    pub mode: TTSMode,
    #[serde(rename = "lang")]
    pub voice: std::borrow::Cow<'static, str>,
    #[serde(default)]
    pub speaking_rate: Option<f32>,
    pub max_length: Option<u16>,
    #[serde(default)]
    pub preferred_format: Option<FixedString<u8>>,
    #[serde(default)]
    pub translation_lang: Option<FixedString<u8>>,
}

#[derive(serde::Serialize)]
pub struct WSConnectionInfo {
    #[serde(serialize_with = "serialize_channel_id")]
    pub channel_id: serenity::ChannelId,
    pub endpoint: FixedString,
    #[serde(serialize_with = "serialize_guild_id")]
    pub guild_id: serenity::GuildId,
    pub session_id: FixedString,
    pub token: FixedString,
    #[serde(serialize_with = "serialize_user_id")]
    pub bot_id: serenity::UserId,
}
