#![allow(clippy::module_name_repetitions)]
#![feature(let_chains)]

mod channel;
mod guild;
mod member;
mod message;
mod other;
mod ready;
mod voice_state;

use poise::serenity_prelude as serenity;

use tts_core::errors;

#[must_use]
pub fn get_intents() -> serenity::GatewayIntents {
    serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::GUILD_VOICE_STATES
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::MESSAGE_CONTENT
}

#[derive(Clone, Copy)]
pub struct EventHandler;

#[serenity::async_trait]
impl serenity::EventHandler for EventHandler {
    async fn dispatch(&self, ctx: &serenity::Context, event: &serenity::FullEvent) {
        match event {
            serenity::FullEvent::Message { new_message, .. } => {
                if let Err(err) = message::handle(&ctx, &new_message).await {
                    if let Err(err) = errors::handle_message(&ctx, &new_message, err).await {
                        tracing::error!("Error in message event handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::Ready { data_about_bot, .. } => {
                if let Err(err) = ready::handle(&ctx, data_about_bot).await {
                    if let Err(err) = errors::handle_unexpected_default(&ctx, "Ready", err).await {
                        tracing::error!("Error in message event handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::GuildCreate { guild, is_new, .. } => {
                if let Err(err) = guild::handle_create(&ctx, &guild, *is_new).await {
                    if let Err(err) =
                        errors::handle_guild("GuildCreate", &ctx, Some(&guild), err).await
                    {
                        tracing::error!("Error in guild create handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::GuildDelete {
                incomplete, full, ..
            } => {
                if let Err(err) = guild::handle_delete(&ctx, *incomplete, full.as_ref()).await {
                    if let Err(err) =
                        errors::handle_guild("GuildDelete", &ctx, full.as_ref(), err).await
                    {
                        tracing::error!("Error in guild delete handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::GuildMemberAddition { new_member, .. } => {
                if let Err(err) = member::handle_addition(&ctx, &new_member).await {
                    if let Err(err) = errors::handle_member(&ctx, &new_member, err).await {
                        tracing::error!("Error in guild member addition handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
                if let Err(err) = member::handle_removal(&ctx, *guild_id, user.id).await {
                    tracing::error!("Error in guild member removal handler: {err:?}");
                }
            }
            serenity::FullEvent::VoiceStateUpdate { old, new, .. } => {
                if let Err(err) = voice_state::handle(&ctx, old.as_ref(), &new).await {
                    if let Err(err) =
                        errors::handle_unexpected_default(&ctx, "VoiceStateUpdate", err).await
                    {
                        tracing::error!("Error in voice state update handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::ChannelDelete { channel, .. } => {
                if let Err(err) = channel::handle_delete(&ctx, &channel).await {
                    tracing::error!("Error in channel delete handler: {err:?}");
                }
            }
            serenity::FullEvent::InteractionCreate { interaction, .. } => {
                if let Err(err) = other::interaction_create(&ctx, &interaction).await {
                    if let Err(err) =
                        errors::handle_unexpected_default(&ctx, "InteractionCreate", err).await
                    {
                        tracing::error!("Error in interaction create handler: {err:?}");
                    }
                }
            }
            serenity::FullEvent::Resume { .. } => {
                other::resume(&ctx);
            }

            _ => {}
        }
    }
}
