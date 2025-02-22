use std::sync::Arc;

use tokio::sync::Mutex;

use poise::serenity_prelude as serenity;
use serenity::GuildId;

use crate::structs::Data;

type LockedCall = Arc<Mutex<songbird::Call>>;

pub struct JoinVCToken(pub GuildId, pub Arc<Mutex<()>>);
impl JoinVCToken {
    pub fn acquire(data: &Data, guild_id: GuildId) -> Self {
        let lock = data
            .join_vc_tokens
            .entry(guild_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        Self(guild_id, lock)
    }
}

pub struct SpeakingEventHandler {
    call_lock: LockedCall,
}

#[serenity::async_trait]
impl songbird::EventHandler for SpeakingEventHandler {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        let songbird::EventContext::VoiceTick(tick) = ctx else {
            return None;
        };

        let mut call = self.call_lock.lock().await;
        match (call.is_mute(), tick.speaking.is_empty()) {
            (true, true) => {
                println!("yapping");
                call.mute(false).await.ok()
            }
            (false, false) => {
                println!("shutting up");
                call.mute(true).await.ok()
            }

            _ => None,
        };

        None
    }
}

pub trait ManagerExt {
    async fn join_vc(
        &self,
        guild_id: JoinVCToken,
        channel_id: serenity::ChannelId,
    ) -> Result<LockedCall, songbird::error::JoinError>;
}

impl ManagerExt for songbird::Songbird {
    async fn join_vc(
        &self,
        JoinVCToken(guild_id, lock): JoinVCToken,
        channel_id: serenity::ChannelId,
    ) -> Result<LockedCall, songbird::error::JoinError> {
        let _guard = lock.lock().await;
        match self.join(guild_id, channel_id).await {
            Ok(call) => {
                call.lock().await.add_global_event(
                    songbird::Event::Core(songbird::CoreEvent::VoiceTick),
                    SpeakingEventHandler {
                        call_lock: Arc::clone(&call),
                    },
                );

                Ok(call)
            }
            Err(err) => {
                // On error, the Call is left in a semi-connected state.
                // We need to correct this by removing the call from the manager.
                drop(self.leave(guild_id).await);
                Err(err)
            }
        }
    }
}
