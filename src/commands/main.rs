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

use crate::structs::{Context, Error, PoiseContextAdditions, SerenityContextAdditions};
use crate::funcs::random_footer;

async fn channel_check(ctx: &Context<'_>) -> Result<bool, Error> {
    let guild_id = ctx.guild_id().unwrap();
    let channel_id: i64 = ctx.data().guilds_db.get(guild_id.into()).await?.get("channel");

    if channel_id == ctx.channel_id().0 as i64 {
        Ok(true)
    } else {
        ctx.send_error(
            "you ran this command in the wrong channel",
            Some(format!("do {}channel get the channel that has been setup", ctx.prefix()))
        ).await?;
        Ok(false)
    }
}


/// Joins the voice channel you're in!
#[poise::command(
    category="Main Commands",
    prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let guild = ctx.guild().ok_or(Error::GuildOnly)?;
    if !channel_check(&ctx).await? {
        return Ok(())
    }

    let author = ctx.author();

    let channel_id = match guild
        .voice_states
        .get(&author.id)
        .and_then(|vc| vc.channel_id)
    {
        Some(channel) => channel,
        None => {
            ctx.send_error(
                "you need to be in a voice channel to make me join your voice channel",
                Some(String::from("join a voice channel and try again")),
            ).await?;
            return Ok(())
        }
    };

    let ctx_discord = ctx.discord();
    let member = guild.member(ctx_discord, author.id).await?;
    let channel = channel_id.to_channel(ctx_discord).await?.guild().unwrap();

    let permissions = channel.permissions_for_user(ctx_discord, ctx_discord.cache.current_user_id())?;
    let required_permissions = serenity::Permissions::CONNECT | serenity::Permissions::SPEAK;

    let missing_permissions = required_permissions - permissions;
    if !missing_permissions.is_empty() {
        ctx.send_error(
            "I do not have permissions to TTS in your voice channel!",
            Some(format!(
                "please ask an administrator to give me: {}",
                missing_permissions.get_permission_names().join(", ")
            ))
        ).await?;
        return Ok(());
    }

    if let Some(bot_vc) = songbird::get(ctx_discord).await.unwrap().get(guild.id) {
        let bot_channel_id = bot_vc.lock().await.current_channel();
        if let Some(bot_channel_id) = bot_channel_id {
            if bot_channel_id.0 == channel_id.0 {
                ctx.say("I am already in your voice channel!").await?;
                return Ok(());
            };

            ctx.say(format!("I am already in <#{}>!", bot_channel_id))
                .await?;
            return Ok(());
        }
    };
    {
        let _typing = ctx.defer_or_broadcast().await?;
        ctx_discord.join_vc(&ctx.data().lavalink, &guild.id, &channel_id).await?;
    }

    ctx.send(|m| {
        m.embed(|e| {
            e.title("Joined your voice channel!");
            e.description("Just type normally and TTS Bot will say your messages!");
            e.thumbnail(ctx_discord.cache.current_user_field(|u| u.face()));
            e.author(|a| {
                a.name(member.nick.unwrap_or_else(|| author.name.clone()));
                a.icon_url(author.face())
            });
            e.footer(|f| f.text(random_footer(
                Some(ctx.prefix()),
                Some(&ctx.data().config.server_invite),
                Some(ctx_discord.cache.current_user_id().0)
            )))
        })
    })
    .await?;
    Ok(())
}

/// Leaves voice channel TTS Bot is in!
#[poise::command(
    category="Main Commands",
    prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let guild = ctx.guild().ok_or(Error::GuildOnly)?;
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
            ctx.say("Error: You need to be in the same voice channel as me to make me leave!")
                .await?;
            return Ok(());
        }

        manager.remove(guild.id).await?;
        ctx.data().lavalink.destroy(guild.id).await?;

        ctx.say("Left voice channel!").await?;
    } else {
        ctx.say("Error: How do I leave a voice channel if I am not in one?")
            .await?;
    }

    Ok(())
}

/// Clears the message queue!
#[poise::command(
    category="Main Commands",
    prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES | ADD_REACTIONS"
)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let guild = ctx.guild().ok_or(Error::GuildOnly)?;
    if !channel_check(&ctx).await? {
        return Ok(())
    }

    let lavalink = &ctx.data().lavalink;
    {
        let nodes = lavalink.nodes().await;
        let node = nodes.get_mut(&guild.id.into());
        if node.as_ref()
            .map(|node| node.queue.is_empty())
            .unwrap_or(true)
        {
            ctx.say("**Error:** Nothing in message queue to skip!").await?;
            return Ok(());
        } else {
            node.unwrap().queue.clear();
        }
    }

    lavalink.skip(guild.id).await;
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

    Ok(())
}
