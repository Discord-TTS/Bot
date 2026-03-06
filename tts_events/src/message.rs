use self::serenity::{CreateMessage, GenericGuildChannelRef};
use poise::serenity_prelude as serenity;

use tts_core::{
    opt_ext::OptionTryUnwrap,
    structs::{Data, Result},
};

use tts::process_tts_msg;

mod tts;

pub async fn handle(ctx: &serenity::Context, new_message: &serenity::Message) -> Result<()> {
    tokio::try_join!(
        process_tts_msg(ctx, new_message),
        process_mention_msg(ctx, new_message)
    )?;

    Ok(())
}

async fn process_mention_msg(ctx: &serenity::Context, message: &serenity::Message) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    let Some(bot_mention_regex) = data.regex_cache.bot_mention.get() else {
        return Ok(());
    };

    if !bot_mention_regex.is_match(&message.content) {
        return Ok(());
    }

    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };

    let bot_user = ctx.cache.current_user().id;
    let bot_send_messages = {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            return Ok(());
        };

        let bot_member = guild.members.get(&bot_user).try_unwrap()?;
        match guild.channel(message.channel_id) {
            Some(GenericGuildChannelRef::Channel(ch)) => {
                guild.user_permissions_in(ch, bot_member).send_messages()
            }
            Some(GenericGuildChannelRef::Thread(th)) => {
                let parent_channel = guild.channels.get(&th.parent_id).try_unwrap()?;
                guild
                    .user_permissions_in(parent_channel, bot_member)
                    .send_messages_in_threads()
            }
            None => return Ok(()),
        }
    };

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let mut prefix = guild_row.prefix.as_str().replace(['`', '\\'], "");

    if bot_send_messages {
        prefix.insert_str(0, "Current prefix for this server is: ");
        message.channel_id.say(&ctx.http, prefix).await?;
    } else {
        let msg = {
            let guild = ctx.cache.guild(guild_id);
            let guild_name = match guild.as_ref() {
                Some(g) => &g.name,
                None => "Unknown Server",
            };

            format!(
                "My prefix for `{guild_name}` is {prefix} however I do not have permission to send messages so I cannot respond to your commands!"
            )
        };

        let msg = CreateMessage::default().content(msg);
        match message.author.id.dm(&ctx.http, msg).await {
            Err(serenity::Error::Http(error))
                if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {}
            Err(error) => return Err(anyhow::Error::from(error)),
            _ => {}
        }
    }

    Ok(())
}
