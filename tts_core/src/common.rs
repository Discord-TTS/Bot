use std::borrow::Cow;

use rand::Rng as _;

use serenity::all as serenity;
use serenity::{CollectComponentInteractions, CreateActionRow, CreateButton, CreateComponent};

use crate::structs::{Context, Data, Result, TTSMode, TTSServiceError};

pub(crate) fn timestamp_in_future(ts: serenity::Timestamp) -> bool {
    *ts > chrono::Utc::now()
}

/// Builds components for invite command and invite link in DMs
///
/// This has to allocate internally due to lifetime issues with trying CPS.
pub fn build_invite_components(
    bot_id: serenity::UserId,
    main_server_invite: &str,
) -> serenity::CreateComponent<'_> {
    let suggested_bot_permissions = serenity::Permissions::VIEW_CHANNEL
        | serenity::Permissions::SEND_MESSAGES
        | serenity::Permissions::ADD_REACTIONS
        | serenity::Permissions::ATTACH_FILES
        | serenity::Permissions::EMBED_LINKS
        | serenity::Permissions::CONNECT
        | serenity::Permissions::SPEAK
        | serenity::Permissions::USE_VAD;

    let bot_invite_link = format!(
        "https://discord.com/oauth2/authorize?client_id={bot_id}&permissions={}&scope=bot%20applications.commands",
        suggested_bot_permissions.bits()
    );

    let buttons = vec![
        CreateButton::new_link(bot_invite_link).label("Invite the Bot!"),
        CreateButton::new_link(main_server_invite).label("Get Support!"),
    ];

    CreateComponent::ActionRow(CreateActionRow::buttons(buttons))
}

pub fn push_permission_names(buffer: &mut String, permissions: serenity::Permissions) {
    let permission_names = permissions.get_permission_names();
    for (i, permission) in permission_names.iter().enumerate() {
        buffer.push_str(permission);
        if i != permission_names.len() - 1 {
            buffer.push_str(", ");
        }
    }
}

pub async fn remove_premium(data: &Data, guild_id: serenity::GuildId) -> Result<()> {
    tokio::try_join!(
        data.guilds_db
            .set_one(guild_id.into(), "premium_user", None::<i64>),
        data.guilds_db
            .set_one(guild_id.into(), "voice_mode", TTSMode::default()),
    )?;

    Ok(())
}

pub async fn dm_generic(
    ctx: &serenity::Context,
    author: &serenity::User,
    target: serenity::UserId,
    mut target_tag: String,
    attachment_url: Option<&str>,
    message: &str,
) -> Result<(String, serenity::Embed)> {
    let mut embed = serenity::CreateEmbed::default();
    if let Some(url) = attachment_url {
        embed = embed.image(url);
    }

    let embeds = [embed
        .title("Message from the developers:")
        .description(message)
        .author(serenity::CreateEmbedAuthor::new(author.tag()).icon_url(author.face()))];

    let sent = target
        .dm(
            &ctx.http,
            serenity::CreateMessage::default().embeds(&embeds),
        )
        .await?;

    target_tag.insert_str(0, "Sent message to: ");
    Ok((target_tag, sent.embeds.into_iter().next().unwrap()))
}

pub async fn fetch_audio(
    reqwest: &reqwest::Client,
    url: reqwest::Url,
    auth_key: Option<&str>,
) -> Result<Option<reqwest::Response>> {
    let resp = reqwest
        .get(url)
        .header(reqwest::header::AUTHORIZATION, auth_key.unwrap_or(""))
        .send()
        .await?;

    match resp.error_for_status_ref() {
        Ok(_) => Ok(Some(resp)),
        Err(backup_err) => match resp.json::<TTSServiceError>().await {
            Ok(err) => {
                if err.code.should_ignore() {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("Error fetching audio: {}", err.display))
                }
            }
            Err(_) => Err(backup_err.into()),
        },
    }
}

#[must_use]
pub fn prepare_url(
    mut tts_service: reqwest::Url,
    content: &str,
    lang: &str,
    mode: TTSMode,
    speaking_rate: &str,
    max_length: &str,
    translation_lang: Option<&str>,
) -> reqwest::Url {
    {
        let mut params = tts_service.query_pairs_mut();
        params.append_pair("text", content);
        params.append_pair("lang", lang);
        params.append_pair("mode", mode.into());
        params.append_pair("max_length", max_length);
        params.append_pair("preferred_format", "mp3");
        params.append_pair("speaking_rate", speaking_rate);

        if let Some(translation_lang) = translation_lang {
            params.append_pair("translation_lang", translation_lang);
        }

        params.finish();
    }

    tts_service.set_path("tts");
    tts_service
}

#[must_use]
pub fn random_footer(server_invite: &str, client_id: serenity::UserId) -> Cow<'static, str> {
    match rand::rng().random_range(0..4) {
        0 => Cow::Owned(format!(
            "If you find a bug or want to ask a question, join the support server: {server_invite}"
        )),
        1 => Cow::Owned(format!(
            "You can vote for me or review me on top.gg!\nhttps://top.gg/bot/{client_id}"
        )),
        2 => Cow::Borrowed(
            "If you want to support the development and hosting of TTS Bot, check out `/premium info`!",
        ),
        3 => Cow::Borrowed("There are loads of customizable settings, check out `/help set`"),
        _ => unreachable!(),
    }
}

pub fn confirm_dialog_buttons<'a>(positive: &'a str, negative: &'a str) -> [CreateButton<'a>; 2] {
    [
        CreateButton::new("True")
            .style(serenity::ButtonStyle::Success)
            .label(positive),
        CreateButton::new("False")
            .style(serenity::ButtonStyle::Danger)
            .label(negative),
    ]
}

pub async fn confirm_dialog_wait(
    ctx: &serenity::Context,
    message_id: serenity::MessageId,
    author_id: serenity::UserId,
) -> Result<Option<bool>> {
    let interaction = message_id
        .collect_component_interactions(ctx)
        .timeout(std::time::Duration::from_secs(60 * 5))
        .author_id(author_id)
        .await;

    if let Some(interaction) = interaction {
        interaction.defer(&ctx.http).await?;
        match &*interaction.data.custom_id {
            "True" => Ok(Some(true)),
            "False" => Ok(Some(false)),
            _ => unreachable!(),
        }
    } else {
        Ok(None)
    }
}

pub async fn confirm_dialog(
    ctx: Context<'_>,
    prompt: &str,
    positive: &str,
    negative: &str,
) -> Result<Option<bool>> {
    let buttons = confirm_dialog_buttons(positive, negative);
    let components = CreateComponent::ActionRow(CreateActionRow::buttons(&buttons));
    let builder = poise::CreateReply::default()
        .content(prompt)
        .ephemeral(true)
        .components(std::slice::from_ref(&components));

    let reply = ctx.send(builder).await?;
    let message = reply.message().await?;

    confirm_dialog_wait(ctx.serenity_context(), message.id, ctx.author().id).await
}

/// Avoid char boundary panics with utf8 chars
pub fn safe_truncate(string: &mut String, mut new_len: usize) {
    if string.len() <= new_len {
        return;
    }

    new_len -= 3;
    while !string.is_char_boundary(new_len) {
        new_len -= 1;
    }

    string.truncate(new_len);
    string.push_str("...");
}
