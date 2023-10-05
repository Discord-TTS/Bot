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

use std::collections::HashSet;
use std::num::NonZeroU16;
use std::sync::atomic::Ordering::SeqCst;

use poise::{serenity_prelude as serenity, futures_util::TryStreamExt};
use self::serenity::builder::*;

use crate::structs::{Context, CommandResult, PrefixContext, TTSModeChoice, Command};
use crate::opt_ext::OptionTryUnwrap;
use crate::funcs::dm_generic;

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn register(ctx: Context<'_>, #[flag] global: bool) -> CommandResult {
    poise::samples::register_application_commands(ctx, global).await.map_err(Into::into)
}

#[poise::command(prefix_command, hide_in_help, owners_only)]
pub async fn dm(ctx: PrefixContext<'_>, todm: serenity::User, #[rest] message: String) -> CommandResult {
    let attachment_url = ctx.msg.attachments.first().map(|a| a.url.clone());
    let (content, embed) = dm_generic(
        ctx.serenity_context(), &ctx.msg.author, todm.id, todm.tag(),
        attachment_url, message
    ).await?;

    ctx.msg.channel_id.send_message(&ctx.serenity_context(), CreateMessage::default()
        .content(content)
        .add_embed(CreateEmbed::from(embed))
    ).await?;

    Ok(())
}

#[poise::command(prefix_command, hide_in_help, owners_only)]
pub async fn close(ctx: Context<'_>) -> CommandResult {
    ctx.say(format!("Shutting down {} shards!", ctx.cache().shard_count())).await?;
    ctx.framework().shard_manager().shutdown_all().await;

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
        user.id, user.name, user.discriminator.map_or(0, NonZeroU16::get), user.id, guild.name
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

#[derive(poise::ChoiceParameter, PartialEq, Eq)]
pub enum PurgeGuildsMode {
    Run,
    Check,
    Abort,
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn purge_guilds(ctx: Context<'_>, mode: PurgeGuildsMode) -> CommandResult {
    #[derive(sqlx::FromRow)]
    struct HasGuildId {
        guild_id: i64
    }

    let data = ctx.data();
    if mode == PurgeGuildsMode::Abort {
        data.currently_purging.store(false, SeqCst);
        ctx.say("Done!").await?;
        return Ok(());
    }

    let cache = ctx.cache();
    let mut setup_guilds = HashSet::with_capacity(cache.guild_count());

    let mut stream = sqlx::query_as::<_, HasGuildId>("SELECT guild_id from guilds WHERE channel != 0").fetch(&data.pool);
    while let Some(item) = stream.try_next().await? {
        setup_guilds.insert(std::num::NonZeroU64::new(item.guild_id as u64).try_unwrap()?);
    }

    let to_leave: Vec<_> = cache.guilds().into_iter().filter(|g| !setup_guilds.contains(&g.0)).collect();
    let to_leave_count = to_leave.len();

    if mode == PurgeGuildsMode::Run {
        let msg = ctx.say(format!("Leaving {to_leave_count} guilds!")).await?;

        data.currently_purging.store(true, SeqCst);
        for guild in to_leave {
            guild.leave(ctx).await?;

            if !data.currently_purging.load(SeqCst) {
                msg.edit(ctx, poise::CreateReply::default().content("Aborted!")).await?;
                return Ok(());
            }
        }

        msg.edit(ctx, poise::CreateReply::default().content("Done! Left {to_leave_count} guilds!")).await?;
    } else {
        ctx.say(format!("Would purge {to_leave_count} guilds!")).await?;
    }

    Ok(())
}

#[poise::command(prefix_command, owners_only, hide_in_help)]
pub async fn refresh_ofs(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let http = &ctx.http();
    let cache = &ctx.cache();

    let support_guild_id = data.config.main_server;
    let support_guild_members = support_guild_id.members(http, None, None).await?;

    let all_guild_owners = cache.guilds().iter()
        .filter_map(|id| cache.guild(id).map(|g| g.owner_id))
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
        http.add_member_role(
            support_guild_id,
            *member,
            data.config.ofs_role,
            None
        ).await?;
    }

    let mut removed_role = 0;
    for member in should_not_be_ofs_members {
        removed_role += 1;
        http.remove_member_role(
            support_guild_id,
            *member,
            data.config.ofs_role,
            None
        ).await?;
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
    let author_id = ctx.author().id.into();

    let shard_id = ctx.serenity_context().shard_id;
    let user_row = data.userinfo_db.get(author_id).await?;
    let guild_row = data.guilds_db.get(guild_id_db).await?;
    let nick_row = data.nickname_db.get([guild_id_db, author_id]).await?;
    let guild_voice_row = data.guild_voice_db.get((guild_id_db, guild_row.voice_mode)).await?;
    let user_voice_row = data.user_voice_db.get((author_id, user_row.voice_mode.unwrap_or_default())).await?;

    let voice_client = data.songbird.get(guild_id);
    ctx.send(poise::CreateReply::default().embed(CreateEmbed::default()
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
    )).await?;
    Ok(())
}

/// Force leaves the voice channel in the current server to bypass buggy states
#[poise::command(prefix_command, guild_only, hide_in_help)]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    ctx.data().songbird.remove(guild_id).await.map_err(Into::into)
}

pub fn commands() -> [Command; 8] {
    [dm(), close(), debug(), register(), add_premium(), remove_cache(), refresh_ofs(), purge_guilds()]
}
