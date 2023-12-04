#![allow(clippy::module_name_repetitions)]

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

use crate::structs::{FrameworkContext, Result};

pub async fn listen(
    ctx: &serenity::Context,
    event: &Event,
    framework_ctx: FrameworkContext<'_>,
) -> Result<()> {
    let data = framework_ctx.user_data;

    match event {
        Event::Message { new_message } => message(framework_ctx, ctx, new_message).await,
        Event::GuildCreate { guild, is_new } => guild_create(ctx, data, guild, *is_new).await,
        Event::Ready { data_about_bot } => ready(framework_ctx, ctx, data_about_bot).await,
        Event::GuildDelete { incomplete, full } => {
            guild_delete(ctx, data, incomplete, full.as_ref()).await
        }
        Event::GuildMemberAddition { new_member } => {
            guild_member_addition(ctx, data, new_member).await
        }
        Event::GuildMemberRemoval { guild_id, user, .. } => {
            guild_member_removal(ctx, data, *guild_id, user).await
        }
        Event::VoiceStateUpdate { old, new } => {
            voice_state_update(ctx, data, old.as_ref(), new).await
        }
        Event::ChannelDelete { channel, .. } => channel_delete(data, channel).await,
        Event::InteractionCreate { interaction } => {
            interaction_create(framework_ctx, ctx, interaction).await
        }
        Event::Resume { .. } => {
            resume(data);
            Ok(())
        }
        _ => Ok(()),
    }
}
