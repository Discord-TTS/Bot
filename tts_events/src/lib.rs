#![allow(clippy::module_name_repetitions)]
#![feature(let_chains)]

mod channel;
mod guild;
mod member;
mod message;
mod other;
mod ready;
mod voice_state;

use channel::*;
use guild::*;
use member::*;
use message::*;
use other::*;
use ready::*;
use voice_state::*;

use poise::serenity_prelude as serenity;
use serenity::FullEvent as Event;

use tts_core::structs::Result;

#[must_use]
pub fn get_intents() -> serenity::GatewayIntents {
    serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::GUILD_VOICE_STATES
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::MESSAGE_CONTENT
}

pub async fn listen(ctx: &serenity::Context, event: &Event) -> Result<()> {
    match event {
        Event::Message { new_message } => message(ctx, new_message).await,
        Event::GuildCreate { guild, is_new } => guild_create(ctx, guild, *is_new).await,
        Event::Ready { data_about_bot } => ready(ctx, data_about_bot).await,
        Event::GuildDelete { incomplete, full } => {
            guild_delete(ctx, incomplete, full.as_ref()).await
        }
        Event::GuildMemberAddition { new_member } => guild_member_addition(ctx, new_member).await,
        Event::GuildMemberRemoval { guild_id, user, .. } => {
            guild_member_removal(ctx, *guild_id, user.id).await
        }
        Event::VoiceStateUpdate { old, new } => voice_state_update(ctx, old.as_ref(), new).await,
        Event::ChannelDelete { channel, .. } => channel_delete(ctx, channel).await,
        Event::InteractionCreate { interaction } => interaction_create(ctx, interaction).await,
        Event::Resume { .. } => {
            resume(ctx);
            Ok(())
        }
        _ => Ok(()),
    }
}
