use poise::serenity_prelude as serenity;

use tts_core::structs::{Data, Result};

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
pub async fn handle_delete(
    ctx: &serenity::Context,
    channel: &serenity::GuildChannel,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    let guild_id = channel.base.guild_id;

    let call_channel_id = guild_call_channel_id(&data.songbird, guild_id).await;
    if call_channel_id == Some(channel.id) {
        // Ignore errors from leaving the channel, probably already left.
        let _ = data.leave_vc(guild_id).await;
    }

    Ok(())
}
