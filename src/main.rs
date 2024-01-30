// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#![allow(stable_features)]
#![feature(let_chains)]
#![warn(
    rust_2018_idioms,
    missing_copy_implementations,
    noop_method_call,
    unused
)]
#![warn(clippy::pedantic)]
// clippy::pedantic complains about u64 -> i64 and back when db conversion, however it is fine
#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::cast_possible_truncation
)]
#![allow(
    clippy::unreadable_literal,
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::similar_names
)]

use std::{
    borrow::Cow,
    collections::BTreeMap,
    future::Future,
    num::NonZeroU16,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Ok;
use parking_lot::Mutex;
use tracing::{error, warn};

use poise::serenity_prelude::{self as serenity, builder::*, Mentionable as _};
use serenity::small_fixed_array::FixedString;

mod analytics;
mod bot_list_updater;
mod commands;
mod constants;
mod database;
mod database_models;
mod errors;
mod events;
mod funcs;
mod logging;
mod looper;
mod macros;
mod migration;
mod opt_ext;
mod structs;
mod traits;
mod translations;
mod web_updater;

use constants::PREMIUM_NEUTRAL_COLOUR;
use funcs::{get_translation_langs, prepare_gcloud_voices};
use looper::Looper;
use opt_ext::OptionTryUnwrap;
use structs::{
    Context, Data, FailurePoint, PartialContext, PollyVoice, Result, TTSMode, WebhookConfig,
    WebhookConfigRaw,
};
use traits::PoiseContextExt;
use translations::GetTextContextExt;

async fn wrap_in_res<T, E, Fut>(fut: Fut) -> Result<T>
where
    E: Into<anyhow::Error>,
    Fut: Future<Output = Result<T, E>>,
{
    fut.await.map_err(Into::into)
}

async fn get_webhooks(
    http: &serenity::Http,
    webhooks_raw: WebhookConfigRaw,
) -> Result<WebhookConfig> {
    let get_webhook = |url: reqwest::Url| async move {
        let (webhook_id, token) = serenity::parse_webhook(&url).try_unwrap()?;
        Ok(http.get_webhook_with_token(webhook_id, token).await?)
    };

    let (logs, errors, dm_logs) = tokio::try_join!(
        get_webhook(webhooks_raw.logs),
        get_webhook(webhooks_raw.errors),
        get_webhook(webhooks_raw.dm_logs),
    )?;

    Ok(WebhookConfig {
        logs,
        errors,
        dm_logs,
    })
}

fn main() -> Result<()> {
    let start_time = std::time::SystemTime::now();

    std::env::set_var("RUST_LIB_BACKTRACE", "1");
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(_main(start_time))
}

async fn _main(start_time: std::time::SystemTime) -> Result<()> {
    let (pool, config) = migration::load_db_and_conf().await?;

    let reqwest = reqwest::Client::new();
    let auth_key = config.main.tts_service_auth_key.as_deref();
    let http = Arc::new(serenity::Http::new(config.main.token.as_deref().unwrap()));

    let (
        webhooks,
        guilds_db,
        userinfo_db,
        user_voice_db,
        guild_voice_db,
        nickname_db,
        premium_user,
        gtts_voices,
        espeak_voices,
        gcloud_voices,
        polly_voices,
        translation_languages,
    ) = tokio::try_join!(
        get_webhooks(&http, config.webhooks),
        create_db_handler!(pool.clone(), "guilds", "guild_id"),
        create_db_handler!(pool.clone(), "userinfo", "user_id"),
        create_db_handler!(pool.clone(), "user_voice", "user_id", "mode"),
        create_db_handler!(pool.clone(), "guild_voice", "guild_id", "mode"),
        create_db_handler!(pool.clone(), "nicknames", "guild_id", "user_id"),
        wrap_in_res(serenity::UserId::new(802632257658683442).to_user(&http)),
        TTSMode::gTTS.fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key),
        TTSMode::eSpeak.fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key),
        TTSMode::gCloud.fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key),
        TTSMode::Polly.fetch_voices::<Vec<PollyVoice>>(
            config.main.tts_service.clone(),
            &reqwest,
            auth_key
        ),
        get_translation_langs(
            &reqwest,
            config.main.translation_url.as_ref(),
            config.main.translation_token.as_deref()
        ),
    )?;

    let analytics = Arc::new(analytics::Handler::new(pool.clone()));
    tokio::spawn(analytics.clone().start());

    let startup_builder = ExecuteWebhook::default().content("**TTS Bot is starting up**");
    let startup_message = webhooks
        .logs
        .execute(&http, true, startup_builder)
        .await?
        .unwrap()
        .id;

    let logger =
        logging::WebhookLogger::new(http.clone(), webhooks.logs.clone(), webhooks.errors.clone());

    tracing::subscriber::set_global_default(logger.clone())?;
    tokio::spawn(logger.0.start());

    let data = Arc::new(Data {
        pool,
        translations: translations::read_files()?,
        system_info: Mutex::new(sysinfo::System::new()),
        bot_list_tokens: Mutex::new(config.bot_list_tokens),

        fully_started: AtomicBool::new(false),
        join_vc_tokens: dashmap::DashMap::new(),
        songbird: songbird::Songbird::serenity(),
        currently_purging: AtomicBool::new(false),
        last_to_xsaid_tracker: dashmap::DashMap::new(),
        update_startup_lock: tokio::sync::Mutex::new(()),

        gtts_voices,
        espeak_voices,
        translation_languages,
        gcloud_voices: prepare_gcloud_voices(gcloud_voices),
        polly_voices: polly_voices
            .into_iter()
            .map(|v| (v.id.clone(), v))
            .collect::<BTreeMap<_, _>>(),

        website_info: Mutex::new(config.website_info),
        config: config.main,
        reqwest,
        premium_avatar_url: FixedString::from_string_trunc(premium_user.face()),
        analytics,
        webhooks,
        start_time,
        startup_message,
        regex_cache: structs::RegexCache::new()?,
        guilds_db,
        userinfo_db,
        nickname_db,
        user_voice_db,
        guild_voice_db,
    });

    let framework_options = poise::FrameworkOptions {
        commands: commands::commands(),
        event_handler: |fw_ctx, event| Box::pin(events::listen(fw_ctx, event)),
        on_error: |error| {
            Box::pin(async move {
                let res = errors::handle(error).await;
                res.unwrap_or_else(|err| error!("on_error: {:?}", err));
            })
        },
        allowed_mentions: Some(
            serenity::CreateAllowedMentions::default()
                .replied_user(true)
                .all_users(true),
        ),
        pre_command: analytics::pre_command,
        prefix_options: poise::PrefixFrameworkOptions {
            dynamic_prefix: Some(|ctx| Box::pin(get_prefix(ctx))),
            ..poise::PrefixFrameworkOptions::default()
        },
        command_check: Some(|ctx| Box::pin(command_check(ctx))),
        ..poise::FrameworkOptions::default()
    };

    let mut client = serenity::ClientBuilder::new_with_http(http, events::get_intents())
        .voice_manager::<songbird::Songbird>(data.songbird.clone())
        .framework(poise::Framework::new(framework_options))
        .data(data as _)
        .await?;

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        wait_until_shutdown().await;

        warn!("Recieved control C and shutting down.");
        shard_manager.shutdown_all().await;
    });

    client.start_autosharded().await.map_err(Into::into)
}

async fn get_prefix(ctx: PartialContext<'_>) -> Result<Option<String>> {
    let prefix = match ctx.guild_id {
        Some(guild_id) => {
            let data = ctx.framework.user_data();
            let row = data.guilds_db.get(guild_id.into()).await?;
            String::from(row.prefix.as_str())
        }
        None => String::from("-"),
    };

    Ok(Some(prefix))
}

async fn command_check(ctx: Context<'_>) -> Result<bool> {
    if ctx.author().bot() {
        return Ok(false);
    };

    let Some(guild_id) = ctx.guild_id() else {
        return Ok(true);
    };

    let guild_row = ctx.data().guilds_db.get(guild_id.into()).await?;
    let Some(required_role) = guild_row.required_role else {
        return Ok(true);
    };

    let member = ctx.author_member().await.try_unwrap()?;

    let is_admin = || {
        let guild = require_guild!(ctx, Ok(false));
        let channel = guild.channels.get(&ctx.channel_id()).try_unwrap()?;

        let permissions = guild.user_permissions_in(channel, &member);
        Ok(permissions.administrator())
    };

    if member.roles.contains(&required_role) || is_admin()? {
        return Ok(true);
    };

    let msg = ctx
        .gettext(
            "You do not have the required role to use this bot, ask a server administrator for {}.",
        )
        .replace("{}", &required_role.mention().to_string());

    ctx.send_error(msg).await?;
    Ok(false)
}

async fn premium_command_check(ctx: Context<'_>) -> Result<bool> {
    if let Context::Application(ctx) = ctx {
        if ctx.interaction_type == poise::CommandInteractionType::Autocomplete {
            // Ignore the premium check during autocomplete.
            return Ok(true);
        }
    }

    let data = ctx.data();
    let guild_id = ctx.guild_id();
    let serenity_ctx = ctx.serenity_context();

    let main_msg =
        match data.premium_check(guild_id).await? {
            None => return Ok(true),
            Some(FailurePoint::Guild) => Cow::Borrowed("Hey, this is a premium command so it must be run in a server!"),
            Some(FailurePoint::PremiumUser) => Cow::Borrowed("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`/donate`)"),
            Some(FailurePoint::NotSubscribed(premium_user_id)) => {
                let premium_user = premium_user_id.to_user(serenity_ctx).await?;
                Cow::Owned(format!(concat!(
                    "Hey, this server has a premium user setup, however they are not longer a patreon! ",
                    "Please ask {}#{} to renew their membership."
                ), premium_user.name, premium_user.discriminator.map_or(0, NonZeroU16::get)))
            }
        };

    let author = ctx.author();
    warn!(
        "{}#{} | {} failed the premium check in {}",
        author.name,
        author.discriminator.map_or(0, NonZeroU16::get),
        author.id,
        guild_id
            .and_then(|g_id| serenity_ctx.cache.guild(g_id))
            .map_or(Cow::Borrowed("DMs"), |g| (Cow::Owned(format!(
                "{} | {}",
                g.name, g.id
            ))))
    );

    let permissions = ctx.author_permissions().await?;
    if permissions.send_messages() {
        let builder = poise::CreateReply::default();
        ctx.send({
            const FOOTER_MSG: &str = "If this is an error, please contact GnomedDev.";
            if permissions.embed_links() {
                let embed = CreateEmbed::default()
                    .title("TTS Bot Premium - Premium Only Command!")
                    .description(main_msg)
                    .colour(PREMIUM_NEUTRAL_COLOUR)
                    .thumbnail(data.premium_avatar_url.as_str())
                    .footer(serenity::CreateEmbedFooter::new(FOOTER_MSG));

                builder.embed(embed)
            } else {
                builder.content(format!("{main_msg}\n{FOOTER_MSG}"))
            }
        })
        .await?;
    }

    Ok(false)
}

#[cfg(unix)]
async fn wait_until_shutdown() {
    use tokio::signal::unix as signal;

    let [mut s1, mut s2, mut s3] = [
        signal::signal(signal::SignalKind::hangup()).unwrap(),
        signal::signal(signal::SignalKind::interrupt()).unwrap(),
        signal::signal(signal::SignalKind::terminate()).unwrap(),
    ];

    tokio::select!(
        v = s1.recv() => v.unwrap(),
        v = s2.recv() => v.unwrap(),
        v = s3.recv() => v.unwrap(),
    );
}

#[cfg(windows)]
async fn wait_until_shutdown() {
    let (mut s1, mut s2) = (
        tokio::signal::windows::ctrl_c().unwrap(),
        tokio::signal::windows::ctrl_break().unwrap(),
    );

    tokio::select!(
        v = s1.recv() => v.unwrap(),
        v = s2.recv() => v.unwrap(),
    );
}
