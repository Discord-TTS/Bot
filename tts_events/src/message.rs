use std::{borrow::Cow, num::NonZeroU16};

use to_arraystring::ToArrayString;
use tracing::info;

use self::serenity::{CreateEmbed, CreateEmbedFooter, CreateMessage, ExecuteWebhook, Mentionable};
use poise::serenity_prelude as serenity;

use tts_core::{
    common::{clean_msg, dm_generic, fetch_audio, prepare_url, random_footer, run_checks},
    constants::DM_WELCOME_MESSAGE,
    errors,
    opt_ext::OptionTryUnwrap,
    require,
    structs::{Data, FrameworkContext, IsPremium, JoinVCToken, Result, TTSMode},
    traits::SongbirdManagerExt,
};

pub async fn message(
    framework_ctx: FrameworkContext<'_>,
    new_message: &serenity::Message,
) -> Result<()> {
    tokio::try_join!(
        process_tts_msg(framework_ctx, new_message),
        process_support_dm(framework_ctx, new_message),
        process_mention_msg(framework_ctx, new_message),
    )?;

    Ok(())
}

async fn process_tts_msg(
    framework_ctx: FrameworkContext<'_>,
    message: &serenity::Message,
) -> Result<()> {
    let data = framework_ctx.user_data();
    let ctx = framework_ctx.serenity_context;

    let guild_id = require!(message.guild_id, Ok(()));
    let (guild_row, user_row) = tokio::try_join!(
        data.guilds_db.get(guild_id.into()),
        data.userinfo_db.get(message.author.id.into()),
    )?;

    let (mut content, to_autojoin) = require!(
        run_checks(ctx, message, &guild_row, &user_row).await?,
        Ok(())
    );

    let is_premium = data.is_premium_simple(guild_id).await?;
    let (voice, mode) = {
        if let Some(channel_id) = to_autojoin {
            let join_vc_lock = JoinVCToken::acquire(&data, guild_id);
            match data.songbird.join_vc(join_vc_lock, channel_id).await {
                Ok(call) => call,
                Err(songbird::error::JoinError::TimedOut) => return Ok(()),
                Err(err) => return Err(err.into()),
            };
        }

        let is_ephemeral = message
            .flags
            .is_some_and(|f| f.contains(serenity::model::channel::MessageFlags::EPHEMERAL));

        let m;
        let member_nick = match &message.member {
            Some(member) => member.nick.as_deref(),
            None if message.webhook_id.is_none() && !is_ephemeral => {
                m = guild_id.member(ctx, message.author.id).await?;
                m.nick.as_deref()
            }
            None => None,
        };

        let (voice, mode) = data
            .parse_user_or_guild_with_premium(message.author.id, Some((guild_id, is_premium)))
            .await?;

        let nickname_row = data
            .nickname_db
            .get([guild_id.into(), message.author.id.into()])
            .await?;

        content = clean_msg(
            &content,
            &message.author,
            &ctx.cache,
            guild_id,
            member_nick,
            &message.attachments,
            &voice,
            guild_row.xsaid(),
            guild_row.repeated_chars,
            nickname_row.name.as_deref(),
            user_row.use_new_formatting(),
            &data.regex_cache,
            &data.last_to_xsaid_tracker,
        );

        (voice, mode)
    };

    // Final check, make sure we aren't sending an empty message or just symbols.
    let mut removed_chars_content = content.clone();
    removed_chars_content.retain(|c| !" ?.)'!\":".contains(c));
    if removed_chars_content.is_empty() {
        return Ok(());
    }

    let speaking_rate = data.speaking_rate(message.author.id, mode).await?;
    let url = prepare_url(
        data.config.tts_service.clone(),
        &content,
        &voice,
        mode,
        &speaking_rate,
        &guild_row.msg_length.to_arraystring(),
        guild_row.target_lang(IsPremium::from(is_premium)),
    );

    let call_lock = if let Some(call) = data.songbird.get(guild_id) {
        call
    } else {
        // At this point, the bot is "in" the voice channel, but without a voice client,
        // this is usually if the bot restarted but the bot is still in the vc from the last boot.
        let voice_channel_id = {
            let guild = ctx.cache.guild(guild_id).try_unwrap()?;
            guild
                .voice_states
                .get(&message.author.id)
                .and_then(|vs| vs.channel_id)
                .try_unwrap()?
        };

        let join_vc_token = JoinVCToken::acquire(&data, guild_id);
        match data.songbird.join_vc(join_vc_token, voice_channel_id).await {
            Ok(call) => call,
            Err(songbird::error::JoinError::TimedOut) => return Ok(()),
            Err(err) => return Err(err.into()),
        }
    };

    // Pre-fetch the audio to handle max_length errors
    let audio = require!(
        fetch_audio(
            &data.reqwest,
            url.clone(),
            data.config.tts_service_auth_key.as_deref()
        )
        .await?,
        Ok(())
    );

    let hint = audio
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .map(|ct| {
            let mut hint = songbird::input::core::probe::Hint::new();
            hint.mime_type(ct.to_str()?);
            Ok::<_, anyhow::Error>(hint)
        })
        .transpose()?;

    let input = Box::new(std::io::Cursor::new(audio.bytes().await?));
    let wrapped_audio =
        songbird::input::LiveInput::Raw(songbird::input::AudioStream { input, hint });

    let track_handle = {
        let mut call = call_lock.lock().await;
        call.enqueue_input(songbird::input::Input::Live(wrapped_audio, None))
            .await
    };

    data.analytics.log(
        Cow::Borrowed(match mode {
            TTSMode::gTTS => "gTTS_tts",
            TTSMode::eSpeak => "eSpeak_tts",
            TTSMode::gCloud => "gCloud_tts",
            TTSMode::Polly => "Polly_tts",
        }),
        false,
    );

    let guild = ctx.cache.guild(guild_id).try_unwrap()?;
    let (blank_name, blank_value, blank_inline) = errors::blank_field();

    let extra_fields = [
        ("Guild Name", Cow::Owned(guild.name.to_string()), true),
        ("Guild ID", Cow::Owned(guild.id.to_string()), true),
        (blank_name, blank_value, blank_inline),
        (
            "Message length",
            Cow::Owned(content.len().to_string()),
            true,
        ),
        ("Voice", voice, true),
        ("Mode", Cow::Owned(mode.to_string()), true),
    ];

    let shard_manager = framework_ctx.shard_manager.clone();
    let author_name = message.author.name.clone();
    let icon_url = message.author.face();

    errors::handle_track(
        ctx.clone(),
        shard_manager,
        extra_fields,
        author_name,
        icon_url,
        &track_handle,
    )
    .map_err(Into::into)
}

async fn process_mention_msg(
    framework_ctx: FrameworkContext<'_>,
    message: &serenity::Message,
) -> Result<()> {
    let data = framework_ctx.user_data();
    let Some(bot_mention_regex) = data.regex_cache.bot_mention.get() else {
        return Ok(());
    };

    if !bot_mention_regex.is_match(&message.content) {
        return Ok(());
    };

    let ctx = framework_ctx.serenity_context;
    let bot_user = ctx.cache.current_user().id;
    let guild_id = require!(message.guild_id, Ok(()));
    let channel = message.channel(ctx).await?.guild().unwrap();
    let permissions = channel.permissions_for_user(&ctx.cache, bot_user)?;

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let prefix = guild_row.prefix.as_str().replace(['`', '\\'], "");

    if permissions.send_messages() {
        channel
            .say(
                &ctx.http,
                format!("Current prefix for this server is: {prefix}"),
            )
            .await?;
    } else {
        let msg = {
            let guild = ctx.cache.guild(guild_id);
            let guild_name = guild.as_ref().map_or("Unknown Server", |g| g.name.as_str());

            format!("My prefix for `{guild_name}` is {prefix} however I do not have permission to send messages so I cannot respond to your commands!")
        };

        match message
            .author
            .dm(&ctx.http, CreateMessage::default().content(msg))
            .await
        {
            Err(serenity::Error::Http(error))
                if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {}
            Err(error) => return Err(anyhow::Error::from(error)),
            _ => {}
        }
    }

    Ok(())
}

async fn process_support_dm(
    framework_ctx: FrameworkContext<'_>,
    message: &serenity::Message,
) -> Result<()> {
    let data = framework_ctx.user_data();
    let ctx = framework_ctx.serenity_context;

    let channel = match message.channel(ctx).await? {
        serenity::Channel::Guild(channel) => {
            return process_support_response(ctx, message, &data, channel).await
        }
        serenity::Channel::Private(channel) => channel,
        _ => return Ok(()),
    };

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

            channel.say(&ctx.http, content).await?;
        } else if content.as_str() == "help" {
            channel.say(&ctx.http, "We cannot help you unless you ask a question, if you want the help command just do `-help`!").await?;
        } else if !userinfo.dm_blocked() {
            let webhook_username = format!("{} ({})", message.author.tag(), message.author.id);

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
                format!("Welcome to {} Support DMs!", current_user.name),
            )
        };

        let welcome_msg = channel
            .send_message(
                &ctx.http,
                CreateMessage::default().embed(
                    CreateEmbed::default()
                        .title(title)
                        .description(DM_WELCOME_MESSAGE)
                        .footer(CreateEmbedFooter::new(random_footer(
                            &data.config.main_server_invite,
                            client_id,
                            data.default_catalog(),
                        ))),
                ),
            )
            .await?;

        data.userinfo_db
            .set_one(message.author.id.into(), "dm_welcomed", &true)
            .await?;
        if channel.pins(&ctx.http).await?.len() < 50 {
            welcome_msg.pin(&ctx.http, None).await?;
        }

        info!(
            "{}#{} just got the 'Welcome to support DMs' message",
            message.author.name,
            message.author.discriminator.map_or(0, NonZeroU16::get)
        );
    };

    Ok(())
}

async fn process_support_response(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
    channel: serenity::GuildChannel,
) -> Result<()> {
    let reference = require!(&message.message_reference, Ok(()));
    if data.webhooks.dm_logs.channel_id.try_unwrap()? != channel.id {
        return Ok(());
    };

    let resolved_id = require!(reference.message_id, Ok(()));
    let (resolved_author_name, resolved_author_discrim) = {
        let message = ctx.http.get_message(channel.id, resolved_id).await?;
        (message.author.name, message.author.discriminator)
    };

    if resolved_author_discrim.is_some() {
        return Ok(());
    }

    let (target, target_tag) = {
        let re_match = require!(
            data.regex_cache
                .id_in_brackets
                .captures(&resolved_author_name),
            Ok(())
        );

        let target: serenity::UserId = require!(re_match.get(1), Ok(())).as_str().parse()?;
        let target_tag = target.to_user(ctx).await?.tag();

        (target, target_tag)
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

    channel
        .send_message(
            &ctx.http,
            CreateMessage::default()
                .content(content)
                .embed(CreateEmbed::from(embed)),
        )
        .await?;

    Ok(())
}
