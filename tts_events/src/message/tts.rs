use std::borrow::Cow;

use aformat::ToArrayString as _;
use poise::serenity_prelude as serenity;

use tts_core::{
    common::{clean_msg, fetch_audio, prepare_url},
    database::{GuildRow, UserRow},
    errors,
    opt_ext::OptionTryUnwrap as _,
    structs::{FrameworkContext, IsPremium, JoinVCToken, Result, TTSMode},
    traits::SongbirdManagerExt as _,
};

pub(crate) async fn process_tts_msg(
    framework_ctx: FrameworkContext<'_>,
    message: &serenity::Message,
) -> Result<()> {
    let data = framework_ctx.user_data();
    let ctx = framework_ctx.serenity_context;

    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };

    let (guild_row, user_row) = tokio::try_join!(
        data.guilds_db.get(guild_id.into()),
        data.userinfo_db.get(message.author.id.into()),
    )?;

    let Some((mut content, to_autojoin)) = run_checks(ctx, message, &guild_row, &user_row)? else {
        return Ok(());
    };

    let is_premium = data.is_premium_simple(&ctx.http, guild_id).await?;
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
            guild_row.skip_emoji(),
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
    let tts_auth_key = data.config.tts_service_auth_key.as_deref();
    let Some(audio) = fetch_audio(&data.reqwest, url.clone(), tts_auth_key).await? else {
        return Ok(());
    };

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

fn run_checks(
    ctx: &serenity::Context,
    message: &serenity::Message,
    guild_row: &GuildRow,
    user_row: &UserRow,
) -> Result<Option<(String, Option<serenity::ChannelId>)>> {
    if user_row.bot_banned() {
        return Ok(None);
    }

    let Some(guild) = message.guild(&ctx.cache) else {
        return Ok(None);
    };

    if guild_row.channel != Some(message.channel_id) {
        // "Text in Voice" works by just sending messages in voice channels, so checking for it just takes
        // checking if the message's channel_id is the author's voice channel_id
        if !guild_row.text_in_voice() {
            return Ok(None);
        }

        let author_vc = guild
            .voice_states
            .get(&message.author.id)
            .and_then(|c| c.channel_id);

        if author_vc.is_none_or(|author_vc| author_vc != message.channel_id) {
            return Ok(None);
        }
    }

    if let Some(required_role) = guild_row.required_role {
        let message_member = message.member.as_deref().try_unwrap()?;
        if !message_member.roles.contains(&required_role) {
            let Some(channel) = guild.channels.get(&message.channel_id) else {
                return Ok(None);
            };

            let author_permissions =
                guild.partial_member_permissions_in(channel, message.author.id, message_member);
            if !author_permissions.administrator() {
                return Ok(None);
            }
        }
    }

    let mut content = serenity::content_safe(
        &guild,
        &message.content,
        serenity::ContentSafeOptions::default()
            .clean_here(false)
            .clean_everyone(false)
            .show_discriminator(false),
        &message.mentions,
    );

    if content.len() >= 1500 {
        return Ok(None);
    }

    content = content.to_lowercase();

    if let Some(required_prefix) = &guild_row.required_prefix {
        if let Some(stripped_content) = content.strip_prefix(required_prefix.as_str()) {
            content = String::from(stripped_content);
        } else {
            return Ok(None);
        }
    }

    if content.starts_with(guild_row.prefix.as_str()) {
        return Ok(None);
    }

    let voice_state = guild.voice_states.get(&message.author.id);
    let bot_voice_state = guild.voice_states.get(&ctx.cache.current_user().id);

    let mut to_autojoin = None;
    if message.author.bot() {
        if guild_row.bot_ignore() || bot_voice_state.is_none() {
            return Ok(None); // Is bot
        }
    } else {
        // If the bot is in vc
        if let Some(vc) = bot_voice_state {
            // If the user needs to be in the vc, and the user's voice channel is not the same as the bot's
            if guild_row.require_voice()
                && vc.channel_id != voice_state.and_then(|vs| vs.channel_id)
            {
                return Ok(None); // Wrong vc
            }
        // Else if the user is in the vc and autojoin is on
        } else if let Some(voice_state) = voice_state
            && guild_row.auto_join()
        {
            to_autojoin = Some(voice_state.channel_id.try_unwrap()?);
        } else {
            return Ok(None); // Bot not in vc
        };

        if guild_row.require_voice() {
            let voice_channel = voice_state.unwrap().channel_id.try_unwrap()?;
            let channel = guild.channels.get(&voice_channel).try_unwrap()?;

            if channel.kind == serenity::ChannelType::Stage
                && voice_state.is_some_and(serenity::VoiceState::suppress)
                && guild_row.audience_ignore()
            {
                return Ok(None); // Is audience
            }
        }
    }

    Ok(Some((content, to_autojoin)))
}
