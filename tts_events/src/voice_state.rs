use poise::serenity_prelude as serenity;

use tts_core::{
    opt_ext::OptionTryUnwrap,
    structs::{FrameworkContext, Result},
};

/// If (on leave) the bot should also leave as it is alone
pub async fn voice_state_update(
    framework_ctx: FrameworkContext<'_>,
    old: Option<&serenity::VoiceState>,
    new: &serenity::VoiceState,
) -> Result<()> {
    // User left vc
    let Some(old) = old else { return Ok(()) };

    let data = framework_ctx.user_data();

    // Bot is in vc on server
    let guild_id = new.guild_id.try_unwrap()?;
    if data.songbird.get(guild_id).is_none() {
        return Ok(());
    }

    // Bot is not the one leaving
    let ctx = framework_ctx.serenity_context;
    let bot_id = ctx.cache.current_user().id;
    if new.member.as_ref().is_none_or(|m| m.user.id == bot_id) {
        return Ok(());
    }

    {
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
            return Ok(());
        }

        // All the users in the vc are now bots
        if channel_members.any(|m| !m.user.bot()) {
            return Ok(());
        };
    }

    data.songbird.remove(guild_id).await.map_err(Into::into)
}
