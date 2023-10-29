use std::sync::atomic::Ordering;

use tracing::info;

use serenity::builder::*;

use crate::{serenity, Data, Result};

pub async fn guild_create(
    ctx: &serenity::Context,
    data: &Data,
    guild: &serenity::Guild,
    is_new: Option<bool>,
) -> Result<()> {
    if !is_new.unwrap() {
        return Ok(());
    };

    let (owner_tag, owner_face) = {
        let owner = guild.owner_id.to_user(&ctx).await?;
        (owner.tag(), owner.face())
    };

    let dm_channel = guild.owner_id.create_dm_channel(&ctx).await?;
    match dm_channel.send_message(&ctx, serenity::CreateMessage::default().embed(CreateEmbed::default()
        .title(format!("Welcome to {}!", ctx.cache.current_user().name))
        .description(format!("
Hello! Someone invited me to your server `{}`!
TTS Bot is a text to speech bot, as in, it reads messages from a text channel and speaks it into a voice channel

**Most commands need to be done on your server, such as `/setup` and `/join`**

I need someone with the administrator permission to do `/setup #channel`
You can then do `/join` in that channel and I will join your voice channel!
Then, you can just type normal messages and I will say them, like magic!

You can view all the commands with `/help`
Ask questions by either responding here or asking on the support server!",
        guild.name))
        .footer(CreateEmbedFooter::new(format!("Support Server: {} | Bot Invite: https://bit.ly/TTSBotSlash", data.config.main_server_invite)))
        .author(CreateEmbedAuthor::new(owner_tag.clone()).icon_url(owner_face))
    )).await {
        Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {},
        Err(error) => return Err(anyhow::Error::from(error)),
        _ => {}
    }

    match ctx
        .http
        .add_member_role(
            data.config.main_server,
            guild.owner_id,
            data.config.ofs_role,
            None,
        )
        .await
    {
        Err(serenity::Error::Http(error))
            if error.status_code() == Some(serenity::StatusCode::NOT_FOUND) =>
        {
            return Ok(())
        }
        Err(err) => return Err(anyhow::Error::from(err)),
        Result::Ok(()) => (),
    }

    info!("Added OFS role to {}", owner_tag);

    Ok(())
}

pub async fn guild_delete(
    ctx: &serenity::Context,
    data: &Data,
    incomplete: &serenity::UnavailableGuild,
    full: Option<&serenity::Guild>,
) -> Result<()> {
    data.guilds_db.delete(incomplete.id.into()).await?;

    let Some(guild) = full else { return Ok(()) };
    if data.currently_purging.load(Ordering::SeqCst) {
        return Ok(());
    }

    if data
        .config
        .main_server
        .members(&ctx.http, None, None)
        .await?
        .into_iter()
        .filter(|m| m.roles.contains(&data.config.ofs_role))
        .any(|m| m.user.id == guild.owner_id)
    {
        ctx.http
            .remove_member_role(
                data.config.main_server,
                guild.owner_id,
                data.config.ofs_role,
                None,
            )
            .await?;
    }

    Ok(())
}
