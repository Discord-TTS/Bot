use poise::serenity_prelude as serenity;

use crate::{structs::FrameworkContext, Result};

async fn guild_call_channel_id(
    songbird: &songbird::Songbird,
    guild_id: serenity::GuildId,
) -> Option<serenity::ChannelId> {
    songbird
        .get(guild_id)?
        .lock()
        .await
        .current_channel()
        .map(|c| serenity::ChannelId::new(c.get()))
}

// Check if the channel the bot was in was deleted.
pub async fn channel_delete(
    framework_ctx: FrameworkContext<'_>,
    channel: &serenity::GuildChannel,
) -> Result<()> {
    let data = framework_ctx.user_data();

    let call_channel_id = guild_call_channel_id(&data.songbird, channel.guild_id).await;
    if call_channel_id == Some(channel.id) {
        // Ignore errors from leaving the channel, probably already left.
        let _ = data.songbird.remove(channel.guild_id).await;
        data.last_to_xsaid_tracker.remove(&channel.guild_id);
    }

    Ok(())
}
