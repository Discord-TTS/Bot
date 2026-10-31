use aformat::aformat;
use reqwest::{
    StatusCode,
    header::{AUTHORIZATION, USER_AGENT},
};
use serenity::all as serenity;

use tts_core::{
    opt_ext::OptionTryUnwrap as _,
    structs::{Data, Result},
    version,
};

pub(crate) const PK_USER_ID: serenity::UserId = serenity::UserId::new(466378653216014359);
pub(super) async fn is_proxy_delete(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
    message_id: serenity::MessageId,
) -> Result<bool> {
    let Some(pluralkit_token) = data.config.pluralkit_token.as_deref() else {
        return Ok(false);
    };

    if is_pluralkit_in_guild(ctx, data, guild_id).await? {
        let bot_id = ctx.cache.current_user().id;

        let is_proxy = check_proxy_delete(&data.reqwest, pluralkit_token, bot_id, message_id).await;
        data.analytics.log(pk_event_from_result(&is_proxy), false);
        is_proxy
    } else {
        Ok(false)
    }
}

async fn is_pluralkit_in_guild(
    ctx: &serenity::Context,
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<bool> {
    if let Some(pluralkit_in_guild) = data.pluralkit_guild_status.get(&guild_id) {
        return Ok(*pluralkit_in_guild);
    }

    let pluralkit_in_guild = check_pluralkit_in_guild(ctx, guild_id).await?;
    data.pluralkit_guild_status
        .insert(guild_id, pluralkit_in_guild);
    Ok(pluralkit_in_guild)
}

async fn check_pluralkit_in_guild(
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
) -> Result<bool> {
    let is_in_cache = {
        let guild = ctx.cache.guild(guild_id).try_unwrap()?;
        guild.members.contains_key(&PK_USER_ID)
    };

    if is_in_cache {
        return Ok(true);
    }

    let result = ctx.http.get_member(guild_id, PK_USER_ID).await;
    if let Err(err) = result {
        if let serenity::Error::Http(serenity::HttpError::UnsuccessfulRequest(err)) = &err
            && err.status_code == StatusCode::NOT_FOUND
        {
            Ok(false)
        } else {
            Err(err.into())
        }
    } else {
        Ok(true)
    }
}

async fn check_proxy_delete(
    reqwest: &reqwest::Client,
    pluralkit_token: &str,
    bot_id: serenity::UserId,
    message_id: serenity::MessageId,
) -> Result<bool> {
    let url = aformat!("https://api.pluralkit.me/v2/messages/{message_id}");
    let version_hash = version::readable();

    let response = reqwest
        .get(&*url)
        .header(AUTHORIZATION, pluralkit_token)
        .header(USER_AGENT, &*aformat!("TTS Bot/{version_hash} ({bot_id})"))
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await?;

    match response.status() {
        StatusCode::OK => Ok(true),
        StatusCode::NOT_FOUND => Ok(false),
        StatusCode::TOO_MANY_REQUESTS => {
            tracing::warn!("Time to ask for a Pluralkit RL increase");
            Ok(false)
        }
        other => Err(anyhow::anyhow!("Unknown response from PK: {other}")),
    }
}

fn pk_event_from_result(is_proxy: &Result<bool>) -> std::borrow::Cow<'static, str> {
    if let Ok(true) = is_proxy {
        "pluralkit_proxy_delete"
    } else if let Ok(false) = is_proxy {
        "pluralkit_proxy_miss"
    } else {
        "pluralkit_proxy_err"
    }
    .into()
}
