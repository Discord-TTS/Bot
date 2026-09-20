use poise::serenity_prelude as serenity;
use tts_core::{structs::Data, structs::Result, voice};
pub async fn handle(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    channel_id: serenity::GenericChannelId,
    message_id: serenity::MessageId,
) -> Result<()> {
    let message_ids = vec![message_id].into_boxed_slice();
    handle_bulk(ctx, guild_id, channel_id, message_ids).await
}

pub async fn handle_bulk(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    channel_id: serenity::GenericChannelId,
    message_ids: Box<[serenity::MessageId]>,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();

    // Avoid sending deletion requests for non-setup channel or text-in-voice messages.
    //
    // Expecting the channel ID for this imperfect check is fine as this is just to prevent WS load.
    let channel_id = channel_id.expect_channel();
    let guild_row = data.guilds_db.get(guild_id.get() as _).await?;
    if guild_row.channel != Some(channel_id) {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            return Ok(());
        };

        let Some(bot_voice_state) = guild.voice_states.get(&ctx.cache.current_user().id) else {
            return Ok(());
        };

        if bot_voice_state.channel_id != Some(channel_id) {
            return Ok(());
        }
    }

    voice::delete_messages(data, guild_id, message_ids)?;
    Ok(())
}
