use std::sync::Arc;

use gnomeutils::require_guild;
use poise::serenity_prelude as serenity;

use crate::structs::{Result, JoinVCToken, Context, TTSMode};
use crate::constants::{FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};


#[serenity::async_trait]
pub trait PoiseContextExt {
    async fn neutral_colour(&self) -> u32;
    fn author_vc(&self) -> Option<serenity::ChannelId>;
}

#[serenity::async_trait]
impl PoiseContextExt for Context<'_> {
    fn author_vc(&self) -> Option<serenity::ChannelId> {
        require_guild!(self, None)
            .voice_states
            .get(&self.author().id)
            .and_then(|vc| vc.channel_id)
    }

    async fn neutral_colour(&self) -> u32 {
        if let Some(guild_id) = self.guild_id() {
            let row = self.data().guilds_db.get(guild_id.get() as i64).await;
            if row.map(|row| row.voice_mode).map_or(false, TTSMode::is_premium) {
                return PREMIUM_NEUTRAL_COLOUR
            }
        }

        FREE_NEUTRAL_COLOUR
    }
}

#[serenity::async_trait]
pub trait SerenityContextExt {
    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>>;
}

#[serenity::async_trait]
impl SerenityContextExt for serenity::Context {
    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>> {
        let manager = songbird::get(self).await.unwrap();
        let (call, result) = manager.join(guild_id.0, channel_id).await;

        if let Err(err) = result {
            // On error, the Call is left in a semi-connected state.
            // We need to correct this by removing the call from the manager.
            drop(manager.leave(guild_id.0).await);
            Err(err.into())
        } else {
            Ok(call)
        }
    }
}
