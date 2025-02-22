#![allow(clippy::module_name_repetitions)]
#![feature(let_chains)]

mod channel;
mod guild;
mod member;
mod message;
mod other;
mod ready;
mod voice_state;

use std::collections::VecDeque;

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
    async fn message(&self, ctx: serenity::Context, message: serenity::Message) {
        if let Err(err) = message::handle(&ctx, &message).await {
            if let Err(err) = errors::handle_message(&ctx, &message, err).await {
                tracing::error!("Error in message event handler: {err:?}");
            }
        }
    }

    async fn ready(&self, ctx: serenity::Context, ready: serenity::Ready) {
        if let Err(err) = ready::handle(&ctx, ready).await {
            if let Err(err) = errors::handle_unexpected_default(&ctx, "Ready", err).await {
                tracing::error!("Error in message event handler: {err:?}");
            }
        }
    }

    async fn guild_create(
        &self,
        ctx: serenity::Context,
        guild: serenity::Guild,
        is_new: Option<bool>,
    ) {
        if let Err(err) = guild::handle_create(&ctx, &guild, is_new).await {
            if let Err(err) = errors::handle_guild("GuildCreate", &ctx, Some(&guild), err).await {
                tracing::error!("Error in guild create handler: {err:?}");
            }
        }
    }

    async fn guild_delete(
        &self,
        ctx: serenity::Context,
        incomplete: serenity::UnavailableGuild,
        full: Option<serenity::Guild>,
    ) {
        if let Err(err) = guild::handle_delete(&ctx, incomplete, full.as_ref()).await {
            if let Err(err) = errors::handle_guild("GuildDelete", &ctx, full.as_ref(), err).await {
                tracing::error!("Error in guild delete handler: {err:?}");
            }
        }
    }

    async fn guild_member_addition(&self, ctx: serenity::Context, new_member: serenity::Member) {
        if let Err(err) = member::handle_addition(&ctx, &new_member).await {
            if let Err(err) = errors::handle_member(&ctx, &new_member, err).await {
                tracing::error!("Error in guild member addition handler: {err:?}");
            }
        }
    }

    async fn guild_member_removal(
        &self,
        ctx: serenity::Context,
        guild_id: serenity::GuildId,
        user: serenity::User,
        _: Option<serenity::Member>,
    ) {
        if let Err(err) = member::handle_removal(&ctx, guild_id, user.id).await {
            tracing::error!("Error in guild member removal handler: {err:?}");
        }
    }

    async fn voice_state_update(
        &self,
        ctx: serenity::Context,
        old: Option<serenity::VoiceState>,
        new: serenity::VoiceState,
    ) {
        if let Err(err) = voice_state::handle(&ctx, old.as_ref(), &new).await {
            if let Err(err) = errors::handle_unexpected_default(&ctx, "VoiceStateUpdate", err).await
            {
                tracing::error!("Error in voice state update handler: {err:?}");
            }
        }
    }

    async fn channel_delete(
        &self,
        ctx: serenity::Context,
        channel: serenity::GuildChannel,
        _: Option<VecDeque<serenity::Message>>,
    ) {
        if let Err(err) = channel::handle_delete(&ctx, &channel).await {
            tracing::error!("Error in channel delete handler: {err:?}");
        }
    }

    async fn interaction_create(&self, ctx: serenity::Context, interaction: serenity::Interaction) {
        if let Err(err) = other::interaction_create(&ctx, &interaction).await {
            if let Err(err) =
                errors::handle_unexpected_default(&ctx, "InteractionCreate", err).await
            {
                tracing::error!("Error in interaction create handler: {err:?}");
            }
        }
    }

    async fn resume(&self, ctx: serenity::Context, _: serenity::ResumedEvent) {
        other::resume(&ctx);
    }
}
