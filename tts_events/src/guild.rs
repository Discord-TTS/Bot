use aformat::aformat;
use reqwest::StatusCode;
use tracing::info;

use serenity::{all as serenity, builder::*};

use tts_core::structs::{Data, Result};

pub async fn handle_create(
    ctx: &serenity::Context,
    guild: &serenity::Guild,
    is_new: Option<bool>,
) -> Result<()> {
    if !is_new.unwrap() {
        return Ok(());
    }

    let (owner_tag, owner_face) = {
        let owner = guild.owner_id.to_user(&ctx).await?;
        (owner.tag(), owner.face())
    };

    let data = ctx.data_ref::<Data>();
    let title = aformat!("Welcome to {}!", &ctx.cache.current_user().name);
    let embeds = [CreateEmbed::default()
        .title(title.as_str())
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
        .author(CreateEmbedAuthor::new(&owner_tag).icon_url(owner_face))];

    match guild
        .owner_id
        .dm(
            &ctx.http,
            serenity::CreateMessage::default().embeds(&embeds),
        )
        .await
    {
        Err(serenity::Error::Http(error))
            if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {}
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
            return Ok(());
        }
        Err(err) => return Err(anyhow::Error::from(err)),
        Result::Ok(()) => (),
    }

    info!("Added OFS role to {}", owner_tag);

    Ok(())
}

pub async fn handle_delete(
    ctx: &serenity::Context,
    incomplete: serenity::UnavailableGuild,
    full: Option<&serenity::Guild>,
) -> Result<()> {
    if incomplete.unavailable {
        return Ok(());
    }

    let data = ctx.data_ref::<Data>();
    data.guilds_db.delete(incomplete.id.into()).await?;

    let Some(guild) = full else { return Ok(()) };

    let owner_of_other_server = ctx
        .cache
        .guilds()
        .into_iter()
        .filter_map(|g| ctx.cache.guild(g))
        .any(|g| g.owner_id == guild.owner_id);

    if owner_of_other_server {
        return Ok(());
    }

    match ctx
        .http
        .remove_member_role(
            data.config.main_server,
            guild.owner_id,
            data.config.ofs_role,
            None,
        )
        .await
    {
        Ok(()) => Ok(()),
        Err(serenity::Error::Http(serenity::HttpError::UnsuccessfulRequest(err)))
            if err.status_code == StatusCode::NOT_FOUND =>
        {
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}
