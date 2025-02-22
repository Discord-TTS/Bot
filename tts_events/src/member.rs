use poise::serenity_prelude as serenity;
use reqwest::StatusCode;

use tts_core::{
    common::{confirm_dialog_buttons, confirm_dialog_wait, remove_premium},
    constants::PREMIUM_NEUTRAL_COLOUR,
    structs::{Data, Result},
};

fn is_guild_owner(cache: &serenity::Cache, user_id: serenity::UserId) -> bool {
    cache
        .guilds()
        .into_iter()
        .find_map(|id| cache.guild(id).map(|g| g.owner_id == user_id))
        .unwrap_or(false)
}

async fn add_ofs_role(data: &Data, http: &serenity::Http, user_id: serenity::UserId) -> Result<()> {
    match http
        .add_member_role(data.config.main_server, user_id, data.config.ofs_role, None)
        .await
    {
        // Unknown member
        Err(serenity::Error::Http(serenity::HttpError::UnsuccessfulRequest(err)))
            if err.error.code == serenity::JsonErrorCode::UnknownMember =>
        {
            Ok(())
        }

        r => r.map_err(Into::into),
    }
}

pub async fn guild_member_addition(
    ctx: &serenity::Context,
    member: &serenity::Member,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    if member.guild_id == data.config.main_server && is_guild_owner(&ctx.cache, member.user.id) {
        add_ofs_role(data, &ctx.http, member.user.id).await?;
    }

    Ok(())
}

async fn dm_premium_notice(
    http: &serenity::Http,
    user_id: serenity::UserId,
) -> serenity::Result<serenity::Message> {
    let embed = [serenity::CreateEmbed::new()
        .colour(PREMIUM_NEUTRAL_COLOUR)
        .title("TTS Bot Premium - Important Message")
        .description(
            "You have just left a server that you have assigned as premium!
Do you want to remove that server from your assigned slots?",
        )];

    let buttons = confirm_dialog_buttons(
        "Keep premium subscription assigned",
        "Unassign premium subscription",
    );

    let components = [serenity::CreateActionRow::buttons(&buttons)];
    let notice = serenity::CreateMessage::new()
        .embeds(&embed)
        .components(&components);

    user_id.dm(http, notice).await
}

pub async fn guild_member_removal(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let Some(premium_user) = guild_row.premium_user else {
        return Ok(());
    };

    if premium_user != user_id {
        return Ok(());
    }

    if !data.is_premium_simple(&ctx.http, guild_id).await? {
        return Ok(());
    }

    let msg = match dm_premium_notice(&ctx.http, user_id).await {
        Ok(msg) => msg,
        Err(err) => {
            // We cannot DM this premium user, just remove premium by default.
            remove_premium(data, guild_id).await?;
            if let serenity::Error::Http(serenity::HttpError::UnsuccessfulRequest(err)) = &err
                && err.status_code == StatusCode::FORBIDDEN
            {
                return Ok(());
            }

            return Err(err.into());
        }
    };

    let guild_name = match ctx.cache.guild(guild_id) {
        Some(g) => &g.name.clone(),
        None => "<Unknown>",
    };

    let response = match confirm_dialog_wait(ctx, msg.id, premium_user).await? {
        Some(true) => format!("Okay, kept your premium assigned to {guild_name} ({guild_id})."),
        Some(false) => {
            remove_premium(data, guild_id).await?;
            format!("Okay, removed your premium assignment from {guild_name} ({guild_id}).")
        }
        None => {
            remove_premium(data, guild_id).await?;
            format!("You did not respond to whether or not to remove premium assignment from {guild_name} ({guild_id}), so it has been unassigned.")
        }
    };

    user_id
        .dm(&ctx.http, serenity::CreateMessage::new().content(response))
        .await?;

    Ok(())
}
