use std::sync::Arc;

use lazy_static::lazy_static;
use regex::Regex;

use poise::serenity_prelude as serenity;

use crate::structs::{Result, JoinVCToken, Context, TTSMode};
use crate::constants::{FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};


#[serenity::async_trait]
pub trait PoiseContextExt {
    async fn neutral_colour(&self) -> u32;
}

#[serenity::async_trait]
impl PoiseContextExt for Context<'_> {
    async fn neutral_colour(&self) -> u32 {
        if let Some(guild_id) = self.guild_id() {
            let row = self.data().guilds_db.get(guild_id.0 as i64).await;
            if row.map(|row| row.voice_mode).map_or(false, TTSMode::is_premium) {
                return PREMIUM_NEUTRAL_COLOUR
            }
        }

        FREE_NEUTRAL_COLOUR
    }
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
