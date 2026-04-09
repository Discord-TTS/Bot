use std::{
    borrow::Cow,
    sync::{Arc, atomic::AtomicU64},
};

use poise::serenity_prelude as serenity;

use ::serenity::small_fixed_array::FixedString;
use tts_core::{
    database::{GuildRow, UserRow},
    opt_ext::OptionTryUnwrap as _,
    process_msg::{self, MessageContent, TTSMessageKind},
    structs::{Data, IsPremium, Result, TTSMode},
    voice,
};

pub(crate) async fn process_tts_msg(
    ctx: &serenity::Context,
    message: &serenity::Message,
) -> Result<()> {
    let data = ctx.data_ref::<Data>();
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };

    let (guild_row, user_row) = tokio::try_join!(
        data.guilds_db.get(guild_id.into()),
        data.userinfo_db.get(message.author.id.into()),
    )?;

    let Some(mut content) = run_checks(ctx, message, &guild_row, *user_row)? else {
        return Ok(());
    };

    let is_premium = data.is_premium_simple(&ctx.http, guild_id).await?;
    let (voice, mode) = {
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

        let get_should_announce = || voice::should_announce_name(data, guild_id, message.author.id);

        process_msg::clean(
            &mut content,
            &message.author,
            member_nick,
            &voice,
            guild_row.xsaid(),
            guild_row.skip_emoji(),
            guild_row.repeated_chars,
            nickname_row.name.as_deref(),
            &data.regex_cache,
            get_should_announce,
        );

        (voice, mode)
    };

    // Final check, make sure we aren't sending an empty message or just symbols.
    if content.text.find(|c| !" ?.)'!\":".contains(c)).is_none() {
        return Ok(());
    }

    // Try to join VC, if we are already in VC we will just get an error back.
    //
    // This also handles autojoining and cases where Voice Client and Voice Connection state have desync'd due to restarts.
    let voice_tx = {
        let author_voice_channel_id = {
            let guild = ctx.cache.guild(guild_id).try_unwrap()?;
            guild
                .voice_states
                .get(&message.author.id)
                .and_then(|vs| vs.channel_id)
                .try_unwrap()?
        };

        let voice_context = voice::VCContext {
            tts_service: data.config.pick_tts_service(guild_id).clone(),
            serenity: ctx.clone(),
            bot_id: ctx.cache.current_user().id,
            guild_id,
            channel_id: Arc::new(AtomicU64::new(author_voice_channel_id.get())),
        };

        match voice::start_connection(data, voice_context).await {
            voice::StartConnectionResult::Started(tx)
            | voice::StartConnectionResult::AlreadyIn((tx, _, _)) => tx,
            voice::StartConnectionResult::TimedOut => {
                return Ok(());
            }
        }
    };

    let tx_res = voice_tx.unbounded_send(voice::InterconnectMessage::QueueTTS(voice::GetTTS {
        text: content.text,
        mode,
        voice,
        preferred_format: None,
        max_length: Some(guild_row.msg_length),
        speaking_rate: Some(data.speaking_rate(message.author.id, mode).await?),
        translation_lang: guild_row
            .target_lang(IsPremium::from(is_premium))
            .map(FixedString::from_str_trunc),
    }));

    if tx_res.is_ok() {
        data.analytics.log(
            Cow::Borrowed(match mode {
                TTSMode::gTTS => "gTTS_tts",
                TTSMode::eSpeak => "eSpeak_tts",
                TTSMode::gCloud => "gCloud_tts",
                TTSMode::Polly => "Polly_tts",
            }),
            false,
        );
    }

    Ok(())
}

fn run_checks<'c>(
    ctx: &serenity::Context,
    message: &'c serenity::Message,
    guild_row: &GuildRow,
    user_row: UserRow,
) -> Result<Option<MessageContent<'c>>> {
    if user_row.bot_banned {
        return Ok(None);
    }

    let Some(guild) = message.guild(&ctx.cache) else {
        return Ok(None);
    };

    // `expect_channel` is fine as we are checking if the message is in the setup channel, or a voice channel.
    let channel_id = message.channel_id.expect_channel();
    if guild_row.channel != Some(channel_id) {
        // "Text in Voice" works by just sending messages in voice channels, so checking for it just takes
        // checking if the message's channel_id is the author's voice channel_id
        if !guild_row.text_in_voice() {
            return Ok(None);
        }

        let author_vc = guild
            .voice_states
            .get(&message.author.id)
            .and_then(|c| c.channel_id);

        if author_vc.is_none_or(|author_vc| author_vc != channel_id) {
            return Ok(None);
        }
    }

    if let Some(required_role) = guild_row.required_role
        && let Some(message_member) = &message.member
        && !message_member.roles.contains(&required_role)
    {
        let Some(channel) = guild.channels.get(&channel_id) else {
            return Ok(None);
        };

        if !guild
            .partial_member_permissions_in(channel, message.author.id, message_member)
            .administrator()
        {
            return Ok(None);
        }
    }

    // "A forwarded message can be identified by looking at its message_reference.type field"
    let kind = match &message.message_reference {
        Some(reference) if reference.kind == serenity::MessageReferenceKind::Forward => {
            TTSMessageKind::Forward
        }
        _ => TTSMessageKind::Default,
    };

    let content;
    let mentions;
    let attachments;
    if kind == TTSMessageKind::Forward {
        // "message_snapshots will be the message data associated with the forward. Currently we support only 1 snapshot."
        let snapshot = message.message_snapshots.first().try_unwrap()?;
        content = &*snapshot.content;
        mentions = &*snapshot.mentions;
        attachments = &*snapshot.attachments;
    } else {
        content = &*message.content;
        mentions = &*message.mentions;
        attachments = &*message.attachments;
    }

    let mut content = {
        let options = serenity::ContentSafeOptions::default()
            .clean_here(false)
            .clean_everyone(false);

        serenity::content_safe(&guild, content, options, mentions)
    };

    if content.len() >= 1500 {
        return Ok(None);
    }

    content = content.to_lowercase();

    if let Some(required_prefix) = &guild_row.required_prefix {
        if let Some(stripped_content) = content.strip_prefix(required_prefix.as_str())
            && kind != TTSMessageKind::Forward
        {
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
        } else if !guild_row.auto_join() {
            return Ok(None); // Bot not in vc and not auto joining
        }

        // If the user's voice channel is a stage, audience ignore is enabled, and the user is server muted: skip
        if let Some(voice_state) = voice_state
            && guild_row.audience_ignore()
        {
            let voice_channel_id = voice_state.channel_id.try_unwrap()?;
            let voice_channel = guild.channels.get(&voice_channel_id).try_unwrap()?;
            if voice_channel.base.kind == serenity::ChannelType::Stage && voice_state.suppress() {
                return Ok(None);
            }
        }
    }

    Ok(Some(MessageContent {
        text: content,
        kind,
        attachments,
    }))
}
