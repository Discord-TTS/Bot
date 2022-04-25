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
use std::borrow::Cow;

use poise::serenity_prelude as serenity;
use sqlx::Row;

use crate::structs::{Context, Result, CommandResult, PoiseContextExt, SerenityContextExt, TTSMode, JoinVCToken};
use crate::macros::{require_guild, require};
use crate::funcs::random_footer;

async fn channel_check(ctx: &Context<'_>) -> Result<bool> {
    let guild_id = ctx.guild_id().unwrap();
    let channel_id = ctx.data().guilds_db.get(guild_id.into()).await?.channel;

    if channel_id == ctx.channel_id().0 as i64 {
        Ok(true)
    } else {
        ctx.send_error(
            ctx.gettext("you ran this command in the wrong channel"),
            Some(ctx.gettext("do `/channel` get the channel that has been setup"))
        ).await?;
        Ok(false)
    }
}


/// Joins the voice channel you're in!
#[poise::command(
    category="Main Commands",
    guild_only, prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn join(ctx: Context<'_>) -> CommandResult {
    let guild = require_guild!(ctx);
    if !channel_check(&ctx).await? {
        return Ok(())
    }

    let author = ctx.author();

    let channel_id = require!(guild.voice_states.get(&author.id).and_then(|vc| vc.channel_id), {
        ctx.send_error(
            ctx.gettext("you need to be in a voice channel to make me join your voice channel"),
            Some(ctx.gettext("join a voice channel and try again")),
        ).await.map(|_| ())
    });

    let ctx_discord = ctx.discord();
    let member = guild.member(ctx_discord, author.id).await?;
    let channel = channel_id.to_channel(ctx_discord).await?.guild().unwrap();

    let permissions = channel.permissions_for_user(ctx_discord, ctx_discord.cache.current_user_id())?;
    let required_permissions = serenity::Permissions::CONNECT | serenity::Permissions::SPEAK;

    let missing_permissions = required_permissions - permissions;
    if !missing_permissions.is_empty() {
        ctx.send_error(
            ctx.gettext("I do not have permissions to TTS in your voice channel!"),
            Some(&ctx.gettext("please ask an administrator to give me: {missing_permissions}").replace(
                "{missing_permissions}",
                &missing_permissions.get_permission_names().join(", ")
            ))
        ).await?;
        return Ok(());
    }

    if let Some(bot_vc) = songbird::get(ctx_discord).await.unwrap().get(guild.id) {
        let bot_channel_id = bot_vc.lock().await.current_channel();
        if let Some(bot_channel_id) = bot_channel_id {
            if bot_channel_id.0 == channel_id.0 {
                ctx.say(ctx.gettext("I am already in your voice channel!")).await?;
                return Ok(());
            };

            ctx.say(&ctx.gettext("I am already in <#{channel_id}>!").replace("channel_id", &bot_channel_id.0.to_string())).await?;
            return Ok(());
        }
    };

    let data = ctx.data();

    {
        let _typing = ctx.defer_or_broadcast().await?;

        let join_vc_lock = JoinVCToken::acquire(data, guild.id);
        ctx_discord.join_vc(join_vc_lock.lock().await, channel_id).await?;
    }

    ctx.send(|m| {
        m.embed(|e| {e
            .title(ctx.gettext("Joined your voice channel!"))
            .description(ctx.gettext("Just type normally and TTS Bot will say your messages!"))
            .thumbnail(ctx_discord.cache.current_user_field(serenity::CurrentUser::face))
            .author(|a| {a
                .name(member.nick.unwrap_or_else(|| author.name.clone()))
                .icon_url(author.face())
            })
            .footer(|f| f.text(random_footer(
                &data.config.main_server_invite, ctx_discord.cache.current_user_id().0, ctx.current_catalog()
            )))
        })
    })
    .await?;
    Ok(())
}

/// Leaves voice channel TTS Bot is in!
#[poise::command(
    category="Main Commands",
    guild_only, prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let guild = require_guild!(ctx);
    if !channel_check(&ctx).await? {
        return Ok(())
    }

    let author_channel_id = guild
        .voice_states
        .get(&ctx.author().id)
        .and_then(|vs| vs.channel_id)
        .map(|vc| vc.0);

    let manager = songbird::get(ctx.discord()).await.unwrap();
    if let Some(handler) = manager.get(guild.id) {
        if handler.lock().await.current_channel().map(|c| c.0) != author_channel_id {
            ctx.say(ctx.gettext("Error: You need to be in the same voice channel as me to make me leave!")).await?;
            return Ok(());
        }

        let data = ctx.data();

        manager.remove(guild.id).await?;
        data.last_to_xsaid_tracker.remove(&guild.id);

        ctx.say(ctx.gettext("Left voice channel!")).await?;
    } else {
        ctx.say(ctx.gettext("Error: How do I leave a voice channel if I am not in one?")).await?;
    }

    Ok(())
}

/// Clears the message queue!
#[poise::command(
    aliases("skip"),
    category="Main Commands",
    guild_only, prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES | ADD_REACTIONS"
)]
pub async fn clear(ctx: Context<'_>) -> CommandResult {
    if !channel_check(&ctx).await? {
        return Ok(())
    }

    let guild_id = ctx.guild_id().unwrap();
    let manager = songbird::get(ctx.discord()).await.unwrap();
    if let Some(call_lock) = manager.get(guild_id) {
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
        ctx.say(ctx.gettext("**Error**: I am not in a voice channel!")).await?;
    };

    Ok(())
}

/// Activates a server for TTS Bot Premium!
#[poise::command(
    category="Main Commands",
    guild_only, prefix_command, slash_command,
    aliases("activate"),
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS | ADD_REACTIONS"
)]
pub async fn premium_activate(ctx: Context<'_>) -> CommandResult {
    let ctx_discord = ctx.discord();
    let guild = require_guild!(ctx);
    let data = ctx.data();

    if crate::premium_check(ctx_discord, data, Some(guild.id)).await?.is_none() {
        ctx.say(ctx.gettext("Hey, this server is already premium!")).await?;
        return Ok(())
    }

    let author = ctx.author();
    let author_id = author.id.into();

    let mut error_msg: Option<Cow<'_, str>> = match data.config.main_server.member(ctx_discord, author.id).await {
        Ok(m) if !m.roles.contains(&data.config.patreon_role) => Some(
            Cow::Borrowed(ctx.gettext(concat!(
                "Hey, you do not have the Patreon Role on the Support Server! Please link your ",
                "[patreon account to your discord account](https://support.patreon.com/hc/en-gb/articles/212052266) ",
                "or [purchase TTS Bot Premium via Patreon](https://patreon.com/Gnome_The_Bot_Maker)!"
            )))
        ),
        Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::NOT_FOUND) => Some(
            Cow::Owned(ctx
                .gettext("Hey, you are not in the [Support Server]({server_invite}) so I cannot validate your membership!")
                .replace("{server_invite}", &data.config.main_server_invite)
            )
        ),
        _ => None
    };

    let linked_guilds: i64 = {
        sqlx::query("SELECT count(*) FROM guilds WHERE premium_user = $1")
            .bind(&(author_id as i64))
            .fetch_one(&data.pool)
            .await?.get("count")
    };

    if error_msg.is_none() && linked_guilds >= 2 {
        error_msg = Some(Cow::Borrowed(ctx.gettext("Hey, you have too many servers linked! Please contact Gnome!#6669 if you have purchased the 5 Servers tier")));
    }

    if let Some(error_msg) = error_msg {
        ctx.send(|b| b.embed(|e| {e
            .title("TTS Bot Premium")
            .description(error_msg)
            .thumbnail(data.premium_avatar_url.clone())
            .colour(crate::constants::PREMIUM_NEUTRAL_COLOUR)
            .footer(|f| f.text("If this is an error, please contact Gnome!#6669."))
        })).await?;
        return Ok(())
    }

    data.userinfo_db.create_row(author_id).await?;
    data.guilds_db.set_one(guild.id.into(), "premium_user", &author_id).await?;
    data.guilds_db.set_one(guild.id.into(), "voice_mode", &TTSMode::Premium).await?;

    ctx.say(ctx.gettext("Done! This server is now premium!")).await?;

    tracing::info!(
        "{}#{} | {} linked premium to {} | {}, they had {} linked servers",
        author.name, author.discriminator, author.id, guild.name, guild.id, linked_guilds
    );
    Ok(())
}
