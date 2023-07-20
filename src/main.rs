// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

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
    clippy::similar_names
)]

use std::{
    borrow::Cow,
    collections::BTreeMap,
    num::NonZeroU16,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Ok;
use parking_lot::{Mutex, RwLock};
use sysinfo::SystemExt;
use tracing::{error, warn};

use gnomeutils::{
    analytics, logging, require, require_guild, Looper, OptionTryUnwrap, PoiseContextExt,
};
use poise::serenity_prelude::{self as serenity, builder::*, Mentionable as _};

mod commands;
mod constants;
mod database;
mod events;
mod funcs;
mod macros;
mod migration;
mod structs;
mod traits;
mod web_updater;

use constants::PREMIUM_NEUTRAL_COLOUR;
use funcs::{decode_resp, get_translation_langs, prepare_gcloud_voices};
use structs::{
    Config, Context, Data, DataInner, FailurePoint, PollyVoice, PostgresConfig, Result, TTSMode,
    WebhookConfig, WebhookConfigRaw,
};

enum EntryCheck {
    IsFile,
    IsDir,
}

async fn get_webhooks(
    http: &serenity::Http,
    webhooks_raw: WebhookConfigRaw,
) -> Result<WebhookConfig> {
    let get_webhook = |url: reqwest::Url| async move {
        let (webhook_id, token) = serenity::parse_webhook(&url).try_unwrap()?;
        Ok(http.get_webhook_with_token(webhook_id, token).await?)
    };

    let (logs, errors, servers, dm_logs) = tokio::try_join!(
        get_webhook(webhooks_raw.logs),
        get_webhook(webhooks_raw.errors),
        get_webhook(webhooks_raw.servers),
        get_webhook(webhooks_raw.dm_logs),
    )?;

    Ok(WebhookConfig {
        logs,
        servers,
        dm_logs,
        errors: Some(errors),
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

#[allow(clippy::too_many_lines)]
async fn _main(start_time: std::time::SystemTime) -> Result<()> {
    let (pool, mut config) = {
        let mut config_toml: toml::Value = std::fs::read_to_string("config.toml")?.parse()?;
        let postgres: PostgresConfig =
            toml::Value::try_into(config_toml["PostgreSQL-Info"].clone())?;

        let pool_config = sqlx::postgres::PgPoolOptions::new();
        let pool_config = if let Some(max_connections) = postgres.max_connections {
            pool_config.max_connections(max_connections)
        } else {
            pool_config
        };

        let pool = pool_config
            .connect_with(
                sqlx::postgres::PgConnectOptions::new()
                    .host(&postgres.host)
                    .username(&postgres.user)
                    .database(&postgres.database)
                    .password(&postgres.password),
            )
            .await?;

        migration::run(&mut config_toml, &pool).await?;

        let config: Config = config_toml.try_into()?;
        (pool, config)
    };

    let filter_entry = |to_check| {
        move |entry: &std::fs::DirEntry| {
            entry
                .metadata()
                .map(|m| match to_check {
                    EntryCheck::IsFile => m.is_file(),
                    EntryCheck::IsDir => m.is_dir(),
                })
                .unwrap_or(false)
        }
    };

    let translations = std::fs::read_dir("translations")?
        .map(Result::unwrap)
        .filter(filter_entry(EntryCheck::IsDir))
        .flat_map(|d| {
            std::fs::read_dir(d.path())
                .unwrap()
                .map(Result::unwrap)
                .filter(filter_entry(EntryCheck::IsFile))
                .filter(|e| e.path().extension().map_or(false, |e| e == "mo"))
                .map(|entry| {
                    Ok((
                        entry
                            .file_name()
                            .to_str()
                            .unwrap()
                            .split('.')
                            .next()
                            .unwrap()
                            .to_string(),
                        gettext::Catalog::parse(std::fs::File::open(entry.path())?)?,
                    ))
                })
                .filter_map(Result::ok)
        })
        .collect();

    let reqwest = reqwest::Client::new();
    let songbird = songbird::Songbird::serenity();
    let auth_key = config.main.tts_service_auth_key.as_deref();
    let http = serenity::Http::new(config.main.token.as_deref().unwrap());

    let (
        guilds_db,
        userinfo_db,
        user_voice_db,
        guild_voice_db,
        nickname_db,
        mut webhooks,
        translation_languages,
        premium_avatar_url,
        gtts_voices,
        espeak_voices,
        gcloud_voices,
        polly_voices,
    ) = tokio::try_join!(
        create_db_handler!(pool.clone(), "guilds", "guild_id"),
        create_db_handler!(pool.clone(), "userinfo", "user_id"),
        create_db_handler!(pool.clone(), "user_voice", "user_id", "mode"),
        create_db_handler!(pool.clone(), "guild_voice", "guild_id", "mode"),
        create_db_handler!(pool.clone(), "nicknames", "guild_id", "user_id"),
        get_webhooks(&http, config.webhooks),
        get_translation_langs(
            &reqwest,
            &config.main.translation_url,
            &config.main.translation_token
        ),
        async {
            serenity::UserId::new(802632257658683442)
                .to_user(&http)
                .await
                .map(|u| u.face())
                .map_err(Into::into)
        },
        async {
            Ok(decode_resp::<BTreeMap<String, String>>(
                TTSMode::gTTS
                    .fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key)
                    .await?,
            )
            .await?)
        },
        async {
            Ok(decode_resp::<Vec<String>>(
                TTSMode::eSpeak
                    .fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key)
                    .await?,
            )
            .await?)
        },
        async {
            Ok(prepare_gcloud_voices(
                decode_resp(
                    TTSMode::gCloud
                        .fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key)
                        .await?,
                )
                .await?,
            ))
        },
        async {
            Ok(decode_resp::<Vec<PollyVoice>>(
                TTSMode::Polly
                    .fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key)
                    .await?,
            )
            .await?
            .into_iter()
            .map(|v| (v.id.clone(), v))
            .collect::<BTreeMap<_, _>>())
        },
    )?;

    let analytics = Arc::new(analytics::Handler::new(pool.clone()));
    tokio::spawn(analytics.clone().start());

    let startup_message = webhooks
        .logs
        .execute(
            &http,
            true,
            ExecuteWebhook::default().content("**TTS Bot is starting up**"),
        )
        .await?
        .unwrap()
        .id;

    let logger = logging::WebhookLogger::new(
        Arc::new(http),
        "discord_tts_bot",
        "TTS-Webhook",
        tracing::Level::from_str(&config.main.log_level)?,
        webhooks.logs.clone(),
        webhooks.errors.clone().unwrap(),
    );

    tracing::subscriber::set_global_default(logger.clone())?;
    tokio::spawn(logger.0.start());

    let token = config.main.token.take().unwrap();
    let regex_cache = structs::RegexCache {
        replacements: [
            (
                regex::Regex::new(r"\|\|(?s:.)*?\|\|")?,
                ". spoiler avoided.",
            ),
            (regex::Regex::new(r"```(?s:.)*?```")?, ". code block."),
            (regex::Regex::new(r"`(?s:.)*?`")?, ". code snippet."),
        ],
        id_in_brackets: regex::Regex::new(r"\((\d+)\)")?,
        emoji: regex::Regex::new(r"<(a?):([^<>]+):\d+>")?,
    };

    let data = Data(Arc::new(DataInner {
        bot_list_tokens: config.bot_list_tokens,
        inner: gnomeutils::GnomeData {
            pool,
            translations,
            error_webhook: webhooks.errors.take().unwrap(),
            system_info: Mutex::new(sysinfo::System::new()),
            main_server_invite: config.main.main_server_invite.clone(),
        },

        songbird: songbird.clone(),
        fully_started: AtomicBool::new(false),
        join_vc_tokens: dashmap::DashMap::new(),
        currently_purging: AtomicBool::new(false),
        last_to_xsaid_tracker: dashmap::DashMap::new(),

        gtts_voices,
        espeak_voices,
        gcloud_voices,
        polly_voices,
        translation_languages,

        website_info: RwLock::new(config.website_info),
        config: config.main,
        reqwest,
        premium_avatar_url,
        analytics,
        webhooks,
        start_time,
        startup_message,
        regex_cache,
        guilds_db,
        userinfo_db,
        nickname_db,
        user_voice_db,
        guild_voice_db,
    }));

    let intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::GUILD_VOICE_STATES
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework_options = poise::FrameworkOptions {
        commands: commands::commands(),
        listener: |event, ctx, _| Box::pin(events::listen(ctx, event)),
        on_error: |error| {
            Box::pin(async move {
                gnomeutils::errors::handle(error)
                    .await
                    .unwrap_or_else(|err| error!("on_error: {:?}", err));
            })
        },
        allowed_mentions: Some(
            serenity::CreateAllowedMentions::default()
                .replied_user(true)
                .all_users(true),
        ),
        pre_command: |ctx| {
            Box::pin(async move {
                let analytics_handler: &analytics::Handler = &ctx.data().analytics;

                analytics_handler.log(Cow::Owned(ctx.command().qualified_name.clone()), true);
                analytics_handler.log(
                    Cow::Borrowed(match ctx {
                        poise::Context::Prefix(_) => "command",
                        poise::Context::Application(ctx) => match ctx.interaction {
                            poise::CommandOrAutocompleteInteraction::Autocomplete(_) => {
                                "autocomplete"
                            }
                            poise::CommandOrAutocompleteInteraction::Command(_) => "slash_command",
                        },
                    }),
                    false,
                );
            })
        },
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: None,
            dynamic_prefix: Some(|ctx| {
                Box::pin(async move {
                    Ok(Some(match ctx.guild_id.map(Into::into) {
                        Some(guild_id) => ctx.data.guilds_db.get(guild_id).await?.prefix.clone(),
                        None => String::from("-"),
                    }))
                })
            }),
            ..poise::PrefixFrameworkOptions::default()
        },
        command_check: Some(|ctx| {
            Box::pin(async move {
                if ctx.author().bot {
                Ok(false)
            } else if let Some(guild_id) = ctx.guild_id() && let Some(required_role) = ctx.data().guilds_db.get(guild_id.into()).await?.required_role {
                let required_role = serenity::RoleId::new(required_role as u64);
                let member = ctx.author_member().await.try_unwrap()?;

                let is_admin = || {
                    let guild = require_guild!(ctx, Ok(false));
                    let channel = guild.channels.get(&ctx.channel_id()).try_unwrap()?;

                    let permissions = guild.user_permissions_in(channel, &member);
                    Ok(permissions.administrator())
                };

                if member.roles.contains(&required_role) || is_admin()? {
                    Ok(true)
                } else {
                    ctx.send_error(
                        "you do not have the required role to use this command",
                        Some(&format!("ask a server admin for {}", required_role.mention()))
                    ).await.map(|_| false).map_err(Into::into)
                }
            } else {
                Ok(true)
            }
            })
        }),
        ..poise::FrameworkOptions::default()
    };

    let mut client = serenity::Client::builder(token, intents)
        .voice_manager_arc(songbird)
        .framework(poise::Framework::new(framework_options, |_, _, _| {
            Box::pin(async { Ok(data) })
        }))
        .await?;

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        #[cfg(unix)]
        {
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
        {
            let (mut s1, mut s2) = (
                tokio::signal::windows::ctrl_c().unwrap(),
                tokio::signal::windows::ctrl_break().unwrap(),
            );

            tokio::select!(
                v = s1.recv() => v.unwrap(),
                v = s2.recv() => v.unwrap(),
            );
        }

        warn!("Recieved control C and shutting down.");
        shard_manager.lock().await.shutdown_all().await;
    });

    client.start_autosharded().await.map_err(Into::into)
}

#[allow(unreachable_code, unused)]
async fn premium_command_check(ctx: Context<'_>) -> Result<bool> {
    return Ok(true);
    if let Context::Application(ctx) = ctx {
        if let poise::CommandOrAutocompleteInteraction::Autocomplete(_) = ctx.interaction {
            // Ignore the premium check during autocomplete.
            return Ok(true);
        }
    }

    let guild_id = ctx.guild_id();
    let ctx_discord = ctx.discord();
    let data = ctx.data();

    let main_msg =
        match data.premium_check(guild_id).await? {
            None => return Ok(true),
            Some(FailurePoint::Guild) => Cow::Borrowed("Hey, this is a premium command so it must be run in a server!"),
            Some(FailurePoint::PremiumUser) => Cow::Borrowed("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`/donate`)"),
            Some(FailurePoint::NotSubscribed(premium_user_id)) => {
                let premium_user = premium_user_id.to_user(ctx_discord).await?;
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
            .and_then(|g_id| ctx_discord
                .cache
                .guild(g_id)
                .map(|g| (Cow::Owned(format!("{} | {g_id}", g.name)))))
            .unwrap_or(Cow::Borrowed("DMs"))
    );

    let permissions = ctx.author_permissions().await?;
    if permissions.send_messages() {
        let builder = poise::CreateReply::default();
        ctx.send({
            const FOOTER_MSG: &str = "If this is an error, please contact Gnome!#6669.";
            if permissions.embed_links() {
                builder.embed(
                    CreateEmbed::default()
                        .title("TTS Bot Premium - Premium Only Command!")
                        .description(main_msg)
                        .colour(PREMIUM_NEUTRAL_COLOUR)
                        .thumbnail(&data.premium_avatar_url)
                        .footer(serenity::CreateEmbedFooter::new(FOOTER_MSG)),
                )
            } else {
                builder.content(format!("{main_msg}\n{FOOTER_MSG}"))
            }
        })
        .await?;
    }

    Ok(false)
}
