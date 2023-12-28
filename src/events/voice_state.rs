use poise::serenity_prelude as serenity;

use crate::{
    opt_ext::OptionTryUnwrap,
    structs::{Data, Result},
};

/// If (on leave) the bot should also leave as it is alone
pub async fn voice_state_update(
    ctx: &serenity::Context,
    data: &Data,
    old: Option<&serenity::VoiceState>,
    new: &serenity::VoiceState,
) -> Result<()> {
    // User left vc
    let Some(old) = old else { return Ok(()) };

    // Bot is in vc on server
    let guild_id = new.guild_id.try_unwrap()?;
    if data.songbird.get(guild_id).is_none() {
        return Ok(());
    }

    // Bot is not the one leaving
    let bot_id = ctx.cache.current_user().id;
    if new.member.as_ref().map_or(true, |m| m.user.id == bot_id) {
        return Ok(());
    }

    let channel_members = ctx
        .cache
        .channel(old.channel_id.try_unwrap()?)
        .try_unwrap()?
        .members(&ctx.cache)?;

    // Bot is in the voice channel being left from
    if channel_members.iter().all(|m| m.user.id != bot_id) {
        return Ok(());
    }

    // All the users in the vc are now bots
    if channel_members.into_iter().any(|m| !m.user.bot()) {
        return Ok(());
    };

    data.songbird.remove(guild_id).await.map_err(Into::into)
}
