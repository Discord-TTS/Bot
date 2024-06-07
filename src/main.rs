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
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Ok;
use parking_lot::Mutex;

use poise::serenity_prelude as serenity;
use serenity::small_fixed_array::FixedString;

use tts_core::{
    analytics, create_db_handler, database,
    structs::{Data, PollyVoice, RegexCache, Result, TTSMode},
    translations,
};
use tts_tasks::Looper as _;

mod startup;

use startup::*;

fn main() -> Result<()> {
    let start_time = std::time::SystemTime::now();

    println!("Starting tokio runtime");
    std::env::set_var("RUST_LIB_BACKTRACE", "1");
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(_main(start_time))
}

async fn _main(start_time: std::time::SystemTime) -> Result<()> {
    println!("Loading and performing migrations");
    let (pool, config) = tts_migrations::load_db_and_conf().await?;

    println!("Initialising Http client");
    let reqwest = reqwest::Client::new();
    let auth_key = config.main.tts_service_auth_key.as_deref();

    let mut http_builder = serenity::HttpBuilder::new(config.main.token.as_deref().unwrap());
    if let Some(proxy) = &config.main.proxy_url {
        println!("Connecting via proxy");
        http_builder = http_builder
            .proxy(proxy.as_str())
            .ratelimiter_disabled(true);
    }

    let http = Arc::new(http_builder.build());

    println!("Performing big startup join");
    let tts_service = || config.main.tts_service.clone();
    let (
        translations,
        webhooks,
        guilds_db,
        userinfo_db,
        user_voice_db,
        guild_voice_db,
        nickname_db,
        gtts_voices,
        espeak_voices,
        gcloud_voices,
        polly_voices,
        translation_languages,
        premium_user,
    ) = tokio::try_join!(
        translations::read_files(),
        get_webhooks(&http, config.webhooks),
        create_db_handler!(pool.clone(), "guilds", "guild_id"),
        create_db_handler!(pool.clone(), "userinfo", "user_id"),
        create_db_handler!(pool.clone(), "user_voice", "user_id", "mode"),
        create_db_handler!(pool.clone(), "guild_voice", "guild_id", "mode"),
        create_db_handler!(pool.clone(), "nicknames", "guild_id", "user_id"),
        fetch_voices(&reqwest, tts_service(), auth_key, TTSMode::gTTS),
        fetch_voices(&reqwest, tts_service(), auth_key, TTSMode::eSpeak),
        fetch_voices(&reqwest, tts_service(), auth_key, TTSMode::gCloud),
        fetch_voices::<Vec<PollyVoice>>(&reqwest, tts_service(), auth_key, TTSMode::Polly),
        fetch_translation_languages(&reqwest, tts_service(), auth_key),
        async {
            let res = serenity::UserId::new(802632257658683442)
                .to_user(&http)
                .await?;

            println!("Loaded premium user");
            Ok(res)
        }
    )?;

    println!("Setting up webhook logging");
    tts_tasks::logging::WebhookLogger::init(
        http.clone(),
        webhooks.logs.clone(),
        webhooks.errors.clone(),
    );

    println!("Sending startup message");
    let startup_message = send_startup_message(&http, &webhooks.logs).await?;

    println!("Spawning analytics handler");
    let analytics = Arc::new(analytics::Handler::new(pool.clone()));
    tokio::spawn(analytics.clone().start());

    let data = Arc::new(Data {
        pool,
        translations,
        system_info: Mutex::new(sysinfo::System::new()),
        bot_list_tokens: Mutex::new(config.bot_list_tokens),

        fully_started: AtomicBool::new(false),
        join_vc_tokens: dashmap::DashMap::new(),
        songbird: songbird::Songbird::serenity(),
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
        regex_cache: RegexCache::new()?,
        guilds_db,
        userinfo_db,
        nickname_db,
        user_voice_db,
        guild_voice_db,
    });

    let framework_options = poise::FrameworkOptions {
        commands: tts_commands::commands(),
        event_handler: |fw_ctx, event| Box::pin(tts_events::listen(fw_ctx, event)),
        on_error: |error| {
            Box::pin(async move {
                let res = tts_core::errors::handle(error).await;
                res.unwrap_or_else(|err| tracing::error!("on_error: {:?}", err));
            })
        },
        allowed_mentions: Some(
            serenity::CreateAllowedMentions::default()
                .replied_user(true)
                .all_users(true),
        ),
        pre_command: analytics::pre_command,
        prefix_options: poise::PrefixFrameworkOptions {
            dynamic_prefix: Some(|ctx| Box::pin(tts_commands::get_prefix(ctx))),
            ..poise::PrefixFrameworkOptions::default()
        },
        command_check: Some(|ctx| Box::pin(tts_commands::command_check(ctx))),
        ..poise::FrameworkOptions::default()
    };

    let mut client = serenity::ClientBuilder::new_with_http(http, tts_events::get_intents())
        .voice_manager::<songbird::Songbird>(data.songbird.clone())
        .framework(poise::Framework::new(framework_options))
        .data(data as _)
        .await?;

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        wait_until_shutdown().await;

        tracing::warn!("Recieved control C and shutting down.");
        shard_manager.shutdown_all().await;
    });

    client.start_autosharded().await.map_err(Into::into)
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
