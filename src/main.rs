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

#![feature(let_chains, must_not_suspend)]

#![warn(rust_2018_idioms, missing_copy_implementations, must_not_suspend, noop_method_call, unused)]
#![warn(clippy::pedantic)]

// clippy::pedantic complains about u64 -> i64 and back when db conversion, however it is fine
#![allow(clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::cast_lossless, clippy::cast_possible_truncation)]
#![allow(clippy::unreadable_literal)]

use std::{borrow::Cow, collections::BTreeMap, path::Path, str::FromStr, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use anyhow::Ok;
use sysinfo::SystemExt;
use once_cell::sync::OnceCell;
use tracing::{error, info, warn};

use gnomeutils::{analytics, errors, logging, Looper, require, OptionTryUnwrap, PoiseContextExt, require_guild};
use poise::serenity_prelude::{self as serenity, Mentionable as _, json::prelude as json};
use songbird::SerenityInit; // adds serenity::ClientBuilder.register_songbird

mod migration;
mod constants;
mod database;
mod commands;
mod structs;
mod traits;
mod macros;
mod funcs;

use traits::SerenityContextExt;
use constants::{DM_WELCOME_MESSAGE, FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};
use funcs::{clean_msg, run_checks, random_footer, generate_status, prepare_gcloud_voices};
use structs::{TTSMode, Config, Data, Result, PostgresConfig, JoinVCToken, PollyVoice, FrameworkContext, Framework, WebhookConfigRaw, WebhookConfig};


use crate::structs::FailurePoint;

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

    let (logs, errors, servers, dm_logs, suggestions) = tokio::try_join!(
        get_webhook(webhooks_raw.logs),
        get_webhook(webhooks_raw.errors),
        get_webhook(webhooks_raw.servers),
        get_webhook(webhooks_raw.dm_logs),
        get_webhook(webhooks_raw.suggestions),
    )?;

    Ok(WebhookConfig{logs, servers, dm_logs, suggestions, errors: Some(errors)})
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(_main())
}

#[allow(clippy::too_many_lines)]
async fn _main() -> Result<()> {
    let start_time = std::time::SystemTime::now();
    std::env::set_var("RUST_LIB_BACKTRACE", "1");

    let (pool, mut config) = {
        let mut config_toml: toml::Value = std::fs::read_to_string("config.toml")?.parse()?;
        let postgres: PostgresConfig = toml::Value::try_into(config_toml["PostgreSQL-Info"].clone())?;

        let pool_config = sqlx::postgres::PgPoolOptions::new();
        let pool_config = if let Some(max_connections) = postgres.max_connections {
            pool_config.max_connections(max_connections)
        } else {
            pool_config
        };

        let pool = pool_config.connect_with(
            sqlx::postgres::PgConnectOptions::new()
            .host(&postgres.host)
            .username(&postgres.user)
            .database(&postgres.database)
            .password(&postgres.password)
        ).await?;

        migration::run(&mut config_toml, &pool).await?;

        let config: Config = config_toml.try_into()?;
        (pool, config)
    };

    let filter_entry = |to_check| move |entry: &std::fs::DirEntry| entry
        .metadata()
        .map(|m| match to_check {
            EntryCheck::IsFile => m.is_file(),
            EntryCheck::IsDir => m.is_dir(),
        }).unwrap_or(false);

    let translations =
        std::fs::read_dir("translations")?
            .map(Result::unwrap)
            .filter(filter_entry(EntryCheck::IsDir))
            .flat_map(|d| std::fs::read_dir(d.path()).unwrap()
                .map(Result::unwrap)
                .filter(filter_entry(EntryCheck::IsFile))
                .filter(|e| e.path().extension().map_or(false, |e| e == "mo"))
                .map(|entry| Ok((
                    entry.file_name().to_str().unwrap().split('.').next().unwrap().to_string(),
                    gettext::Catalog::parse(std::fs::File::open(entry.path())?)?
                )))
                .filter_map(Result::ok)
            )
            .collect();

    let reqwest = reqwest::Client::new();
    let auth_key = config.main.tts_service_auth_key.as_deref();
    let http = serenity::Http::new(config.main.token.as_deref().unwrap());

    let (
        guilds_db, userinfo_db, user_voice_db, guild_voice_db, nickname_db,
        mut webhooks, premium_avatar_url,
        gtts_voices, espeak_voices, gcloud_voices, polly_voices
    ) = tokio::try_join!(
        create_db_handler!(pool.clone(), "guilds", "guild_id"),
        create_db_handler!(pool.clone(), "userinfo", "user_id"),
        create_db_handler!(pool.clone(), "user_voice", "user_id", "mode"),
        create_db_handler!(pool.clone(), "guild_voice", "guild_id", "mode"),
        create_db_handler!(pool.clone(), "nicknames", "guild_id", "user_id"),
        get_webhooks(&http, config.webhooks),
        async {serenity::UserId::new(802632257658683442).to_user(&http).await.map(|u| u.face()).map_err(Into::into)},
        async {Ok(TTSMode::gTTS.fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key).await?.json::<BTreeMap<String, String>>().await?)},
        async {Ok(TTSMode::eSpeak.fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key).await?.json::<Vec<String>>().await?)},
        async {Ok(prepare_gcloud_voices(json::from_slice(&TTSMode::gCloud
            .fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key).await?.bytes().await?
        )?))},
        async {Ok(TTSMode::Polly
            .fetch_voices(config.main.tts_service.clone(), &reqwest, auth_key).await?.json::<Vec<PollyVoice>>().await?
            .into_iter().map(|v| (v.id.clone(), v)).collect::<BTreeMap<String, PollyVoice>>())
        },
    )?;

    let analytics = Arc::new(analytics::Handler::new(pool.clone()));
    tokio::spawn(analytics.clone().start());

    let startup_message = webhooks.logs.execute(&http, true, |b| b
        .content("**TTS Bot is starting up**")
    ).await?.unwrap().id;

    let logger = logging::WebhookLogger::new(
        Arc::new(http),
        "discord_tts_bot",
        "TTS-Webhook",
        tracing::Level::from_str(&config.main.log_level)?,
        webhooks.logs.clone(),
        webhooks.errors.as_ref().unwrap().clone(),
    );

    tracing::subscriber::set_global_default(logger.clone())?;
    tokio::spawn(logger.0.start());

    let token = config.main.token.take().unwrap();
    let bot_id = serenity::utils::parse_token(&token).unwrap().0;

    let data = Data {
        bot_list_tokens: config.bot_list_tokens,
        inner: gnomeutils::GnomeData {
            pool, translations,
            error_webhook: webhooks.errors.take().unwrap(),
            main_server_invite: config.main.main_server_invite.clone(),
            system_info: parking_lot::Mutex::new(sysinfo::System::new()),
        },

        join_vc_tokens: dashmap::DashMap::new(),
        last_to_xsaid_tracker: dashmap::DashMap::new(),
        currently_purging: AtomicBool::new(false),

        gtts_voices, espeak_voices, gcloud_voices, polly_voices,

        config: config.main, reqwest, premium_avatar_url,
        analytics, webhooks, start_time, startup_message,
        guilds_db, userinfo_db, nickname_db, user_voice_db, guild_voice_db,
    };

    let framework_oc = Arc::new(once_cell::sync::OnceCell::new());
    let framework_oc_clone = framework_oc.clone();

    let framework = poise::Framework::build()
        .token(token)
        .user_data_setup(|_, _, _| {Box::pin(async {Ok(data)})})
        .intents(
            serenity::GatewayIntents::GUILDS
            | serenity::GatewayIntents::GUILD_MESSAGES
            | serenity::GatewayIntents::DIRECT_MESSAGES
            | serenity::GatewayIntents::GUILD_VOICE_STATES
            | serenity::GatewayIntents::GUILD_MEMBERS
            | serenity::GatewayIntents::MESSAGE_CONTENT,
        )
        .client_settings(move |f| f
            .event_handler(EventHandler {bot_id, framework: framework_oc_clone, fully_started: AtomicBool::new(false)})
            .register_songbird_from_config(songbird::Config::default().decode_mode(songbird::driver::DecodeMode::Pass))
        )
        .options(poise::FrameworkOptions {
            allowed_mentions: Some({
                let mut allowed_mentions = serenity::CreateAllowedMentions::default();
                allowed_mentions.parse(serenity::ParseValue::Users);
                allowed_mentions.replied_user(true);
                allowed_mentions
            }),
            pre_command: |ctx| Box::pin(async move {
                let analytics_handler: &analytics::Handler = &ctx.data().analytics;

                analytics_handler.log(Cow::Owned(ctx.command().qualified_name.clone()), true);
                analytics_handler.log(Cow::Borrowed(match ctx {
                    poise::Context::Prefix(_) => "command",
                    poise::Context::Application(ctx) => match ctx.interaction {
                        poise::ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(_) => "slash_command",
                        poise::ApplicationCommandOrAutocompleteInteraction::Autocomplete(_) => "autocomplete",
                    },
                }), false);
            }),
            on_error: |error| Box::pin(async move {gnomeutils::errors::handle(error).await.unwrap_or_else(|err| error!("on_error: {:?}", err))}),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: None,
                dynamic_prefix: Some(|ctx| {Box::pin(async move {Ok(Some(
                    match ctx.guild_id.map(Into::into) {
                        Some(guild_id) => ctx.data.guilds_db.get(guild_id).await?.prefix.clone(),
                        None => String::from("-"),
                    }
                ))})}),
                ..poise::PrefixFrameworkOptions::default()
            },
            command_check: Some(|ctx| Box::pin(async move {
                if ctx.author().bot {
                    Ok(false)
                } else if let Some(guild_id) = ctx.guild_id() && let Some(required_role) = ctx.data().guilds_db.get(guild_id.into()).await?.required_role {
                    let required_role = serenity::RoleId::new(required_role as u64);
                    let member = ctx.author_member().await.try_unwrap()?;

                    let is_admin = || {
                        let guild = require_guild!(ctx, Ok(false));
                        let channel = guild.channels.get(&ctx.channel_id()).and_then(|c| match c {
                            serenity::Channel::Guild(c) => Some(c),
                            _ => None,
                        }).try_unwrap()?;

                        let permissions = guild.user_permissions_in(channel, &member)?;
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
            })),
            // Add all the commands, this ordering is important as it is shown on the help command
            commands: vec![
                commands::main::join(), commands::main::clear(), commands::main::leave(), commands::main::premium_activate(),

                commands::other::tts(), commands::other::uptime(), commands::other::botstats(), commands::other::channel(),
                commands::other::premium(), commands::other::ping(), commands::other::suggest(), commands::other::invite(),
                commands::other::tts_speak(), commands::other::tts_speak_as(),

                commands::settings::settings(),
                poise::Command {
                    subcommands: vec![
                        poise::Command {
                            name: "channel",
                            ..commands::settings::setup()
                        },
                        commands::settings::xsaid(), commands::settings::autojoin(), commands::settings::required_role(),
                        commands::settings::voice(), commands::settings::server_voice(), commands::settings::mode(),
                        commands::settings::server_mode(), commands::settings::msg_length(),  commands::settings::botignore(), commands::settings::prefix(),
                        commands::settings::translation(), commands::settings::translation_lang(), commands::settings::speaking_rate(),
                        commands::settings::nick(), commands::settings::repeated_characters(), commands::settings::audienceignore(),
                        commands::settings::require_voice(), commands::settings::block(),
                    ],
                    ..commands::settings::set()
                },
                commands::settings::setup(), commands::settings::voices(),

                commands::help::help(),
                commands::owner::dm(), commands::owner::close(), commands::owner::debug(), commands::owner::register(),
                commands::owner::add_premium(), commands::owner::remove_cache(), commands::owner::refresh_ofs(),
                commands::owner::purge_guilds(),
            ],..poise::FrameworkOptions::default()
        })
        .build().await?;

    if framework_oc.set(framework.clone()).is_err() {
        unreachable!()
    };

    let framework_copy = framework.clone();
    tokio::spawn(async move {
        #[cfg(unix)] {
            use tokio::signal::unix as signal;

            let [mut s1, mut s2, mut s3] = [
                signal::signal(signal::SignalKind::hangup()).unwrap(),
                signal::signal(signal::SignalKind::interrupt()).unwrap(),
                signal::signal(signal::SignalKind::terminate()).unwrap()
            ];

            tokio::select!(
                v = s1.recv() => v.unwrap(),
                v = s2.recv() => v.unwrap(),
                v = s3.recv() => v.unwrap(),
            );
        }
        #[cfg(windows)] {
            let (mut s1, mut s2) = (
                tokio::signal::windows::ctrl_c().unwrap(),
                tokio::signal::windows::ctrl_break().unwrap()
            );

            tokio::select!(
                v = s1.recv() => v.unwrap(),
                v = s2.recv() => v.unwrap(),
            );
        }

        warn!("Recieved control C and shutting down.");
        framework_copy.shard_manager().lock().await.shutdown_all().await;
    });

    framework.start_autosharded().await.map_err(Into::into)
}


struct EventHandler {
    bot_id: serenity::UserId,
    fully_started: AtomicBool,
    framework: Arc<OnceCell<Arc<Framework>>>
}

impl EventHandler {
    async fn framework(&self) -> Option<FrameworkContext<'_>> {
        match self.framework.get() {
            None => None,
            Some(framework) => Some(poise::FrameworkContext {
                bot_id: self.bot_id,
                options: framework.options(),
                user_data: framework.user_data().await,
                shard_manager: framework.shard_manager()
            }),
        }
    }
}

#[poise::async_trait]
impl serenity::EventHandler for EventHandler {
    async fn message(&self, ctx: serenity::Context, new_message: serenity::Message) {
        let framework = require!(self.framework().await);
        errors::handle_message(&ctx, framework, &new_message, tokio::try_join!(
            process_tts_msg(&ctx, &new_message, framework.user_data),
            process_support_dm(&ctx, &new_message, framework.user_data),
            process_mention_msg(&ctx, &new_message, framework.user_data),
        )).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn voice_state_update(&self, ctx: serenity::Context, old: Option<serenity::VoiceState>, new: serenity::VoiceState) {
        let framework = require!(self.framework().await);
        errors::handle_unexpected_default(&ctx, framework, "VoiceStateUpdate", async_try!({
            // If (on leave) the bot should also leave as it is alone
            let bot_id = ctx.cache.current_user_id();
            let guild_id = new.guild_id.try_unwrap()?;
            let songbird = songbird::get(&ctx).await.unwrap();

            if songbird.get(guild_id).is_some()
                && let Some(old) = old && new.channel_id.is_none() // user left vc
                && !new.member.map_or(false, |m| m.user.id == bot_id) // user other than bot leaving
                && !ctx.cache // filter out bots from members
                    .guild_channel(old.channel_id.try_unwrap()?)
                    .try_unwrap()?
                    .members(&ctx.cache)?
                    .into_iter().any(|m| !m.user.bot)
            {
                songbird.remove(guild_id).await?;
            };

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn guild_create(&self, ctx: serenity::Context, guild: serenity::Guild, is_new: bool) {
        if !is_new {return};

        let framework = require!(self.framework().await);
        let data = framework.user_data;

        errors::handle_guild("GuildCreate", &ctx, framework, Some(&guild), async_try!({
            // Send to servers channel and DM owner the welcome message

            let (owner, _) = tokio::join!(
                guild.owner_id.to_user(&ctx),
                data.webhooks.servers.execute(&ctx.http, false, |b| {
                    b.content(format!("Just joined {}!", &guild.name))
                }),
            );

            let owner = owner?;
            match owner.direct_message(&ctx, |b| b.embed(|e| e
                .title(ctx.cache.current_user_field(|b| format!("Welcome to {}!", b.name)))
                .description(format!("
Hello! Someone invited me to your server `{}`!
TTS Bot is a text to speech bot, as in, it reads messages from a text channel and speaks it into a voice channel

**Most commands need to be done on your server, such as `-setup` and `-join`**

I need someone with the administrator permission to do `-setup #channel`
You can then do `-join` in that channel and I will join your voice channel!
Then, you can just type normal messages and I will say them, like magic!

You can view all the commands with `-help`
Ask questions by either responding here or asking on the support server!",
                guild.name))
                .footer(|f| f.text(format!("Support Server: {} | Bot Invite: https://bit.ly/TTSBotSlash", data.config.main_server_invite)))
                .author(|a| a.name(format!("{}#{}", &owner.name, &owner.id)).icon_url(owner.face()))
            )).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {},
                Err(error) => return Err(anyhow::Error::from(error)),
                _ => {}
            }

            match ctx.http.add_member_role(
                data.config.main_server.into(),
                owner.id.get(),
                data.config.ofs_role.into(),
                None
            ).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::NOT_FOUND) => return Ok(()),
                Err(err) => return Err(anyhow::Error::from(err)),
                Result::Ok(_) => (),
            }

            info!("Added OFS role to {}#{}", owner.name, owner.discriminator);

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn guild_delete(&self, ctx: serenity::Context, incomplete: serenity::UnavailableGuild, full: Option<serenity::Guild>) {
        let framework = require!(self.framework().await);
        let data = framework.user_data;

        errors::handle_guild("GuildDelete", &ctx, framework, full.as_ref(), async_try!({
            data.guilds_db.delete(incomplete.id.into()).await?;
            if let Some(guild) = &full {
                if data.currently_purging.load(Ordering::SeqCst) {
                    return Ok(());
                }

                if data.config.main_server.members(&ctx.http, None, None).await?.into_iter()
                    .filter(|m| m.roles.contains(&data.config.ofs_role))
                    .any(|m| m.user.id == guild.owner_id)
                {
                    ctx.http.remove_member_role(
                        data.config.main_server.get(),
                        guild.owner_id.get(),
                        data.config.ofs_role.get(),
                        None
                    ).await?;
                }

                data.webhooks.servers.execute(&ctx.http, false, |b| b.content(format!(
                    "Just got kicked from {}. I'm now in {} servers",
                    guild.name, ctx.cache.guilds().len()
                ))).await?;
            };

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn interaction_create(&self, ctx: serenity::Context, interaction: serenity::Interaction) {
        let framework = require!(self.framework().await);
        gnomeutils::errors::interaction_create(ctx, interaction, framework).await;
    }

    async fn ready(&self, ctx: serenity::Context, data_about_bot: serenity::Ready) {
        let framework = require!(self.framework().await);
        let data = framework.user_data;

        let last_shard = (ctx.shard_id + 1) == ctx.cache.shard_count();
        errors::handle_unexpected_default(&ctx, framework, "Ready", async_try!({
            let user_name = &data_about_bot.user.name;
            let status = generate_status(&*framework.shard_manager.lock().await.runners.lock().await);

            data.webhooks.logs.edit_message(&ctx.http, data.startup_message, |m| {m
                .content("")
                .embeds(vec![serenity::Embed::fake(|e| {e
                    .description(status)
                    .colour(FREE_NEUTRAL_COLOUR)
                    .title(if last_shard {
                        format!("{user_name} started in {} seconds", data.start_time.elapsed().unwrap().as_secs())
                    } else {
                        format!("{user_name} is starting up!")
                    })
                })])
            }).await?;

            if last_shard && !self.fully_started.load(Ordering::SeqCst) {
                self.fully_started.store(true, Ordering::SeqCst);
                let stats_updater = Arc::new(gnomeutils::BotListUpdater::new(
                    data.reqwest.clone(), ctx.cache.clone(), data.bot_list_tokens.clone()
                ));

                tokio::spawn(stats_updater.start());
            }

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn guild_member_addition(&self, ctx: serenity::Context, member: serenity::Member) {
        let framework = require!(self.framework().await);
        let data = framework.user_data;

        if
            member.guild_id != data.config.main_server &&
            ctx.cache.guilds().into_iter().find_map(|id| ctx.cache.guild(id).map(|g| g.owner_id == member.user.id)).unwrap_or(false)
        {
            errors::handle_member(&ctx, framework, &member,
                match ctx.http.add_member_role(
                    data.config.main_server.get(),
                    member.user.id.get(),
                    data.config.ofs_role.get(),
                    None
                ).await {
                    // Unknown member
                    Err(serenity::Error::Http(serenity::HttpError::UnsuccessfulRequest(err))) if err.error.code == 10007 => return,
                    r => r
                }
            ).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
        };
    }

    async fn resume(&self, _: serenity::Context, _: serenity::ResumedEvent) {
        if let Some(framework) = self.framework().await {
            framework.user_data.analytics.log(Cow::Borrowed("resumed"), false);
        }
    }
}


async fn premium_command_check(ctx: structs::Context<'_>) -> Result<bool> {
    let guild_id = ctx.guild_id();
    let ctx_discord = ctx.discord();
    let data = ctx.data();

    let main_msg =
        match data.premium_check(guild_id).await? {
            None => return Ok(true),
            Some(FailurePoint::Guild) => Cow::Borrowed("Hey, this is a premium command so it must be run in a server!"),
            Some(FailurePoint::PremiumUser) => Cow::Owned(
                format!("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`{}donate`)", ctx.prefix())
            ),
            Some(FailurePoint::NotSubscribed(premium_user_id)) => {
                let premium_user = premium_user_id.to_user(ctx_discord).await?;
                Cow::Owned(format!(concat!(
                    "Hey, this server has a premium user setup, however they are not longer a patreon! ",
                    "Please ask {}#{} to renew their membership."
                ), premium_user.name, premium_user.discriminator))
            }
        };

    let author = ctx.author();
    warn!(
        "{}#{} | {} failed the premium check in {}",
        author.name, author.discriminator, author.id,
        guild_id.and_then(|g_id| ctx_discord.cache.guild(g_id).map(|g| (
            Cow::Owned(format!("{} | {}", g.name, g_id))
        ))).unwrap_or(Cow::Borrowed("DMs"))
    );

    let permissions = ctx.author_permissions().await?;
    if permissions.send_messages() {
        ctx.send(|b| {
            const FOOTER_MSG: &str = "If this is an error, please contact Gnome!#6669.";
            if permissions.embed_links() {
                b.embed(|e| {e
                    .title("TTS Bot Premium - Premium Only Command!")
                    .description(main_msg)
                    .colour(PREMIUM_NEUTRAL_COLOUR)
                    .thumbnail(&data.premium_avatar_url)
                    .footer(|f| f.text(FOOTER_MSG))
                })
            } else {
                b.content(format!("{}\n{}", main_msg, FOOTER_MSG))
            }
        }).await?;
    }

    Ok(false)
}

async fn process_tts_msg(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<()> {
    let guild_id = require!(message.guild_id, Ok(()));

    let guilds_db = &data.guilds_db;
    let nicknames = &data.nickname_db;

    let guild_row = guilds_db.get(guild_id.into()).await?;
    let xsaid = guild_row.xsaid;
    let channel = guild_row.channel;
    let prefix = &guild_row.prefix;
    let autojoin = guild_row.auto_join;
    let msg_length = guild_row.msg_length;
    let bot_ignore = guild_row.bot_ignore;
    let require_voice = guild_row.require_voice;
    let repeated_limit = guild_row.repeated_chars;
    let audience_ignore = guild_row.audience_ignore;
    let required_role = guild_row.required_role;

    let mode;
    let voice;

    let mut content = match run_checks(
        ctx, message,
        channel as u64, prefix, autojoin, bot_ignore, require_voice, audience_ignore, required_role,
    ).await? {
        None => return Ok(()),
        Some((content, to_autojoin)) => {
            if let Some(channel_id) = to_autojoin {
                let join_vc_lock = JoinVCToken::acquire(data, guild_id);
                ctx.join_vc(join_vc_lock.lock().await, channel_id).await?;
            }

            (voice, mode) = data.parse_user_or_guild(message.author.id, Some(guild_id)).await?;

            let nickname_row = nicknames.get([guild_id.into(), message.author.id.into()]).await?;

            let m;
            let member_nick = match &message.member {
                Some(member) => member.nick.as_deref(),
                None => {
                    m = guild_id.member(ctx, message.author.id).await?;
                    m.nick.as_deref()
                },
            };

            clean_msg(
                &content, &message.author, &ctx.cache, guild_id, member_nick, &message.attachments, &voice,
                xsaid, repeated_limit as usize, nickname_row.name.as_deref(),
                &data.last_to_xsaid_tracker
            )
        }
    };

    let speaking_rate = data.user_voice_db
        .get((message.author.id.into(), mode)).await?
        .speaking_rate
        .map_or_else(
            || mode.speaking_rate_info().map(|(_, d, _, _)| d.to_string()).map_or(Cow::Borrowed("1.0"), Cow::Owned),
            |r| Cow::Owned(r.to_string())
        );

    if let Some(target_lang) = guild_row.target_lang.as_deref() {
        if guild_row.to_translate && data.premium_check(Some(guild_id)).await?.is_none() {
            content = funcs::translate(&content, target_lang, data).await?.unwrap_or(content);
        };
    }

    let url = funcs::prepare_url(
        data.config.tts_service.clone(),
        &content, &voice, mode,
        &speaking_rate, &msg_length.to_string()
    );

    // Pre-fetch the audio to handle max_length errors
    let audio = require!(funcs::fetch_audio(
        &data.reqwest,
        url.clone(),
        data.config.tts_service_auth_key.as_deref()
    ).await?, Ok(()));

    {
        let call_lock = match songbird::get(ctx).await.unwrap().get(guild_id) {
            Some(call) => call,
            None => {
                // At this point, the bot is "in" the voice channel, but without a voice client,
                // this is usually if the bot restarted but the bot is still in the vc from the last boot.
                let voice_channel_id = {
                    let guild = ctx.cache.guild(guild_id).try_unwrap()?;
                    guild.voice_states.get(&message.author.id).and_then(|vs| vs.channel_id).try_unwrap()?
                };

                let join_vc_lock = JoinVCToken::acquire(data, guild_id);
                ctx.join_vc(join_vc_lock.lock().await, voice_channel_id).await?
            }
        };

        let hint = audio.headers().get(reqwest::header::CONTENT_TYPE).map(|ct| {
            let mut hint = songbird::input::core::probe::Hint::new();
            hint.mime_type(ct.to_str()?);
            Ok(hint)
        }).transpose()?;

        let input = Box::new(std::io::Cursor::new(audio.bytes().await?));
        let wrapped_audio = songbird::input::LiveInput::Raw(songbird::input::AudioStream{input, hint});

        let mut call = call_lock.lock().await;
        call.enqueue_input(songbird::input::Input::Live(wrapped_audio, None)).await;
    }

    data.analytics.log(Cow::Borrowed(match mode {
        TTSMode::gTTS => "gTTS_tts",
        TTSMode::eSpeak => "eSpeak_tts",
        TTSMode::gCloud => "gCloud_tts",
        TTSMode::Polly => "Polly_tts",
    }), false);

    Ok(())
}

async fn process_mention_msg(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<()> {
    let bot_user = ctx.cache.current_user_id();
    if ![format!("<@{}>", bot_user), format!("<@!{}>", bot_user)].contains(&message.content) {
        return Ok(());
    };

    let guild_id = require!(message.guild_id, Ok(()));
    let channel = message.channel(ctx).await?.guild().unwrap();
    let permissions = channel.permissions_for_user(ctx, bot_user)?;

    let mut prefix = data.guilds_db.get(guild_id.into()).await?.prefix.clone();
    prefix = prefix.replace('`', "").replace('\\', "");

    if permissions.send_messages() {
        channel.say(ctx, format!("Current prefix for this server is: {}", prefix)).await?;
    } else {
        let msg = {
            let guild = ctx.cache.guild(guild_id);
            let guild_name = guild.as_ref().map_or("Unknown Server", |g| g.name.as_str());

            format!("My prefix for `{guild_name}` is {prefix} however I do not have permission to send messages so I cannot respond to your commands!")
        };

        match message.author.direct_message(ctx, |b| b.content(msg)).await {
            Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {}
            Err(error) => return Err(anyhow::Error::from(error)),
            _ => {}
        }
    }

    Ok(())
}

async fn process_support_dm(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<()> {
    match message.channel(ctx).await? {
        serenity::Channel::Guild(channel) => {
            // Check support server trusted member replies to a DM, if so, pass it through
            if let Some(reference) = &message.message_reference {
                if ![data.webhooks.dm_logs.channel_id.try_unwrap()?,
                     data.webhooks.suggestions.channel_id.try_unwrap()?]
                    .contains(&channel.id)
                {
                    return Ok(());
                };

                if let Some(resolved_id) = reference.message_id {
                    let resolved = channel.message(&ctx.http, resolved_id).await?;
                    if resolved.author.discriminator != 0000 {
                        return Ok(());
                    }

                    let todm = require!(ctx.user_from_dm(&resolved.author.name).await, Ok(()));

                    let (content, embed) = commands::owner::dm_generic(
                        ctx,
                        &message.author,
                        &todm,
                        &message.content
                    ).await?;

                    channel.send_message(ctx, |b| {b
                        .content(content)
                        .set_embed(serenity::CreateEmbed::from(embed))
                    }).await?;
                }
            }
        }
        serenity::Channel::Private(channel) => {
            if message.author.bot || message.content.starts_with('-') {
                return Ok(());
            }

            data.analytics.log(Cow::Borrowed("dm"), false);

            let userinfo = data.userinfo_db.get(message.author.id.into()).await?;
            if userinfo.dm_welcomed {
                let content = message.content.to_lowercase();

                if content.contains("discord.gg") {
                    channel.say(&ctx.http, format!(
                        "Join {} and look in {} to invite {}!",
                        data.config.main_server_invite, data.config.invite_channel.mention(), ctx.cache.current_user_id()
                    )).await?;
                } else if content.as_str() == "help" {
                    channel.say(&ctx.http, "We cannot help you unless you ask a question, if you want the help command just do `-help`!").await?;
                } else if !userinfo.dm_blocked {
                    let display_name = format!("{}#{:04}", &message.author.name, &message.author.discriminator);
                    let webhook_username = format!("{} ({})", display_name, message.author.id.0);
                    let paths: Vec<serenity::AttachmentType<'_>> = message.attachments.iter()
                        .map(|a| serenity::AttachmentType::Path(Path::new(&a.url)))
                        .collect();

                    data.webhooks.dm_logs.execute(&ctx.http, false, |b| {b
                        .files(paths)
                        .content(&message.content)
                        .username(webhook_username)
                        .avatar_url(message.author.face())
                        .embeds(message.embeds.iter().map(|e| json::to_value(e).unwrap()).collect())
                    }).await?;
                }
            } else {
                let welcome_msg = channel.send_message(&ctx.http, |b| b.embed(|e| e
                    .title(ctx.cache.current_user_field(|b| format!(
                        "Welcome to {} Support DMs!", b.name
                    )))
                    .description(DM_WELCOME_MESSAGE)
                    .footer(|f| f.text(random_footer(
                        &data.config.main_server_invite,
                        ctx.cache.current_user_id(),
                        data.default_catalog(),
                    )))
                )).await?;

                data.userinfo_db.set_one(message.author.id.into(), "dm_welcomed", &true).await?;
                if channel.pins(&ctx.http).await?.len() < 50 {
                    welcome_msg.pin(ctx).await?;
                }

                info!("{}#{} just got the 'Welcome to support DMs' message", message.author.name, message.author.discriminator);                
            }
        }
        _ => {}
    }

    Ok(())
}
