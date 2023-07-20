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
use std::{borrow::Cow, num::NonZeroU16};

use songbird::error::JoinError;
use sqlx::Row;

use gnomeutils::{require, require_guild, PoiseContextExt as _};
use poise::{
    serenity_prelude::{self as serenity, builder::*},
    CreateReply,
};

use crate::funcs::random_footer;
use crate::structs::{Command, CommandResult, Context, JoinVCToken, Result, TTSMode};
use crate::traits::{PoiseContextExt, SongbirdManagerExt};

async fn channel_check(ctx: &Context<'_>, author_vc: Option<serenity::ChannelId>) -> Result<bool> {
    let guild_id = ctx.guild_id().unwrap();
    let setup_id = ctx.data().guilds_db.get(guild_id.into()).await?.channel;

    let channel_id = ctx.channel_id();
    if setup_id == channel_id.get() as i64 || author_vc == Some(channel_id) {
        Ok(true)
    } else {
        ctx.send_error(
            ctx.gettext("you ran this command in the wrong channel"),
            Some(ctx.gettext("do `/channel` get the channel that has been setup")),
        )
        .await?;
        Ok(false)
    }
}

/// Joins the voice channel you're in!
#[allow(clippy::too_many_lines)]
#[poise::command(
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn join(ctx: Context<'_>) -> CommandResult {
    let author_vc = require!(
        ctx.author_vc(),
        ctx.send_error(
            ctx.gettext("you need to be in a voice channel to make me join your voice channel"),
            Some(ctx.gettext("join a voice channel and try again")),
        )
        .await
        .map(drop)
    );

    if !channel_check(&ctx, Some(author_vc)).await? {
        return Ok(());
    }

    let ctx_discord = ctx.discord();
    let guild_id = ctx.guild_id().unwrap();
    let (bot_id, bot_face) = {
        let current_user = ctx_discord.cache.current_user();
        (current_user.id, current_user.face())
    };

    let bot_member = guild_id.member(ctx_discord, bot_id).await?;
    if let Some(communication_disabled_until) = bot_member.communication_disabled_until {
        if communication_disabled_until > serenity::Timestamp::now() {
            return ctx
                .send_error(
                    ctx.gettext("I am timed out"),
                    Some(ctx.gettext("ask a moderator to remove the timeout")),
                )
                .await
                .map(drop)
                .map_err(Into::into);
        }
    }

    let author = ctx.author();
    let member = guild_id.member(ctx_discord, author.id).await?;
    let channel = author_vc.to_channel(ctx_discord).await?.guild().unwrap();

    let missing_permissions = (serenity::Permissions::VIEW_CHANNEL
        | serenity::Permissions::CONNECT
        | serenity::Permissions::SPEAK)
        - channel.permissions_for_user(ctx_discord, bot_id)?;

    if !missing_permissions.is_empty() {
        return ctx
            .send_error(
                ctx.gettext("I do not have permissions to TTS in your voice channel"),
                Some(
                    &ctx.gettext("please ask an administrator to give me: {missing_permissions}")
                        .replace(
                            "{missing_permissions}",
                            &missing_permissions.get_permission_names().join(", "),
                        ),
                ),
            )
            .await
            .map(drop)
            .map_err(Into::into);
    }

    let data = ctx.data();
    if let Some(bot_vc) = data.songbird.get(guild_id) {
        let bot_channel_id = bot_vc.lock().await.current_channel();
        if let Some(bot_channel_id) = bot_channel_id {
            if bot_channel_id.0 == author_vc.0 {
                ctx.say(ctx.gettext("I am already in your voice channel!"))
                    .await?;
                return Ok(());
            };

            ctx.say(
                ctx.gettext("I am already in <#{channel_id}>!")
                    .replace("{channel_id}", &bot_channel_id.0.to_string()),
            )
            .await?;
            return Ok(());
        }
    };

    {
        let _typing = ctx.defer_or_broadcast().await?;

        let join_vc_lock = JoinVCToken::acquire(data, guild_id);
        let join_vc_result = data
            .songbird
            .join_vc(join_vc_lock.lock().await, author_vc)
            .await;

        if let Err(err) = join_vc_result {
            return if let JoinError::TimedOut = err {
                ctx.send_error(
                    ctx.gettext("a timeout occurred while joining your voice channel"),
                    Some(ctx.gettext("wait a few seconds and try again")),
                )
                .await
                .map(drop)
                .map_err(Into::into)
            } else {
                Err(err.into())
            };
        };
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title(ctx.gettext("Joined your voice channel!"))
                .description(ctx.gettext("Just type normally and TTS Bot will say your messages!"))
                .thumbnail(bot_face)
                .author(CreateEmbedAuthor::new(member.display_name()).icon_url(author.face()))
                .footer(CreateEmbedFooter::new(random_footer(
                    &data.config.main_server_invite,
                    bot_id,
                    ctx.current_catalog(),
                ))),
        ),
    )
    .await
    .map(drop)
    .map_err(Into::into)
}

/// Leaves voice channel TTS Bot is in!
#[poise::command(
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let (guild_id, author_vc) = {
        let guild = require_guild!(ctx);
        let channel_id = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|vs| vs.channel_id);

        (guild.id, channel_id)
    };

    let data = ctx.data();
    let bot_vc = {
        if let Some(handler) = data.songbird.get(guild_id) {
            handler.lock().await.current_channel()
        } else {
            None
        }
    };

    if let Some(bot_vc) = bot_vc {
        if !channel_check(&ctx, author_vc).await? {
        } else if author_vc.map_or(true, |author_vc| bot_vc.0 != author_vc.0) {
            ctx.say(ctx.gettext(
                "Error: You need to be in the same voice channel as me to make me leave!",
            ))
            .await?;
        } else {
            data.songbird.remove(guild_id).await?;
            data.last_to_xsaid_tracker.remove(&guild_id);

            ctx.say(ctx.gettext("Left voice channel!")).await?;
        }
    } else {
        ctx.say(ctx.gettext("Error: How do I leave a voice channel if I am not in one?"))
            .await?;
    }

    Ok(())
}

/// Clears the message queue!
#[poise::command(
    aliases("skip"),
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | ADD_REACTIONS"
)]
pub async fn clear(ctx: Context<'_>) -> CommandResult {
    if !channel_check(&ctx, ctx.author_vc()).await? {
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();
    if let Some(call_lock) = ctx.data().songbird.get(guild_id) {
        call_lock.lock().await.queue().stop();

        match ctx {
            poise::Context::Prefix(ctx) => {
                // Prefixed command, just add a thumbsup reaction
                ctx.msg.react(ctx.discord, '👍').await?;
            }
            poise::Context::Application(_) => {
                // Slash command, no message to react to, just say thumbsup
                ctx.say('👍').await?;
            }
        }
    } else {
        ctx.say(ctx.gettext("**Error**: I am not in a voice channel!"))
            .await?;
    };

    Ok(())
}

/// Activates a server for TTS Bot Premium!
#[poise::command(
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    aliases("activate"),
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn premium_activate(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let data = ctx.data();

    if data.premium_check(Some(guild_id)).await?.is_none() {
        return ctx
            .say(ctx.gettext("Hey, this server is already premium!"))
            .await
            .map(drop)
            .map_err(Into::into);
    }

    let author = ctx.author();
    let author_id = ctx.author().id.get() as i64;

    let linked_guilds: i64 = sqlx::query("SELECT count(*) FROM guilds WHERE premium_user = $1")
        .bind(author_id)
        .fetch_one(&data.inner.pool)
        .await?
        .get("count");

    let error_msg = match data.fetch_patreon_info(author.id).await? {
        Some(tier) => {
            if linked_guilds as u8 >= tier.entitled_servers {
                Some(Cow::Owned(ctx
                    .gettext("Hey, you already have {server_count} servers linked, you are only subscribed to the {entitled_servers} tier!")
                    .replace("{entitled_servers}", &tier.entitled_servers.to_string())
                    .replace("{server_count}", &linked_guilds.to_string())
                ))
            } else {
                None
            }
        }
        None => Some(Cow::Borrowed(
            ctx.gettext("Hey, I don't think you are subscribed on Patreon!"),
        )),
    };

    if let Some(error_msg) = error_msg {
        return ctx.send(CreateReply::default().embed(CreateEmbed::default()
            .title("TTS Bot Premium")
            .description(error_msg)
            .thumbnail(&data.premium_avatar_url)
            .colour(crate::constants::PREMIUM_NEUTRAL_COLOUR)
            .footer(CreateEmbedFooter::new({
                let line1 = ctx.gettext("If you have just subscribed, please wait for up to an hour for the member list to update!\n");
                let line2 = ctx.gettext("If this is incorrect, and you have waited an hour, please contact Gnome!#6669.");

                let mut concat = String::with_capacity(line1.len() + line2.len());
                concat.push_str(line1);
                concat.push_str(line2);
                concat
            }))
        )).await.map(drop).map_err(Into::into);
    }

    data.userinfo_db.create_row(author_id).await?;
    data.guilds_db
        .set_one(guild_id.into(), "premium_user", &author_id)
        .await?;
    data.guilds_db
        .set_one(guild_id.into(), "voice_mode", &TTSMode::gCloud)
        .await?;

    ctx.say(ctx.gettext("Done! This server is now premium!"))
        .await?;

    let guild = ctx.discord().cache.guild(guild_id);
    let guild_name = guild.as_ref().map_or("<Unknown>", |g| g.name.as_str());

    tracing::info!(
        "{}#{} | {} linked premium to {} | {}, they had {} linked servers",
        author.name,
        author.discriminator.map_or(0, NonZeroU16::get),
        author.id,
        guild_name,
        guild_id,
        linked_guilds
    );
    Ok(())
}

pub fn commands() -> [Command; 4] {
    [join(), leave(), clear(), premium_activate()]
}
