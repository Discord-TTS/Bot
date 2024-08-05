use std::sync::atomic::Ordering;

use aformat::aformat;
use poise::serenity_prelude as serenity;

use tts_core::structs::{CommandResult, Context};

/// Owner only: used to block a user from dms
#[poise::command(
    prefix_command,
    category = "Settings",
    owners_only,
    hide_in_help,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn block(ctx: Context<'_>, user: serenity::UserId, value: bool) -> CommandResult {
    ctx.data()
        .userinfo_db
        .set_one(user.into(), "dm_blocked", &value)
        .await?;

    ctx.say("Done!").await?;
    Ok(())
}

/// Owner only: used to block a user from the bot
#[poise::command(
    prefix_command,
    category = "Settings",
    owners_only,
    hide_in_help,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn bot_ban(ctx: Context<'_>, user: serenity::UserId, value: bool) -> CommandResult {
    let user_id = user.into();
    let userinfo_db = &ctx.data().userinfo_db;

    userinfo_db.set_one(user_id, "bot_banned", &value).await?;
    if value {
        userinfo_db.set_one(user_id, "dm_blocked", &true).await?;
    }

    let msg = aformat!("Set bot ban status for user {user} to `{value}`.");
    ctx.say(msg.as_str()).await?;

    Ok(())
}

/// Owner only: Enables or disables the gTTS voice mode
#[poise::command(
    prefix_command,
    category = "Settings",
    owners_only,
    hide_in_help,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn gtts_disabled(ctx: Context<'_>, value: bool) -> CommandResult {
    let data = ctx.data();
    if data.config.gtts_disabled.swap(value, Ordering::Relaxed) == value {
        ctx.say("It's already set that way, silly.").await?;
        return Ok(());
    }

    let msg = if value {
        "Disabled gTTS globally, womp womp"
    } else {
        "Re-enabled gTTS globally, yippee!\nMake sure to check the config file though."
    };

    ctx.say(msg).await?;
    Ok(())
}
