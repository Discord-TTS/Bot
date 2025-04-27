use std::borrow::Cow;

use ::serenity::all::GenericGuildChannelRef;
use aformat::aformat;
use tracing::info;

use self::serenity::{CreateEmbed, CreateEmbedFooter, CreateMessage, ExecuteWebhook, Mentionable};
use poise::serenity_prelude as serenity;

use tts_core::{
    common::{dm_generic, random_footer},
    constants::DM_WELCOME_MESSAGE,
    opt_ext::OptionTryUnwrap,
    structs::{Data, Result},
};

use tts::process_tts_msg;

mod tts;

pub async fn handle(ctx: &serenity::Context, new_message: &serenity::Message) -> Result<()> {
    tokio::try_join!(
        process_tts_msg(ctx, new_message),
        process_support_dm(ctx, new_message),
        process_mention_msg(ctx, new_message),
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

            format!("My prefix for `{guild_name}` is {prefix} however I do not have permission to send messages so I cannot respond to your commands!")
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

async fn process_support_dm(ctx: &serenity::Context, message: &serenity::Message) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    let channel_id = message.channel_id;
    if message.guild_id.is_some() {
        return process_support_response(ctx, message, data, channel_id).await;
    }

    if message.author.bot() || message.content.starts_with('-') {
        return Ok(());
    }

    data.analytics.log(Cow::Borrowed("dm"), false);

    let userinfo = data.userinfo_db.get(message.author.id.into()).await?;
    if userinfo.dm_welcomed() {
        let content = message.content.to_lowercase();

        if content.contains("discord.gg") {
            let content = {
                let current_user = ctx.cache.current_user();
                format!(
                    "Join {} and look in {} to invite <@{}>!",
                    data.config.main_server_invite,
                    data.config.invite_channel.mention(),
                    current_user.id
                )
            };

            channel_id.say(&ctx.http, content).await?;
        } else if content.as_str() == "help" {
            channel_id.say(&ctx.http, "We cannot help you unless you ask a question, if you want the help command just do `-help`!").await?;
        } else if !userinfo.dm_blocked() {
            let webhook_username = {
                let mut tag = message.author.tag();
                tag.push_str(&aformat!(" ({})", message.author.id));
                tag
            };

            let mut attachments = Vec::new();
            for attachment in &message.attachments {
                let attachment_builder = serenity::CreateAttachment::url(
                    &ctx.http,
                    attachment.url.as_str(),
                    attachment.filename.to_string(),
                )
                .await?;
                attachments.push(attachment_builder);
            }

            let builder = ExecuteWebhook::default()
                .files(attachments)
                .content(message.content.as_str())
                .username(webhook_username)
                .avatar_url(message.author.face())
                .allowed_mentions(serenity::CreateAllowedMentions::new())
                .embeds(
                    message
                        .embeds
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect::<Vec<_>>(),
                );

            data.webhooks
                .dm_logs
                .execute(&ctx.http, false, builder)
                .await?;
        }
    } else {
        let (client_id, title) = {
            let current_user = ctx.cache.current_user();
            (
                current_user.id,
                aformat!("Welcome to {} Support DMs!", &current_user.name),
            )
        };

        let embeds = [CreateEmbed::default()
            .title(title.as_str())
            .description(DM_WELCOME_MESSAGE)
            .footer(CreateEmbedFooter::new(random_footer(
                &data.config.main_server_invite,
                client_id,
            )))];

        let welcome_msg = channel_id
            .send_message(&ctx.http, CreateMessage::default().embeds(&embeds))
            .await?;

        data.userinfo_db
            .set_one(message.author.id.into(), "dm_welcomed", &true)
            .await?;
        if channel_id.pins(&ctx.http).await?.len() < 50 {
            welcome_msg.pin(&ctx.http, None).await?;
        }

        info!(
            "{} just got the 'Welcome to support DMs' message",
            message.author.tag(),
        );
    }

    Ok(())
}

async fn process_support_response(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
    channel_id: serenity::GenericChannelId,
) -> Result<()> {
    if data.webhooks.dm_logs.channel_id.try_unwrap()? != channel_id.expect_channel() {
        return Ok(());
    }

    let Some(reference) = &message.message_reference else {
        return Ok(());
    };

    let Some(resolved_id) = reference.message_id else {
        return Ok(());
    };

    let (resolved_author_name, resolved_author_discrim) = {
        let message = channel_id.message(ctx, resolved_id).await?;
        (message.author.name, message.author.discriminator)
    };

    if resolved_author_discrim.is_some() {
        return Ok(());
    }

    let (target, target_tag) = {
        let Some(re_match) = data
            .regex_cache
            .id_in_brackets
            .captures(&resolved_author_name)
        else {
            return Ok(());
        };

        let Some(target_id_match) = re_match.get(1) else {
            return Ok(());
        };

        let target_id = target_id_match.as_str().parse::<serenity::UserId>()?;
        let target_tag = target_id.to_user(ctx).await?.tag();

        (target_id, target_tag)
    };

    let attachment_url = message.attachments.first().map(|a| a.url.as_str());

    let (content, embed) = dm_generic(
        ctx,
        &message.author,
        target,
        target_tag,
        attachment_url,
        &message.content,
    )
    .await?;

    let embeds = [CreateEmbed::from(embed)];
    channel_id
        .send_message(
            &ctx.http,
            CreateMessage::default().content(content).embeds(&embeds),
        )
        .await?;

    Ok(())
}
