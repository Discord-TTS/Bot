use poise::serenity_prelude as serenity;
use reqwest::StatusCode;

use crate::{
    constants::PREMIUM_NEUTRAL_COLOUR,
    funcs::{confirm_dialog_components, confirm_dialog_wait},
    Data, Result,
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
            if err.error.code == 10007 =>
        {
            Ok(())
        }

        r => r.map_err(Into::into),
    }
}

pub async fn guild_member_addition(
    ctx: &serenity::Context,
    data: &Data,
    member: &serenity::Member,
) -> Result<()> {
    if member.guild_id == data.config.main_server && is_guild_owner(&ctx.cache, member.user.id) {
        add_ofs_role(data, &ctx.http, member.user.id).await?;
    }

    Ok(())
}

async fn remove_premium(data: &Data, guild_id: serenity::GuildId) -> Result<()> {
    data.guilds_db
        .set_one(guild_id.into(), "premium_user", None::<i64>)
        .await
}

fn create_premium_notice() -> serenity::CreateMessage {
    let embed = serenity::CreateEmbed::new()
        .colour(PREMIUM_NEUTRAL_COLOUR)
        .title("TTS Bot Premium - Important Message")
        .description(
            "You have just left a server that you have assigned as premium!
Do you want to remove that server from your assigned slots?",
        );

    let components = confirm_dialog_components(
        String::from("Keep premium subscription assigned"),
        String::from("Unassign premium subscription"),
    );

    serenity::CreateMessage::new()
        .embed(embed)
        .components(components)
}

pub async fn guild_member_removal(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    user: &serenity::User,
) -> Result<()> {
    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let Some(premium_user) = guild_row.premium_user else {
        return Ok(());
    };

    if premium_user != user.id {
        return Ok(());
    }

    if data.premium_check(Some(guild_id)).await?.is_some() {
        return Ok(());
    }

    let msg = match user.dm(ctx, create_premium_notice()).await {
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

    let guild_name = ctx
        .cache
        .guild(guild_id)
        .map_or_else(|| String::from("<Unknown>"), |g| g.name.clone());

    let response = match confirm_dialog_wait(ctx, &msg, premium_user).await? {
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

    user.dm(ctx, serenity::CreateMessage::new().content(response))
        .await?;

    Ok(())
}
