use poise::serenity_prelude as serenity;

use tts_core::{
    opt_ext::OptionTryUnwrap,
    structs::{Data, Result},
};

pub async fn handle(
    ctx: &serenity::Context,
    old: Option<&serenity::VoiceState>,
    new: &serenity::VoiceState,
) -> Result<()> {
    // User left vc
    let Some(old) = old else { return Ok(()) };

    let data = ctx.data_ref::<Data>();

    // Bot is in vc on server
    let guild_id = new.guild_id.try_unwrap()?;
    if data.songbird.get(guild_id).is_none() {
        return Ok(());
    }

    // Check if the bot is leaving
    let bot_id = ctx.cache.current_user().id;
    let leave_vc = match &new.member {
        // songbird does not clean up state on VC disconnections, so we have to do it here
        Some(member) if member.user.id == bot_id => true,
        Some(_) => check_is_lonely(ctx, bot_id, guild_id, old)?,
        None => false,
    };

    if leave_vc {
        data.last_to_xsaid_tracker.remove(&guild_id);
        data.songbird.remove(guild_id).await?;
    }

    Ok(())
}

/// If (on leave) the bot should also leave as it is alone
fn check_is_lonely(
    ctx: &serenity::Context,
    bot_id: serenity::UserId,
    guild_id: serenity::GuildId,
    old: &serenity::VoiceState,
) -> Result<bool> {
    let channel_id = old.channel_id.try_unwrap()?;
    let guild = ctx.cache.guild(guild_id).try_unwrap()?;
    let mut channel_members = guild.members.iter().filter(|m| {
        guild
            .voice_states
            .get(&m.user.id)
            .is_some_and(|v| v.channel_id == Some(channel_id))
    });

    // Bot is in the voice channel being left from
    if channel_members.clone().all(|m| m.user.id != bot_id) {
        return Ok(false);
    }

    // All the users in the vc are now bots
    if channel_members.any(|m| !m.user.bot()) {
        return Ok(false);
    }

    Ok(true)
}
