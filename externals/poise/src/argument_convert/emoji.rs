//! Contains implementations of [`ArgumentConvert`] for [`serenity::Emoji`] and
//! [`serenity::EmojiId`].

use std::str::FromStr;

use super::ArgumentConvert;
use crate::serenity_prelude as serenity;

#[async_trait::async_trait]
impl ArgumentConvert for serenity::EmojiId {
    type Err = <serenity::EmojiId as FromStr>::Err;

    async fn convert(
        _: impl serenity::CacheHttp,
        _: Option<serenity::GuildId>,
        _: Option<serenity::GenericChannelId>,
        s: &str,
    ) -> Result<Self, Self::Err> {
        s.parse()
    }
}
