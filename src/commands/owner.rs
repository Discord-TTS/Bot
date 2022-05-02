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
use poise::serenity_prelude as serenity;

use crate::structs::{Context, CommandResult, Result, TTSModeServerChoice};

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn register(ctx: Context<'_>, #[flag] global: bool) -> CommandResult {
    poise::samples::register_application_commands(ctx, global).await?;

    Ok(())
}

#[poise::command(prefix_command, hide_in_help, owners_only)]
pub async fn dm(ctx: Context<'_>, todm: serenity::User, #[rest] message: String) -> CommandResult {
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
pub async fn close(ctx: Context<'_>) -> CommandResult {
    ctx.say(format!("Shutting down {} shards!", ctx.discord().cache.shard_count())).await?;
    ctx.framework().shard_manager().lock().await.shutdown_all().await;

    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn add_premium(ctx: Context<'_>, guild: serenity::Guild, user: serenity::User) -> CommandResult {
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
pub async fn remove_cache(ctx: Context<'_>) -> CommandResult {
    ctx.say("Please run a subcommand!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild(ctx: Context<'_>, guild: i64) -> CommandResult {
    ctx.data().guilds_db.invalidate_cache(&guild);
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user(ctx: Context<'_>, user: i64) -> CommandResult {
    ctx.data().userinfo_db.invalidate_cache(&user);
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn guild_voice(ctx: Context<'_>, guild: i64, mode: TTSModeServerChoice) -> CommandResult {
    ctx.data().guild_voice_db.invalidate_cache(&(guild, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user_voice(ctx: Context<'_>, user: i64, mode: TTSModeServerChoice) -> CommandResult {
    ctx.data().user_voice_db.invalidate_cache(&(user, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}


#[poise::command(prefix_command, guild_only, hide_in_help)]
pub async fn debug(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let guild_id_db: i64 = guild_id.into();

    let data = ctx.data();
    let ctx_discord = ctx.discord();
    let author_id = ctx.author().id.into();

    let shard_id = ctx_discord.shard_id;
    let user_row = data.userinfo_db.get(author_id).await?;
    let guild_row = data.guilds_db.get(guild_id_db).await?;
    let nick_row = data.nickname_db.get([guild_id_db, author_id]).await?;
    let guild_voice_row = data.guild_voice_db.get((guild_id_db, guild_row.voice_mode)).await?;
    let user_voice_row = match user_row.voice_mode {
        Some(mode) => Some(data.user_voice_db.get((author_id, mode)).await?),
        None => None
    };

    let voice_client = songbird::get(ctx_discord).await.unwrap().get(guild_id);
    ctx.send(|b| {b.embed(|e| {e
        .title("TTS Bot Debug Info")
        .description(format!("
Shard ID: `{shard_id}`
Voice Client: `{voice_client:?}`

Server Data: `{guild_row:?}`
User Data: `{user_row:?}`
Nickname Data: `{nick_row:?}`
User Voice Data: `{user_voice_row:?}`
Guild Voice Data: `{guild_voice_row:?}`
"))
    })}).await.map(drop).map_err(Into::into)
}


pub async fn dm_generic(
    ctx: &serenity::Context,
    author: &serenity::User,
    todm: &serenity::User,
    message: &str,
) -> Result<(String, serenity::Embed)> {
    let sent = todm.direct_message(ctx, |b| {b.embed(|e| {e
        .title("Message from the developers:")
        .description(message)
        .author(|a| {a
            .name(format!("{}#{:04}", author.name, author.discriminator))
            .icon_url(author.face())
        })
    })}).await?;

    Ok((format!("Sent message to {}#{:04}:", todm.name, todm.discriminator), sent.embeds.into_iter().next().unwrap()))
}
