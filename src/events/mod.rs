#![allow(clippy::module_name_repetitions)]

mod other;
mod guild;
mod ready;
mod message;
mod voice_state;

use other::*;
use guild::*;
use ready::*;
use message::*;
use voice_state::*;

use poise::serenity_prelude as serenity;
use serenity::FullEvent as Event;

use crate::structs::{Result, FrameworkContext};

pub async fn listen(framework_ctx: FrameworkContext<'_>, event: &Event) -> Result<()> {
    let data = framework_ctx.user_data;

    match event {
        Event::Message { ctx, new_message } => message(framework_ctx, ctx, new_message).await,
        Event::Ready { ctx, data_about_bot } => ready(framework_ctx, ctx, data_about_bot).await,

        Event::GuildCreate { ctx, guild, is_new } => guild_create(ctx, data, guild, *is_new).await,
        Event::GuildDelete { ctx, incomplete, full } => guild_delete(ctx, data, incomplete, full.as_ref()).await,
        Event::GuildMemberAddition { ctx, new_member } => guild_member_addition(ctx, data, new_member).await,

        Event::VoiceStateUpdate { ctx, old, new } => voice_state_update(ctx, data, old.as_ref(), new).await,

        Event::InteractionCreate { ctx, interaction } => interaction_create(framework_ctx, ctx, interaction).await,
        Event::Resume { .. } => {
            resume(data);
            Ok(())
        },

        _ => Ok(()),
    }
}
