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
use poise::{serenity_prelude as serenity, futures_util::TryStreamExt};

use crate::structs::{Context, CommandResult, Result, TTSModeChoice};

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

#[poise::command(prefix_command, owners_only, hide_in_help, subcommands("guild", "user", "guild_voice", "user_voice"))]
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
pub async fn guild_voice(ctx: Context<'_>, guild: i64, mode: TTSModeChoice) -> CommandResult {
    ctx.data().guild_voice_db.invalidate_cache(&(guild, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn user_voice(ctx: Context<'_>, user: i64, mode: TTSModeChoice) -> CommandResult {
    ctx.data().user_voice_db.invalidate_cache(&(user, mode.into()));
    ctx.say("Done!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn purge_guilds(ctx: Context<'_>, #[flag] run: bool) -> CommandResult {
    #[derive(sqlx::FromRow)]
    struct HasGuildId {
        guild_id: i64
    }

    let data = ctx.data();
    let ctx_discord = ctx.discord();
    let mut setup_guilds = std::collections::HashSet::new();

    let mut stream = sqlx::query_as::<_, HasGuildId>("SELECT * from guilds WHERE channel_id != 0").fetch(&data.inner.pool);
    while let Some(item) = stream.try_next().await? {
        setup_guilds.insert(item.guild_id as u64);
    }

    let to_leave: Vec<_> = ctx_discord.cache.guilds().into_iter().filter(|g| !setup_guilds.contains(&g.0)).collect();
    let to_leave_count = to_leave.len();

    if run {
        let msg = ctx.say(format!("Leaving {to_leave_count} guilds!")).await?;
        for guild in to_leave {
            guild.leave(ctx_discord).await?;
        }

        msg.edit(ctx, |b| b.content("Done! Left {to_leave_count} guilds!")).await.map(drop)
    } else {
        ctx.say(format!("Would purge {to_leave_count} guilds!")).await.map(drop)
    }.map_err(Into::into)
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn refresh_ofs(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let ctx_discord = ctx.discord();
    let http = &ctx_discord.http;
    let cache = &ctx_discord.cache;

    let support_guild_id = data.config.main_server;
    let support_guild_members = support_guild_id.members(http, None, None).await?;

    let all_guild_owners = cache.guilds().iter()
        .filter_map(|id| cache.guild_field(id, |g| g.owner_id))
        .collect::<Vec<_>>();

    let current_ofs_members = support_guild_members.iter()
        .filter(|m| m.roles.contains(&data.config.ofs_role))
        .map(|m| m.user.id)
        .collect::<Vec<_>>();

    let should_not_be_ofs_members = current_ofs_members.iter().filter(|ofs_member| !all_guild_owners.contains(ofs_member));
    let should_be_ofs_members = all_guild_owners.iter().filter(|owner|
        (!current_ofs_members.contains(owner)) &&
        support_guild_members.iter().any(|m| m.user.id == **owner)
    );

    let mut added_role = 0;
    for member in should_be_ofs_members {
        added_role += 1;
        http.add_member_role(support_guild_id.0, member.0, data.config.ofs_role.0, None).await?;
    }

    let mut removed_role = 0;
    for member in should_not_be_ofs_members {
        removed_role += 1;
        http.remove_member_role(support_guild_id.0, member.0, data.config.ofs_role.0, None).await?;
    }

    ctx.say(format!("Done! Removed {removed_role} members and added {added_role} members!")).await?;
    Ok(())
}


/// Debug commands for the bot
#[poise::command(prefix_command, slash_command, guild_only, subcommands("info", "leave"))]
pub async fn debug(ctx: Context<'_>) -> CommandResult {
    _info(ctx).await
}

/// Shows debug information including voice info and database info.
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn info(ctx: Context<'_>) -> CommandResult {
    _info(ctx).await
}

pub async fn _info(ctx: Context<'_>) -> CommandResult {
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

/// Force leaves the voice channel in the current server to bypass buggy states
#[poise::command(prefix_command, guild_only, hide_in_help)]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    songbird::get(ctx.discord()).await.unwrap().remove(guild_id).await.map_err(Into::into)
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
