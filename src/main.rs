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

#![feature(let_chains)]

#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]

// clippy::pedantic complains about u64 -> i64 and back when db conversion, however it is fine
#![allow(clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::cast_lossless)]
#![allow(clippy::unreadable_literal)]

use std::{collections::HashMap, borrow::Cow};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Weak};

use sysinfo::SystemExt;
use once_cell::sync::OnceCell;
use tracing::{error, info, warn};

use poise::serenity_prelude as serenity; // re-exports a lot of serenity with shorter paths
use songbird::SerenityInit; // adds serenity::ClientBuilder.register_songbird

mod migration;
mod analytics;
mod constants;
mod database;
mod commands;
mod logging;
mod structs;
mod macros;
mod error;
mod funcs;

use macros::{require, async_try};
use constants::{DM_WELCOME_MESSAGE, FREE_NEUTRAL_COLOUR, VIEW_TRACEBACK_CUSTOM_ID};
use funcs::{clean_msg, parse_user_or_guild, run_checks, random_footer, get_premium_voices, generate_status};
use structs::{TTSMode, Config, Data, Result, PoiseContextExt, SerenityContextExt, PostgresConfig, OptionTryUnwrap, Framework};

use crate::constants::PREMIUM_NEUTRAL_COLOUR;

async fn get_webhooks(
    http: &serenity::Http,
    webhooks_raw: toml::value::Table,
) -> HashMap<String, serenity::Webhook> {
    let mut webhooks = HashMap::with_capacity(webhooks_raw.len());

    for (name, url) in webhooks_raw {
        let url = url.as_str().unwrap().parse().unwrap();
        let (webhook_id, token) = serenity::parse_webhook(&url).unwrap();

        webhooks.insert(name, http.get_webhook_with_token(webhook_id, token).await.unwrap());
    }

    webhooks
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() {
    let start_time = std::time::SystemTime::now();
    std::env::set_var("RUST_LIB_BACKTRACE", "1");

    let (pool, mut main, webhooks) = {
        let mut config_toml: toml::Value = std::fs::read_to_string("config.toml").unwrap().parse().unwrap();
        let postgres: PostgresConfig = toml::Value::try_into(config_toml["PostgreSQL-Info"].clone()).unwrap();

        // Setup database pool
        let pool = sqlx::PgPool::connect_with(
            sqlx::postgres::PgConnectOptions::new()
            .host(&postgres.host)
            .username(&postgres.user)
            .database(&postgres.database)
            .password(&postgres.password)
        ).await.unwrap();

        migration::run(&mut config_toml, &pool).await.unwrap();

        let Config{main, webhooks} = config_toml.try_into().unwrap();
        (pool, main, webhooks)
    };

    // CLEANUP
    let analytics = Arc::new(analytics::Handler::new(pool.clone()));
    {
        let analytics_sender = analytics.clone();
        tokio::spawn(async move {analytics_sender.loop_task().await});
    }

    let guilds_db = database::Handler::new(pool.clone(), 0,
        "SELECT * FROM guilds WHERE guild_id = $1",
        "DELETE FROM guilds WHERE guild_id = $1",
        "
            INSERT INTO guilds(guild_id) VALUES ($1)
            ON CONFLICT (guild_id) DO NOTHING
        ",
        "
            INSERT INTO guilds(guild_id, {key}) VALUES ($1, $2)
            ON CONFLICT (guild_id) DO UPDATE SET {key} = $2
        "
    ).await.unwrap();
    let userinfo_db = database::Handler::new(pool.clone(), 0,
        "SELECT * FROM userinfo WHERE user_id = $1",
        "DELETE FROM userinfo WHERE user_id = $1",
        "
            INSERT INTO userinfo(user_id) VALUES ($1)
            ON CONFLICT (user_id) DO NOTHING
        ",
        "
            INSERT INTO userinfo(user_id, {key}) VALUES ($1, $2)
            ON CONFLICT (user_id) DO UPDATE SET {key} = $2
        "
    ).await.unwrap();
    let nickname_db = database::Handler::new(pool.clone(), [0, 0],
        "SELECT * FROM nicknames WHERE guild_id = $1 AND user_id = $2",
        "DELETE FROM nicknames WHERE guild_id = $1 AND user_id = $2",
        "
            INSERT INTO nicknames(guild_id, user_id) VALUES ($1, $2)
            ON CONFLICT (guild_id, user_id) DO NOTHING
        ",
        "
            INSERT INTO nicknames(guild_id, user_id, {key}) VALUES ($1, $2, $3)
            ON CONFLICT (guild_id, user_id) DO UPDATE SET {key} = $3
        "
    ).await.unwrap();
    let user_voice_db = database::Handler::new(pool.clone(), (0, TTSMode::gTTS),
        "SELECT * FROM user_voice WHERE user_id = $1 AND mode = $2",
        "DELETE FROM user_voice WHERE user_id = $1 AND mode = $2",
        "
            INSERT INTO user_voice(user_id, mode) VALUES ($1, $2)
            ON CONFLICT (user_id, mode) DO NOTHING
        ",
        "
            INSERT INTO user_voice(user_id, mode, {key}) VALUES ($1, $2, $3)
            ON CONFLICT (user_id, mode) DO UPDATE SET {key} = $3
        "
    ).await.unwrap();
    let guild_voice_db = database::Handler::new(pool.clone(), (0, TTSMode::gTTS),
        "SELECT * FROM guild_voice WHERE guild_id = $1 AND mode = $2",
        "DELETE FROM guild_voice WHERE guild_id = $1 AND mode = $2",
        "
            INSERT INTO guild_voice(guild_id, mode) VALUES ($1, $2)
            ON CONFLICT (guild_id, mode) DO NOTHING
        ",
        "
            INSERT INTO guild_voice(guild_id, mode, {key}) VALUES ($1, $2, $3)
            ON CONFLICT (guild_id, mode) DO UPDATE SET {key} = $3
        "
    ).await.unwrap();

    let (startup_message, webhooks) = {
        let http = serenity::Http::new(main.token.as_deref().unwrap());
        let webhooks = get_webhooks(&http, webhooks).await;
        (
            webhooks["logs"].execute(&http, true, |b| b
                .content("**TTS Bot is starting up**")
            ).await.unwrap().unwrap().id, webhooks
        )
    };

    let framework_oc = Arc::new(once_cell::sync::OnceCell::new());
    let framework_oc_clone = framework_oc.clone();

    let framework = poise::Framework::build()
        .token(main.token.take().unwrap())
        .intents(
            serenity::GatewayIntents::GUILDS
            | serenity::GatewayIntents::GUILD_MESSAGES
            | serenity::GatewayIntents::DIRECT_MESSAGES
            | serenity::GatewayIntents::GUILD_VOICE_STATES
            | serenity::GatewayIntents::GUILD_MEMBERS
            | serenity::GatewayIntents::MESSAGE_CONTENT,
        )
        .client_settings(move |f| {f
            .event_handler(EventHandler {framework: framework_oc_clone})
            .register_songbird_from_config(songbird::Config::default().decode_mode(songbird::driver::DecodeMode::Pass))
        })
        .user_data_setup(move |ctx, _, _| {Box::pin(async move {
            let (send, rx) = std::sync::mpsc::channel();
            let subscriber = logging::WebhookLogSend::new(send, tracing::Level::from_str(&main.log_level)?);
            let listener = logging::WebhookLogRecv::new(
                rx,
                ctx.http.clone(),
                webhooks["logs"].clone(),
                webhooks["errors"].clone(),
            );

            tokio::spawn(async move {listener.listener().await;});
            tracing::subscriber::set_global_default(subscriber).unwrap();

            Ok(Data {
                config: main,
                reqwest: reqwest::Client::new(),
                premium_voices: get_premium_voices(),
                last_to_xsaid_tracker: dashmap::DashMap::new(),
                system_info: parking_lot::Mutex::new(sysinfo::System::new()),
                premium_avatar_url: serenity::UserId(802632257658683442).to_user(ctx).await?.face(),

                guilds_db, userinfo_db, nickname_db, user_voice_db, guild_voice_db,
                analytics, webhooks, start_time, pool, startup_message
            })
        })})
        .options(poise::FrameworkOptions {
            allowed_mentions: Some({
                let mut allowed_mentions = serenity::CreateAllowedMentions::default();
                allowed_mentions.parse(serenity::ParseValue::Users);
                allowed_mentions.replied_user(true);
                allowed_mentions
            }),
            pre_command: |ctx| Box::pin(async move {
                let analytics_handler: &analytics::Handler = &ctx.data().analytics;

                analytics_handler.log(Cow::Owned(ctx.command().qualified_name.clone()));
                analytics_handler.log(Cow::Borrowed(match ctx {
                    poise::Context::Prefix(_) => "on_command",
                    poise::Context::Application(_) => "on_slash_command",
                }));
            }),
            on_error: |error| Box::pin(async move {error::handle(error).await.unwrap_or_else(|err| error!("on_error: {:?}", err))}),
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
            // Add all the commands, this ordering is important as it is shown on the help command
            commands: vec![
                commands::main::join(), commands::main::clear(), commands::main::leave(), commands::main::premium_activate(),

                commands::other::tts(), commands::other::uptime(), commands::other::botstats(), commands::other::channel(),
                commands::other::donate(), commands::other::ping(), commands::other::suggest(), commands::other::invite(),

                commands::settings::settings(),
                poise::Command {
                    subcommands: vec![
                        poise::Command {
                            name: "channel",
                            ..commands::settings::setup()
                        },
                        commands::settings::xsaid(), commands::settings::autojoin(), commands::settings::botignore(),
                        commands::settings::voice(), commands::settings::server_voice(), commands::settings::mode(),
                        commands::settings::server_mode(), commands::settings::prefix(),
                        commands::settings::translation(), commands::settings::translation_lang(), commands::settings::speaking_rate(),
                        commands::settings::nick(), commands::settings::repeated_characters(), commands::settings::audienceignore(),
                        commands::settings::require_voice(), commands::settings::block(),
                    ],
                    ..commands::settings::set()
                },
                commands::settings::setup(), commands::settings::voices(),

                commands::help::help(),
                commands::owner::dm(), commands::owner::close(), commands::owner::debug(), commands::owner::register(),
                commands::owner::add_premium(),

                poise::Command {
                    subcommands: vec![
                        commands::owner::guild(), commands::owner::user(),
                        commands::owner::guild_voice(), commands::owner::user_voice(),
                    ], ..commands::owner::remove_cache()
                }
            ],..poise::FrameworkOptions::default()
        })
        .build().await.unwrap();

    if framework_oc.set(Arc::downgrade(&framework)).is_err() {unreachable!()};

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

    framework.start_autosharded().await.unwrap();
}

struct EventHandler {
    framework: Arc<OnceCell<Weak<Framework>>>
}

impl EventHandler {
    fn framework(&self) -> Option<Arc<Framework>> {
        self.framework.get().and_then(Weak::upgrade)
    }
}

#[poise::async_trait]
impl serenity::EventHandler for EventHandler {
    async fn message(&self, ctx: serenity::Context, new_message: serenity::Message) {
        let framework = require!(self.framework());
        error::handle_message(&ctx, &framework, &new_message, async_try!({
            let data = framework.user_data().await;

            let (tts_result, support_result, mention_result) = tokio::join!(
                process_tts_msg(&ctx, &new_message, data),
                process_support_dm(&ctx, &new_message, data),
                process_mention_msg(&ctx, &new_message, data),
            );

            tts_result?; support_result?; mention_result?;
            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn voice_state_update(&self, ctx: serenity::Context, old: Option<serenity::VoiceState>, new: serenity::VoiceState) {
        let framework = require!(self.framework());
        error::handle_unexpected_default(&ctx, &framework, "VoiceStateUpdate", async_try!({
            // If (on leave) the bot should also leave as it is alone
            let bot_id = ctx.cache.current_user_id();
            let guild_id = new.guild_id.try_unwrap()?;
            let songbird = songbird::get(&ctx).await.unwrap();

            let bot_voice_client = songbird.get(guild_id);
            if bot_voice_client.is_some()
                && old.is_some() && new.channel_id.is_none() // user left vc
                && !new.member // user other than bot leaving
                    .as_ref()
                    .map_or(false, |m| m.user.id == bot_id)
                && !ctx.cache // filter out bots from members
                    .guild_channel(old.as_ref().unwrap().channel_id.unwrap())
                    .try_unwrap()?
                    .members(&ctx.cache).await?
                    .iter().any(|m| !m.user.bot)
            {
                songbird.remove(guild_id).await?;
            };

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn guild_create(&self, ctx: serenity::Context, guild: serenity::Guild, is_new: bool) {
        let framework = require!(self.framework());
        let data = framework.user_data().await;
        if !is_new {return};

        error::handle_guild("GuildCreate", &ctx, &framework, Some(&guild), async_try!({
            // Send to servers channel and DM owner the welcome message

            let (owner, _) = tokio::join!(
                guild.owner_id.to_user(&ctx),
                data.webhooks["servers"].execute(&ctx.http, false, |b| {
                    b.content(format!("Just joined {}!", &guild.name))
                }),
            );

            let owner = owner?;
            match owner.direct_message(&ctx, |b| {b.embed(|e| {e
                .title(format!("Welcome to {}!", ctx.cache.current_user_field(|b| b.name.clone())))
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
                .footer(|f| {f.text(format!("Support Server: {} | Bot Invite: https://bit.ly/TTSBotSlash", data.config.main_server_invite))})
                .author(|a| {a.name(format!("{}#{}", &owner.name, &owner.id)); a.icon_url(owner.face())})
            })}).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {},
                Err(error) => return Err(anyhow::Error::from(error)),
                _ => {}
            }

            match ctx.http.add_member_role(
                data.config.main_server.into(),
                owner.id.0,
                data.config.ofs_role,
                None
            ).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::NOT_FOUND) => {return Ok(())},
                Err(err) => return Err(anyhow::Error::from(err)),
                Ok(_) => (),
            }

            info!("Added OFS role to {}#{}", owner.name, owner.discriminator);

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn guild_delete(&self, ctx: serenity::Context, incomplete: serenity::UnavailableGuild, full: Option<serenity::Guild>) {
        let framework = require!(self.framework());
        let data = framework.user_data().await;

        error::handle_guild("GuildDelete", &ctx, &framework, full.as_ref(), async_try!({
            data.guilds_db.delete(incomplete.id.into()).await?;
            if let Some(guild) = &full {
                data.webhooks["servers"].execute(&ctx.http, false, |b| {b.content(format!(
                    "Just got kicked from {}. I'm now in {} servers",
                    guild.name, ctx.cache.guilds().len()
                ))}).await?;
            };

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn interaction_create(&self, ctx: serenity::Context, interaction: serenity::Interaction) {
        let framework = require!(self.framework());
        let data = framework.user_data().await;

        error::handle_unexpected_default(&ctx, &framework, "InteractionCreate", async_try!({
            if let serenity::Interaction::MessageComponent(interaction) = interaction {
                if interaction.data.custom_id == VIEW_TRACEBACK_CUSTOM_ID {
                    error::handle_traceback_button(&ctx, data, interaction).await?;
                }
            };

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn ready(&self, ctx: serenity::Context, data_about_bot: serenity::Ready) {
        let framework = require!(self.framework());
        let data = framework.user_data().await;

        error::handle_unexpected_default(&ctx, &framework, "Ready", async_try!({
            let user_name = &data_about_bot.user.name;
            let (status, starting) = generate_status(&framework).await;
            data.webhooks["logs"].edit_message(&ctx.http, data.startup_message, |m| {m
                .content("")
                .embeds(vec![serenity::Embed::fake(|e| {e
                    .description(status)
                    .colour(FREE_NEUTRAL_COLOUR)
                    .title(
                        if starting {format!("{user_name} is starting up!")}
                        else {format!("{user_name} started in {} seconds", data.start_time.elapsed().unwrap().as_secs())
                    })
                })])
            }).await?;

            Ok(())
        })).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
    }

    async fn resume(&self, _: serenity::Context, _: serenity::ResumedEvent) {
        if let Some(framework) = self.framework() {
            framework.user_data().await.analytics.log(Cow::Borrowed("on_resumed"));
        }
    }
}


enum FailurePoint {
    InSupportGuild(serenity::UserId),
    PatreonRole(serenity::UserId),
    PremiumUser,
    Guild,
}

async fn premium_check(ctx: &serenity::Context, data: &Data, guild_id: Option<serenity::GuildId>) -> Result<Option<FailurePoint>> {
    let guild_id = match guild_id {
        Some(guild) => guild,
        None => return Ok(Some(FailurePoint::Guild))
    };

    let premium_user_id = data
        .guilds_db.get(guild_id.0 as i64).await?
        .premium_user
        .map(|u| serenity::UserId(u as u64));

    match premium_user_id {
        Some(premium_user_id) => {
            match data.config.main_server.member(ctx, premium_user_id).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::NOT_FOUND) => Ok(Some(FailurePoint::InSupportGuild(premium_user_id))),
                Ok(premium_user) => Ok((!premium_user.roles.contains(&data.config.patreon_role)).then(|| FailurePoint::PatreonRole(premium_user_id))),
                Err(err) => Err(anyhow::Error::from(err)),
            }
        }
        None => Ok(Some(FailurePoint::PremiumUser))
    }
}

async fn premium_command_check(ctx: structs::Context<'_>) -> Result<bool, error::CommandError> {
    let ctx_discord = ctx.discord();
    let guild = ctx.guild();
    let data = ctx.data();

    let main_msg =
        match premium_check(ctx_discord, data, guild.as_ref().map(|g| g.id)).await? {
            None => return Ok(true),
            Some(FailurePoint::Guild) => Cow::Borrowed("Hey, this is a premium command so it must be run in a server!"),
            Some(FailurePoint::PremiumUser) => {Cow::Owned(
                format!("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`{}donate`)", ctx.prefix())
            )}
            Some(FailurePoint::InSupportGuild(premium_user_id)) => {
                let premium_user = premium_user_id.to_user(ctx_discord).await?;
                Cow::Owned(format!(concat!(
                    "Hey, this server has a premium user setup, however they are not longer in the support server! ",
                    "Please ask {}#{} to rejoin with {}invite",
                ), premium_user.name, premium_user.discriminator, ctx.prefix()))
            },
            Some(FailurePoint::PatreonRole(premium_user_id)) => {
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
        match guild {
            Some(guild) => Cow::Owned(format!("{} | {}", guild.name, guild.id)),
            None => Cow::Borrowed("DMs")
        }
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
                    .thumbnail(data.premium_avatar_url.clone())
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
    let channel: i64 = guild_row.channel;
    let prefix = &guild_row.prefix;
    let autojoin = guild_row.auto_join;
    let bot_ignore = guild_row.bot_ignore;
    let require_voice = guild_row.require_voice;
    let repeated_limit: i16 = guild_row.repeated_chars;
    let audience_ignore = guild_row.audience_ignore;

    let mode;
    let voice;

    let mut content = match run_checks(
        ctx, message, channel as u64, prefix, autojoin, bot_ignore, require_voice, audience_ignore,
    ).await? {
        None => return Ok(()),
        Some((guild, content)) => {
            let member = guild.member(ctx, message.author.id.0).await?;
            (voice, mode) = parse_user_or_guild(data, message.author.id, Some(guild.id)).await?;

            let nickname_row = nicknames.get([guild.id.into(), message.author.id.into()]).await?;

            clean_msg(
                &content, &guild, &member, &message.attachments, &voice,
                xsaid, repeated_limit as usize, nickname_row.name.as_deref(),
                &data.last_to_xsaid_tracker
            )
        }
    };

    let speaking_rate = data.user_voice_db
        .get((message.author.id.into(), mode)).await?
        .speaking_rate
        .map_or_else(
            || mode.speaking_rate_info().map(|(_, d, _, _)| d.to_string()).unwrap_or_default(),
            |r| r.to_string()
        );

    if let Some(target_lang) = guild_row.target_lang.as_deref() {
        if guild_row.to_translate && premium_check(ctx, data, Some(guild_id)).await?.is_none() {
            content = funcs::translate(&content, target_lang, data).await?.unwrap_or(content);
        };
    }

    let mut tracks = Vec::new();
    for url in funcs::fetch_url(&data.config.tts_service, content, &voice, mode, &speaking_rate) {
        tracks.push(songbird::ffmpeg(url.as_str()).await?);
    }

    let call_lock = match songbird::get(ctx).await.unwrap().get(guild_id) {
        Some(call) => call,
        None => {
            // At this point, the bot is "in" the voice channel, but without a voice client,
            // this is usually if the bot restarted but the bot is still in the vc from the last boot.
            ctx.join_vc(guild_id, message.channel_id).await?
        }
    };

    {
        let mut call = call_lock.lock().await;
        for track in tracks {
            call.enqueue_source(track);
        }
    }

    let mode: &'static str = mode.into();
    data.analytics.log(Cow::Owned(format!("on_{mode}_tts")));
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
        channel.say(ctx,format!("Current prefix for this server is: {}", prefix)).await?;
    } else {
        let guild_name= ctx.cache
            .guild_field(guild_id, |g| g.name.clone())
            .map_or(Cow::Borrowed("Unknown Server"), Cow::Owned);

        let result = message.author.direct_message(ctx, |b| b.content(format!(
            "My prefix for `{guild_name}` is {prefix} however I do not have permission to send messages so I cannot respond to your commands!",
        ))).await;

        match result {
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
                if ![data.webhooks["dm_logs"].channel_id.try_unwrap()?,
                     data.webhooks["suggestions"].channel_id.try_unwrap()?]
                    .contains(&channel.id)
                {
                    return Ok(());
                };

                if let Some(resolved_id) = reference.message_id {
                    let resolved = channel.message(&ctx.http, resolved_id).await?;
                    if resolved.author.discriminator != 0000 {
                        return Ok(());
                    }

                    let todm = match ctx.user_from_dm(&resolved.author.name).await {
                        Some(user) => user,
                        None => return Ok(())
                    };
                    
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

            data.analytics.log(Cow::Borrowed("on_dm"));

            let userinfo = data.userinfo_db.get(message.author.id.into()).await?;
            if userinfo.dm_welcomed {
                let content = message.content.to_lowercase();

                if content.contains("discord.gg") {
                    channel.say(&ctx.http, format!(
                        "Join {} and look in <#{}> to invite {}!",
                        data.config.main_server_invite, data.config.invite_channel, ctx.cache.current_user_id()
                    )).await?;
                } else if content.as_str() == "help" {
                    channel.say(&ctx.http, "We cannot help you unless you ask a question, if you want the help command just do `-help`!").await?;
                } else if !userinfo.dm_blocked {
                    let display_name = format!("{}#{:04}", &message.author.name, &message.author.discriminator);
                    let webhook_username = format!("{} ({})", display_name, message.author.id.0);
                    let paths: Vec<serenity::AttachmentType<'_>> = message.attachments.iter()
                        .map(|a| serenity::AttachmentType::Path(Path::new(&a.url)))
                        .collect();

                    data.webhooks["dm_logs"].execute(&ctx.http, false, |b| {b
                        .files(paths)
                        .content(&message.content)
                        .username(webhook_username)
                        .avatar_url(message.author.face())
                        .embeds(message.embeds.iter().map(|e| serenity::json::prelude::to_value(e).unwrap()).collect())
                    }).await?;
                }
            } else {
                let welcome_msg = channel.send_message(&ctx.http, |b| {b.embed(|e| {e
                    .title(format!(
                        "Welcome to {} Support DMs!",
                        ctx.cache.current_user_field(|b| b.name.clone())
                    ))
                    .description(DM_WELCOME_MESSAGE)
                    .footer(|f| {f.text(random_footer(
                        "-", &data.config.main_server_invite, ctx.cache.current_user_id().0
                    ))}
                )})}).await?;

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
