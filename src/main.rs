use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock, atomic::AtomicBool},
    time::Duration,
};

use anyhow::Ok;
use parking_lot::Mutex;
use small_fixed_array::FixedArray;

use poise::serenity_prelude as serenity;
use serenity::small_fixed_array::FixedString;

use tts_core::{
    analytics, create_db_handler, database,
    structs::{Data, PollyVoice, RegexCache, Result, TTSMode},
};
use tts_events::EventHandler;
use tts_tasks::Looper as _;

mod startup;

use startup::*;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn main() -> Result<()> {
    // SAFETY: No other threads have been spawned.
    unsafe { std::env::set_var("RUST_LIB_BACKTRACE", "1") };

    let start_time = std::time::SystemTime::now();

    println!("Starting tokio runtime");
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(main_(start_time))
}

async fn main_(start_time: std::time::SystemTime) -> Result<()> {
    println!("Loading and performing migrations");
    let (pool, config) = tts_migrations::load_db_and_conf().await?;

    println!("Initialising Http client");
    let reqwest = reqwest::Client::new();

    let token = config.main.token.clone();
    let mut http_builder = serenity::HttpBuilder::new(token.clone());
    if let Some(proxy) = &config.main.proxy_url {
        println!("Connecting via proxy");
        http_builder = http_builder
            .proxy(proxy.as_str())
            .ratelimiter_disabled(true);
    }

    let http = Arc::new(http_builder.build());

    println!("Performing big startup join");
    let tts_service = || config.tts_services[0].url.clone();
    let (
        ws_connections,
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
        shard_count,
        premium_user,
    ) = tokio::try_join!(
        setup_ws_stream(&config.tts_services),
        get_webhooks(&http, config.webhooks),
        create_db_handler!(pool.clone(), "guilds", "guild_id"),
        create_db_handler!(pool.clone(), "userinfo", "user_id"),
        create_db_handler!(pool.clone(), "user_voice", "user_id", "mode"),
        create_db_handler!(pool.clone(), "guild_voice", "guild_id", "mode"),
        create_db_handler!(pool.clone(), "nicknames", "guild_id", "user_id"),
        fetch_voices(&reqwest, tts_service(), TTSMode::gTTS),
        fetch_voices(&reqwest, tts_service(), TTSMode::eSpeak),
        fetch_voices(&reqwest, tts_service(), TTSMode::gCloud),
        fetch_voices::<Vec<PollyVoice>>(&reqwest, tts_service(), TTSMode::Polly),
        fetch_translation_languages(&reqwest, tts_service()),
        async { Ok(http.get_bot_gateway().await?.shards) },
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
        analytics,
        guilds_db,
        userinfo_db,
        nickname_db,
        user_voice_db,
        guild_voice_db,

        entitlement_cache: mini_moka::sync::Cache::builder()
            .time_to_live(Duration::from_hours(1))
            .build(),
        startup_message,
        premium_avatar_url: FixedString::from_string_trunc(premium_user.face()),
        system_info: Mutex::new(sysinfo::System::new()),
        start_time,
        reqwest,
        regex_cache: RegexCache::new()?,
        webhooks,
        pool,

        service_weight_lookups: FixedArray::try_from(
            config
                .tts_services
                .iter()
                .enumerate()
                .flat_map(|(index, service)| {
                    (1..=service.weight.get()).map(move |_| index.try_into().unwrap())
                })
                .collect::<Box<[_]>>(),
        )
        .unwrap(),
        tts_services: FixedArray::try_from(
            config
                .tts_services
                .into_iter()
                .map(|service| service.url)
                .collect::<Box<[_]>>(),
        )
        .unwrap(),
        ws_connections,
        voice_connections: Mutex::default(),

        config: config.main,
        premium_config: config.premium,
        runners: OnceLock::new(), // Filled in later

        website_info: Mutex::new(config.website_info),
        bot_list_tokens: Mutex::new(config.bot_list_tokens),
        fully_started: AtomicBool::new(false),
        update_startup_lock: tokio::sync::Mutex::new(()),

        espeak_voices,
        gtts_voices,
        polly_voices: polly_voices
            .into_iter()
            .map(|v| (v.id.clone(), v))
            .collect::<BTreeMap<_, _>>(),
        gcloud_voices: prepare_gcloud_voices(gcloud_voices),

        translation_languages,
    });

    let framework_options = poise::FrameworkOptions {
        commands: tts_commands::commands(),
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
            stripped_dynamic_prefix: Some(|ctx, message, _| {
                Box::pin(tts_commands::try_strip_prefix(ctx, message))
            }),
            ..poise::PrefixFrameworkOptions::default()
        },
        command_check: Some(|ctx| Box::pin(tts_commands::command_check(ctx))),
        ..poise::FrameworkOptions::default()
    };

    let data_clone = Arc::clone(&data);
    let mut client = serenity::ClientBuilder::new_with_http(token, http, tts_events::get_intents())
        .framework(Box::new(poise::Framework::new(framework_options)))
        .compression(serenity::TransportCompression::Zstd)
        .event_handler(Arc::new(EventHandler))
        .data(data as _)
        .await?;

    let shard_runners = Arc::clone(&client.shard_manager.runners);
    if data_clone.runners.set(shard_runners).is_err() {
        unreachable!()
    }

    let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
    tokio::spawn(async move {
        wait_until_shutdown().await;

        tracing::warn!("Recieved control C and shutting down.");
        shutdown_trigger();
    });

    start_ws_health_checks(&data_clone);
    client
        .start_shards(shard_count.get())
        .await
        .map_err(Into::into)
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
