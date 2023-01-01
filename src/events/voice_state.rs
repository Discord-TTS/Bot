use gnomeutils::OptionTryUnwrap;
use poise::serenity_prelude as serenity;

use crate::structs::{Data, Result};


pub async fn voice_state_update(ctx: &serenity::Context, data: &Data, old: Option<&serenity::VoiceState>, new: &serenity::VoiceState) -> Result<()> {
    // If (on leave) the bot should also leave as it is alone
    let bot_id = ctx.cache.current_user().id;
    let guild_id = new.guild_id.try_unwrap()?;

    if data.songbird.get(guild_id).is_some()
        && let Some(old) = old && new.channel_id.is_none() // user left vc
        && !new.member.as_ref().map_or(false, |m| m.user.id == bot_id) // user other than bot leaving
        && !ctx.cache // filter out bots from members
            .guild_channel(old.channel_id.try_unwrap()?)
            .try_unwrap()?
            .members(&ctx.cache)?
            .into_iter().any(|m| !m.user.bot)
    {
        data.songbird.remove(guild_id).await?;
    };

    Ok(())
}
