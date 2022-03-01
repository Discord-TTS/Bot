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

#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]

// clippy::pedantic complains about u64 -> i64 and back when db conversion, however it is fine
#![allow(clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::cast_lossless)]
#![allow(clippy::unreadable_literal)]

use std::{collections::HashMap, borrow::Cow};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use regex::Regex;
use lazy_static::lazy_static;
use deadpool_postgres::tokio_postgres;
use tracing::{debug, error, info, warn};

use poise::serenity_prelude as serenity; // re-exports a lot of serenity with shorter paths
use songbird::SerenityInit; // adds serenity::ClientBuilder.register_songbird
use lavalink_rs::{gateway::LavalinkEventHandler, LavalinkClient};

mod migration;
mod analytics;
mod constants;
mod database;
mod commands;
mod logging;
mod structs;
mod funcs;

use constants::DM_WELCOME_MESSAGE;
use funcs::{clean_msg, parse_user_or_guild, run_checks, random_footer, get_premium_voices};
use structs::{TTSMode, Config, Data, Error, PoiseContextAdditions, SerenityContextAdditions};


struct LavalinkHandler;
impl LavalinkEventHandler for LavalinkHandler {}

fn get_config() -> Result<toml::Value, Error> {
    Ok(std::fs::read_to_string("config.toml")?.parse()?)
}
async fn get_webhooks(
    http: &serenity::Http,
    webhooks_raw: &toml::value::Table,
) -> Option<HashMap<String, serenity::Webhook>> {
    lazy_static! {
        static ref WEBHOOK_URL_REGEX: Regex = Regex::new(r"discord(?:app)?.com/api/webhooks/(\d+)/(.+)").unwrap();
    }

    let mut webhooks = HashMap::new();
    for (name, url) in webhooks_raw {
        let captures = WEBHOOK_URL_REGEX.captures(url.as_str()?)?;

        webhooks.insert(
            name.clone(),
            http.get_webhook_with_token(
                captures.get(1)?.as_str().parse().ok()?,
                captures.get(2)?.as_str(),
            ).await.ok()?,
        );
    }

    Some(webhooks)
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() {
    let start_time = std::time::SystemTime::now();
    let mut config_toml = get_config().unwrap();

    // Setup database pool
    let db_config_toml = &config_toml["PostgreSQL-Info"];
    let mut db_config = deadpool_postgres::Config::new();
    db_config.user = Some(String::from(db_config_toml["user"].as_str().ok_or("").unwrap()));
    db_config.host = Some(String::from(db_config_toml["host"].as_str().ok_or("").unwrap()));
    db_config.dbname = Some(String::from(db_config_toml["database"].as_str().ok_or("").unwrap()));
    db_config.password = Some(String::from(db_config_toml["password"].as_str().ok_or("").unwrap()));

    let pool = Arc::new(db_config.create_pool(
        Some(deadpool_postgres::Runtime::Tokio1),
        tokio_postgres::NoTls,
    ).unwrap());

    migration::run(&mut config_toml, &pool).await.unwrap();

    let config = {
        let main_config = &config_toml["Main"];
        Config {
            translation_token: main_config.get("translation_token").map(|v| v.as_str().unwrap()).map(String::from),
            tts_service: reqwest::Url::parse(main_config["tts_service"].as_str().unwrap()).unwrap(),
            patreon_role: serenity::RoleId(main_config["patreon_role"].as_integer().unwrap() as u64),
            server_invite: String::from(main_config["main_server_invite"].as_str().unwrap()),

            main_server: serenity::GuildId(main_config["main_server"].as_integer().unwrap() as u64),
            invite_channel: main_config["invite_channel"].clone().as_integer().unwrap() as u64,
            ofs_role: main_config["ofs_role"].as_integer().unwrap() as u64,
        }
    };

    let token = config_toml["Main"]["token"].as_str().unwrap();
    let http_client = serenity::http::Http::new_with_token(token);
    let application_info = http_client.get_current_application_info().await.unwrap();

    let lavalink_config = &config_toml["Lavalink"];
    let lavalink = LavalinkClient::builder(application_info.id.0)
        .set_host(&lavalink_config["host"].as_str().unwrap())
        .set_password(&lavalink_config["password"].as_str().unwrap())
        .set_port(u16::try_from(lavalink_config["port"].as_integer().unwrap()).unwrap())
        .set_is_ssl(lavalink_config["ssl"].as_bool().unwrap())
        .build(LavalinkHandler)
        .await.unwrap();

    let webhooks = get_webhooks(
        &http_client,
        config_toml["Webhook-Info"].as_table().unwrap()
    ).await.unwrap();

    let (send, rx) = std::sync::mpsc::channel();
    let listener = logging::WebhookLogRecv::new(
        rx,
        http_client,
        format!("[{}]",
            if cfg!(debug_assertions) {"Debug"}
            else {"Main"}
        ),
        webhooks["logs"].clone(),
        webhooks["errors"].clone(),
    );
    let subscriber = logging::WebhookLogSend::new(
        send,
        tracing::Level::from_str(config_toml["Main"]["log_level"].as_str().unwrap()).unwrap()
    );

    tokio::spawn(async move {listener.listener().await;});
    tracing::subscriber::set_global_default(subscriber).unwrap();

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
    let user_voice_db = database::Handler::new(pool.clone(), (0, TTSMode::Gtts),
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
    let guild_voice_db = database::Handler::new(pool.clone(), (0, TTSMode::Gtts),
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

    let reqwest = reqwest::Client::new();
    let framework = poise::Framework::build()
        .token(token)
        .client_settings(move |f| {
            f.intents(
                serenity::GatewayIntents::GUILDS
                | serenity::GatewayIntents::GUILD_MESSAGES
                | serenity::GatewayIntents::DIRECT_MESSAGES
                | serenity::GatewayIntents::GUILD_VOICE_STATES
                | serenity::GatewayIntents::GUILD_MEMBERS
                | serenity::GatewayIntents::MESSAGE_CONTENT,
            )
            .register_songbird()
        })
        .user_data_setup(move |ctx, _ready, _framework| {Box::pin(async move {Ok(Data {
            premium_voices: get_premium_voices(),
            last_to_xsaid_tracker: dashmap::DashMap::new(),
            premium_avatar_url: serenity::UserId(802632257658683442).to_user(ctx).await?.face(),
            premium_users: config.main_server.members(&ctx.http, None, None).await?.into_iter()
                .filter_map(|m| if m.roles.contains(&config.patreon_role) {Some(m.user.id)} else {None})
                .collect(),

            guilds_db, userinfo_db, nickname_db, user_voice_db, guild_voice_db,
            analytics, lavalink, webhooks, start_time, reqwest, config, pool,
        })})})
        .options(poise::FrameworkOptions {
            allowed_mentions: Some(
                serenity::CreateAllowedMentions::default()
                    .parse(serenity::ParseValue::Users)
                    .replied_user(true)
                    .clone()
            ),
            pre_command: |ctx| Box::pin(async move {
                let analytics_handler: &analytics::Handler = &ctx.data().analytics;

                analytics_handler.log(ctx.command().qualified_name.clone());
                analytics_handler.log(String::from(match ctx {
                    poise::Context::Prefix(_) => "on_command",
                    poise::Context::Application(_) => "on_slash_command",
                }));
            }),
            on_error: |error| Box::pin(on_error(error)),
            listener: |ctx, event, fw, ud| Box::pin(event_listener(ctx, event, fw, ud)),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: None,
                dynamic_prefix: Some(|ctx| {Box::pin(async move {Some(
                    match ctx.guild_id {
                        Some(guild_id) => ctx.data.guilds_db.get(guild_id.into()).await.unwrap().get("prefix"),
                        None => String::from("-"),
                    }
                )})}),
                ..poise::PrefixFrameworkOptions::default()
            },
            // Add all the commands, this ordering is important as it is shown on the help command
            commands: vec![
                commands::main::join(), commands::main::skip(), commands::main::leave(), commands::main::premium_activate(),

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
                        commands::settings::nick(), commands::settings::repeated_characters(), commands::settings::audienceignore()
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

async fn event_listener(
    ctx: &serenity::Context,
    event: &poise::Event<'_>,
    _framework: &poise::Framework<Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        poise::Event::Message { new_message } => {
            let (tts_result, support_result, mention_result) = tokio::join!(
                process_tts_msg(ctx, new_message, data),
                process_support_dm(ctx, new_message, data),
                process_mention_msg(ctx, new_message, data),
            );

            tts_result?; support_result?; mention_result?;
        }
        poise::Event::VoiceStateUpdate { old, new } => {
            // If (on leave) the bot should also leave as it is alone
            let bot_id = ctx.cache.current_user_id();
            let guild_id = new.guild_id.ok_or("no guild_id")?;
            let songbird = songbird::get(ctx).await.unwrap();

            let bot_voice_client = songbird.get(guild_id);
            if bot_voice_client.is_some()
                && old.is_some() && new.channel_id.is_none() // user left vc
                && !new.member // user other than bot leaving
                    .as_ref()
                    .map_or(false, |m| m.user.id == bot_id)
                && !ctx.cache // filter out bots from members
                    .guild_channel(old.as_ref().unwrap().channel_id.unwrap())
                    .ok_or("no channel")?
                    .members(&ctx.cache).await?
                    .iter().any(|m| !m.user.bot)
            {
                songbird.remove(guild_id).await?;
                data.lavalink.destroy(guild_id).await?;
            }
        }
        poise::Event::GuildCreate { guild, is_new } => {
            if !is_new {
                return Ok(());
            }

            // Send to servers channel and DM owner the welcome message
            let (owner, _) = tokio::join!(
                guild.owner_id.to_user(ctx),
                data.webhooks["servers"].execute(&ctx.http, false, |b| {
                    b.content(format!("Just joined {}!", &guild.name))
                }),
            );

            let owner = owner?;
            match owner.direct_message(ctx, |b| {b.embed(|e| {
                e.title(format!("Welcome to {}!", ctx.cache.current_user_field(|b| b.name.clone())));
                e.description(format!("
Hello! Someone invited me to your server `{}`!
TTS Bot is a text to speech bot, as in, it reads messages from a text channel and speaks it into a voice channel
**Most commands need to be done on your server, such as `-setup` and `-join`**
I need someone with the administrator permission to do `-setup #channel`
You can then do `-join` in that channel and I will join your voice channel!
Then, you can just type normal messages and I will say them, like magic!
You can view all the commands with `-help`
Ask questions by either responding here or asking on the support server!",
                guild.name));
                e.footer(|f| {f.text(format!("Support Server: {} | Bot Invite: https://bit.ly/TTSBotSlash", data.config.server_invite))});
                e.author(|a| {a.name(format!("{}#{}", &owner.name, &owner.id)); a.icon_url(owner.face())})
            })}).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {},
                Err(error) => return Err(Error::Unexpected(Box::from(error))),
                _ => {}
            }

            match ctx.http.add_member_role(
                data.config.main_server.into(),
                owner.id.0,
                data.config.ofs_role,
                None
            ).await {
                Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::NOT_FOUND) => {return Ok(())},
                Err(err) => return Err(Error::Unexpected(Box::new(err))),
                Ok(_) => (),
            }

            info!("Added OFS role to {}#{}", owner.name, owner.discriminator);
        }
        poise::Event::GuildDelete { incomplete, full } => {
            let incomplete: &serenity::UnavailableGuild = incomplete;
            let guild: &Option<serenity::Guild> = full;

            data.guilds_db.delete(incomplete.id.into()).await?;
            match guild {
                Some(guild) => {
                    data.webhooks["servers"].execute(&ctx.http, false, |b| {
                        b.content(format!(
                            "Just got kicked from {}. I'm now in {} servers",
                            guild.name, ctx.cache.guilds().len()
                        ))
                    }).await?;
                }
                None => warn!("Guild ID {} just was deleted without being cached!", incomplete.id),
            }
        }
        poise::Event::Ready { data_about_bot } => {
            info!("{} has connected in {} seconds!", data_about_bot.user.name, data.start_time.elapsed()?.as_secs());
        }
        poise::Event::Resume { event: _ } => {
            data.analytics.log(String::from("on_resumed"));
        }
        _ => {}
    }

    Ok(())
}

enum FailurePoint {
    PatreonRole(serenity::UserId),
    PremiumUser,
    Guild,
}

async fn premium_check(data: &Data, guild_id: Option<serenity::GuildId>) -> Result<Option<FailurePoint>, Error> {
    let guild = match guild_id {
        Some(guild) => guild.0 as i64,
        None => return Ok(Some(FailurePoint::Guild))
    };

    let premium_user_id = data
        .guilds_db.get(guild).await?
        .get::<&str, Option<i64>>("premium_user")
        .map(|u| serenity::UserId(u as u64));

    premium_user_id.map_or(
        Ok(Some(FailurePoint::PremiumUser)),
        |premium_user_id|
            if data.premium_users.contains(&premium_user_id) {
                Ok(None)
            } else {
                Ok(Some(FailurePoint::PatreonRole(premium_user_id)))
            }
    )
}

async fn premium_command_check(ctx: structs::Context<'_>) -> Result<bool, Error> {
    let ctx_discord = ctx.discord();
    let guild = ctx.guild();
    let data = ctx.data();

    let main_msg =
        match premium_check(data, guild.as_ref().map(|g| g.id)).await? {
            None => return Ok(true),
            Some(FailurePoint::Guild) => Cow::Borrowed("Hey, this is a premium command so it must be run in a server!"),
            Some(FailurePoint::PremiumUser) => {Cow::Owned(
                format!("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`{}donate`)", ctx.prefix())
            )}
            Some(FailurePoint::PatreonRole(premium_user_id)) => {
                let premium_user = premium_user_id.to_user(ctx_discord).await?;
                Cow::Owned(format!(concat!(
                    "Hey, this server has a premium user setup, however they are not longer a patreon!",
                    "Please ask {}#{} to renew their membership."
                ),premium_user.name, premium_user.discriminator))
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
                b.embed(|e| {
                    e.title("TTS Bot Premium - Premium Only Command!");
                    e.description(main_msg);
                    e.colour(constants::PREMIUM_NEUTRAL_COLOUR);
                    e.thumbnail(data.premium_avatar_url.clone());
                    e.footer(|f| f.text(FOOTER_MSG))
                })
            } else {
                b.content(format!("{}\n{}", main_msg, FOOTER_MSG))
            }
        }).await?;
    }

    Ok(false)
}

async fn _on_error(error: poise::FrameworkError<'_, Data, Error>) -> Result<(), Error> {
    match error {
        poise::FrameworkError::Command { error, ctx } => {
            let command = ctx.command();
            let handle_unexpected = |error: String| {
                error!("Error in {}: {:?}", command.qualified_name, error);
                ("an unknown error occurred".to_owned(), None)
            };

            let (error, fix) = match error {
                Error::GuildOnly => (
                    format!("{} cannot be used in private messages", command.qualified_name),
                    Some(format!(
                        "try running it on a server with {} in",
                        ctx.discord().cache.current_user_field(|b| b.name.clone())
                    )),
                ),
                Error::Unexpected(error) => handle_unexpected(format!("{:?}", error)),
                Error::None(error) => handle_unexpected(error),
                Error::DebugLog(_) => unreachable!(),
            };
            ctx.send_error(&error, fix).await?;
        }
        poise::FrameworkError::Listener{error, event} => {
            if let Error::DebugLog(msg) = error {
                debug!(msg);
            } else {
                error!("Error in event handler: `{}`: `{:?}`", event.name(), error);
            }
        },
        poise::FrameworkError::MissingBotPermissions{missing_permissions, ctx} => {
            ctx.send_error(
                &format!("I cannot run `{}` as I am missing permissions", ctx.command().name),
                Some(format!("give me: {}", missing_permissions.get_permission_names().join(", ")))
            ).await?;
        },
        poise::FrameworkError::MissingUserPermissions{missing_permissions, ctx} => {
            ctx.send_error(
                "you cannot run this command",
                Some(format!(
                    "ask an administator for the following permissions: {}",
                    missing_permissions.ok_or("failed to fetch perms")?.get_permission_names().join(", ")
                ))
            ).await?;
        },
        poise::FrameworkError::ArgumentParse { error, ctx, input } => {
            let fix = None;
            let mut reason = None;

            let argument = || input.unwrap().replace('`', "");
            if error.is::<serenity::MemberParseError>() {
                reason = Some(format!("I cannot find the member: `{}`", argument()));
            } else if error.is::<serenity::GuildParseError>() {
                reason = Some(format!("I cannot find the server: `{}`", argument()));
            } else if error.is::<serenity::GuildChannelParseError>() {
                reason = Some(format!("I cannot find the channel: `{}`", argument()));
            } else if error.is::<std::num::ParseIntError>() {
                reason = Some(format!("I cannot convert `{}` to a number", argument()));
            } else if error.is::<std::str::ParseBoolError>() {
                reason = Some(format!("I cannot convert `{}` to True/False", argument()));
            }

            ctx.send_error(
                &reason.unwrap_or_else(|| String::from("you typed the command wrong")),
                Some(fix.unwrap_or_else(|| format!("check out `{}help {}`", ctx.prefix(), ctx.command().qualified_name)))
            ).await?;
        },
        poise::FrameworkError::CooldownHit { remaining_cooldown, ctx } => {
            let cooldown_response = ctx.send_error(
                &format!("{} is on cooldown", ctx.command().name),
                Some(format!("try again in {:.1} seconds", remaining_cooldown.as_secs_f32()))
            ).await?;

            if let poise::Context::Prefix(ctx) = ctx {
                if let Some(poise::ReplyHandle::Known(error_message)) = cooldown_response {
                    tokio::time::sleep(remaining_cooldown).await;
    
                    let ctx_discord = ctx.discord;
                    error_message.delete(ctx_discord).await?;
                    
                    let bot_user_id = ctx_discord.cache.current_user_id();
                    let channel = error_message.channel(ctx_discord).await?.guild().unwrap();

                    if channel.permissions_for_user(ctx_discord, bot_user_id)?.manage_messages() {
                        ctx.msg.delete(ctx_discord).await?;
                    }
                }
            } 
        },

        poise::FrameworkError::Setup { error } => panic!("{:#?}", error),
        poise::FrameworkError::CommandCheckFailed { error, ctx } => {
            if let Some(error) = error {
                error!("Premium Check Error: {:?}", error);
                ctx.send_error("an unknown error occurred during the premium check", None).await?;
            }
        },

        poise::FrameworkError::CommandStructureMismatch { description: _, ctx: _ } |
        poise::FrameworkError::NotAnOwner{ctx: _}=> {},
    }

    Ok(())
}
async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    _on_error(error).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
}

async fn process_tts_msg(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    let guild = match message.guild(&ctx.cache) {
        Some(guild) => guild,
        None => return Ok(()),
    };

    let guilds_db = &data.guilds_db;
    let nicknames = &data.nickname_db;

    let guild_row = guilds_db.get(guild.id.into()).await?;
    let xsaid = guild_row.get("xsaid");
    let prefix = guild_row.get("prefix");
    let channel: i64 = guild_row.get("channel");
    let autojoin = guild_row.get("auto_join");
    let bot_ignore = guild_row.get("bot_ignore");
    let repeated_limit: i16 = guild_row.get("repeated_chars");
    let audience_ignore = guild_row.get("audience_ignore");

    let mode;
    let voice;

    let lavalink_client = &data.lavalink;
    let mut content = match run_checks(
        ctx, message, lavalink_client,
        channel as u64, prefix, autojoin, bot_ignore, audience_ignore
    ).await? {
        None => return Ok(()),
        Some(content) => {
            let member = guild.member(ctx, message.author.id.0).await?;
            let voice_mode = parse_user_or_guild(data, message.author.id, Some(guild.id)).await?;

            voice = voice_mode.0;
            mode = voice_mode.1;

            let nickname_row = nicknames.get([guild.id.into(), message.author.id.into()]).await?;
            let nickname: Option<String> = nickname_row.get("name");

            clean_msg(
                &content, &guild, member, &message.attachments, &voice,
                xsaid, repeated_limit as usize, nickname,
                &data.last_to_xsaid_tracker
            )?
        }
    };

    let speaking_rate: f32 = data.userinfo_db.get(message.author.id.into()).await?.get("speaking_rate");
    if let Some(target_lang) = guild_row.get("target_lang") {
        if guild_row.get("to_translate") && premium_check(data, Some(guild.id)).await?.is_none() {
            content = funcs::translate(&content, target_lang, data).await?.unwrap_or(content);
        };
    }

    let tts_err = || format!("Guild: {} | Lavalink failed to get track!", guild.id);
    for url in funcs::fetch_url(
        &data.config.tts_service,
        content,
        &voice,
        &mode.to_string(),
        speaking_rate
    ) {
        let tracks = lavalink_client.get_tracks(&url).await?.tracks;
        let track = tracks.into_iter().next().ok_or_else(tts_err)?;

        lavalink_client.play(guild.id, track).queue().await?;
    }

    data.analytics.log(format!("on_{}_tts", mode));
    Ok(())
}

async fn process_mention_msg(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    let bot_user = ctx.cache.current_user();

    let guild = match message.guild(ctx) {
        Some(guild) => guild,
        None => return Ok(()),
    };

    if ![format!("<@{}>", bot_user.id), format!("<@!{}>", bot_user.id)].contains(&message.content) {
        return Ok(());
    };

    let channel = message.channel(ctx).await?.guild().unwrap();
    let permissions = channel.permissions_for_user(ctx, bot_user)?;

    let mut prefix: String = data.guilds_db.get(guild.id.into()).await?.get("prefix");
    prefix = prefix.replace('`', "").replace('\\', "");

    if permissions.send_messages() {
        channel.say(ctx,format!("Current prefix for this server is: {}", prefix)).await?;
    } else {
        let result = message.author.direct_message(ctx, |b|
            b.content(format!("My prefix for `{}` is {} however I do not have permission to send messages so I cannot respond to your commands!", guild.name, prefix))
        ).await;

        match result {
            Err(serenity::Error::Http(error)) if error.status_code() == Some(serenity::StatusCode::FORBIDDEN) => {}
            Err(error) => return Err(Error::Unexpected(Box::from(error))),
            _ => {}
        }
    }

    Ok(())
}

async fn process_support_dm(
    ctx: &serenity::Context,
    message: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    match message.channel(ctx).await? {
        serenity::Channel::Guild(channel) => {
            // Check support server trusted member replies to a DM, if so, pass it through
            if let Some(reference) = &message.message_reference {
                if data.config.main_server != channel.guild_id.0
                    || ["dm_logs", "suggestions"].contains(&channel.name.as_str())
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

                    channel.send_message(ctx, |b| {
                        b.content(content);
                        b.set_embed(serenity::CreateEmbed::from(embed))
                    }).await?;
                }
            }
        }
        serenity::Channel::Private(channel) => {
            if message.author.bot || message.content.starts_with('-') {
                return Ok(());
            }

            data.analytics.log(String::from("on_dm"));

            let userinfo = data.userinfo_db.get(message.author.id.into()).await?;
            if userinfo.get("dm_welcomed") {
                let is_blocked: bool = userinfo.get("dm_blocked");
                let content = message.content.to_lowercase();

                if content.contains("discord.gg") {
                    channel.say(&ctx.http, format!(
                        "Join {} and look in <#{}> to invite {}!",
                        data.config.server_invite, data.config.invite_channel, ctx.cache.current_user_id()
                    )).await?;
                } else if content.as_str() == "help" {
                    channel.say(&ctx.http, "We cannot help you unless you ask a question, if you want the help command just do `-help`!").await?;
                } else if !is_blocked {
                    let display_name = format!("{}#{}", &message.author.name, &message.author.discriminator);
                    let webhook_username = format!("{} ({})", display_name, message.author.id.0);
                    let paths: Vec<serenity::AttachmentType<'_>> = message.attachments.iter()
                        .map(|a| serenity::AttachmentType::Path(Path::new(&a.url)))
                        .collect();

                    data.webhooks["dm_logs"].execute(&ctx.http, false, |b| {
                        b.files(paths);
                        b.content(&message.content);
                        b.username(webhook_username);
                        b.avatar_url(message.author.face());
                        b.embeds(message.embeds.iter().map(|e| serenity::json::prelude::to_value(e).unwrap()).collect(),)
                    }).await?;
                }
            } else {
                let welcome_msg = channel.send_message(&ctx.http, |b| {b.embed(|e| {
                    e.title(format!(
                        "Welcome to {} Support DMs!",
                        ctx.cache.current_user_field(|b| b.name.clone())
                    ));
                    e.description(DM_WELCOME_MESSAGE);
                    e.footer(|f| {f.text(random_footer(
                        Some(&String::from("-")),
                        Some(&data.config.server_invite),
                        Some(ctx.cache.current_user_id().0)
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
