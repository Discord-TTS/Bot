// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
#![allow(clippy::unused_async)]

use poise::serenity_prelude as serenity;

use crate::structs::{Context, Error, TTSModeServerChoice};

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn register(ctx: Context<'_>, #[flag] global: bool) -> Result<(), Error> {
    poise::samples::register_application_commands(ctx, global).await?;

    Ok(())
}

#[poise::command(prefix_command, hide_in_help, owners_only)]
pub async fn dm(ctx: Context<'_>, todm: serenity::User, #[rest] message: String) -> Result<(), Error> {
    let ctx_discord = ctx.discord();
    let (content, embed) = dm_generic(ctx_discord, ctx.author(), &todm, &message).await?;
    
    let http = &ctx_discord.http;
    if let poise::Context::Prefix(ctx) = ctx {
        ctx.msg.channel_id.send_message(http, |b| {b
            .content(content)
            .set_embed(serenity::CreateEmbed::from(embed))
        }).await?;
    }

    Ok(())
}

#[poise::command(prefix_command, hide_in_help, owners_only)]
pub async fn close(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Shutting down {} shards!", ctx.discord().cache.shard_count())).await?;
    ctx.framework().shard_manager().lock().await.shutdown_all().await;

    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn add_premium(ctx: Context<'_>, guild: serenity::Guild, user: serenity::User) -> Result<(), Error> {
    let data = ctx.data();
    let user_id = user.id.into();

    data.userinfo_db.create_row(user_id).await?;
    data.guilds_db.set_one(guild.id.into(), "premium_user", &user_id).await?;

    ctx.say(format!(
        "Linked <@{}> ({}#{} | {}) to {}",
        user.id, user.name, user.discriminator, user.id, guild.name
    )).await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn remove_cache(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Please run a subcommand!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild(ctx: Context<'_>, guild: i64) -> Result<(), Error> {
    ctx.data().guilds_db.invalidate_cache(&guild);
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user(ctx: Context<'_>, user: i64) -> Result<(), Error> {
    ctx.data().userinfo_db.invalidate_cache(&user);
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild_voice(ctx: Context<'_>, guild: i64, mode: TTSModeServerChoice) -> Result<(), Error> {
    ctx.data().guild_voice_db.invalidate_cache(&(guild, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user_voice(ctx: Context<'_>, user: i64, mode: TTSModeServerChoice) -> Result<(), Error> {
    ctx.data().user_voice_db.invalidate_cache(&(user, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}


#[poise::command(prefix_command, hide_in_help)]
pub async fn debug(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?;

    let data = ctx.data();
    let ctx_discord = ctx.discord();

    let shard_id = ctx_discord.shard_id;
    let guild_data = data.guilds_db.get(guild_id.into()).await?;
    let user_data = data.userinfo_db.get(ctx.author().id.into()).await?;
    let voice_client = songbird::get(ctx_discord).await.unwrap().get(guild_id);

    ctx.send(|b| {b.embed(|e| {e
        .title("TTS Bot Debug Info")
        .description(format!("
Shard ID: `{shard_id}`
Voice Client: `{voice_client:?}`
Server Data: `{guild_data:?}`
User Data: `{user_data:?}`
        "))
    })}).await?;
    
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn cause_error(_: Context<'_>) -> Result<(), Error> {
    Err(anyhow::anyhow!("This is a test error!").into())
}

pub async fn dm_generic(
    ctx: &serenity::Context,
    author: &serenity::User,
    todm: &serenity::User,
    message: &str,
) -> Result<(String, serenity::Embed), Error> {
    let sent = todm.direct_message(ctx, |b| {b.embed(|e| {e
        .title("Message from the developers:")
        .description(message)
        .author(|a| {a
            .name(format!("{}#{}", author.name, author.discriminator))
            .icon_url(author.face())
        })
    })}).await?;

    Ok((format!("Sent message to {}#{}:", todm.name, todm.discriminator), sent.embeds.into_iter().next().unwrap()))
}
