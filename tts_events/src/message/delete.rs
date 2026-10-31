use poise::serenity_prelude as serenity;
use tts_core::{
    structs::{Data, Result},
    voice,
};

mod pluralkit;
pub(crate) use pluralkit::PK_USER_ID;

pub async fn handle(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    channel_id: serenity::GenericChannelId,
    message_id: serenity::MessageId,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    if is_correct_channel(data, &ctx.cache, guild_id, channel_id).await? {
        if pluralkit::is_proxy_delete(ctx, data, guild_id, message_id).await? {
            return Ok(());
        }

        voice::delete_messages(data, guild_id, vec![message_id].into_boxed_slice())?;
    }

    Ok(())
}

pub async fn handle_bulk(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    channel_id: serenity::GenericChannelId,
    message_ids: Box<[serenity::MessageId]>,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    if is_correct_channel(data, &ctx.cache, guild_id, channel_id).await? {
        voice::delete_messages(data, guild_id, message_ids)?;
    }

    Ok(())
}

async fn is_correct_channel(
    data: &Data,
    cache: &serenity::Cache,
    guild_id: serenity::GuildId,
    channel_id: serenity::GenericChannelId,
) -> Result<bool> {
    // Avoid sending deletion requests for non-setup channel or text-in-voice messages.
    //
    // Expecting the channel ID for this imperfect check is fine as this is just to prevent PK/WS load.
    let channel_id = channel_id.expect_channel();
    let guild_row = data.guilds_db.get(guild_id.get() as _).await?;
    if guild_row.channel != Some(channel_id) {
        let Some(guild) = cache.guild(guild_id) else {
            return Ok(false);
        };

        let Some(bot_voice_state) = guild.voice_states.get(&cache.current_user().id) else {
            return Ok(false);
        };

        if bot_voice_state.channel_id != Some(channel_id) {
            return Ok(false);
        }
    }

    Ok(true)
}
