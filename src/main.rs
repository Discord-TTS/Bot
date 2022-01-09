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

#![feature(try_blocks)]
#![warn(rust_2018_idioms)]

use std::collections::HashMap;
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

use database::DatabaseHandler;
use analytics::AnalyticsHandler;
use constants::DM_WELCOME_MESSAGE;
use funcs::{clean_msg, parse_voice, run_checks, random_footer};
use structs::{Config, Data, Error, PoiseContextAdditions, SerenityContextAdditions};


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
            name.to_owned(),
            http.get_webhook_with_token(
                captures.get(1)?.as_str().parse().ok()?,
                captures.get(2)?.as_str(),
            ).await.ok()?,
        );
    }

    Some(webhooks)
}

#[tokio::main]
async fn main() {
    let start_time = std::time::SystemTime::now();
    let mut config_toml = get_config().unwrap();

    // Setup database pool
    let db_config_toml = &config_toml["PostgreSQL-Info"];
    let mut db_config = deadpool_postgres::Config::new();
    db_config.user = Some(String::from(db_config_toml["user"].as_str().ok_or("").unwrap()));
    db_config.host = Some(String::from(db_config_toml["host"].as_str().ok_or("").unwrap()));
    db_config.password = Some(String::from(db_config_toml["password"].as_str().ok_or("").unwrap()));
    db_config.dbname = Some(format!(
        "{}{}", db_config_toml["database"].as_str().ok_or("").unwrap(),
        if cfg!(feature="premium") {"_premium"} else {""}
    ));

    let pool = Arc::new(db_config.create_pool(
        Some(deadpool_postgres::Runtime::Tokio1),
        tokio_postgres::NoTls,
    ).unwrap());

    migration::start_migration(&mut config_toml, &pool).await.unwrap();

    let config = {
        let main_config = &config_toml["Main"];
        Config {
            #[cfg(feature="premium")] patreon_role: serenity::RoleId(main_config["patreon_role"].as_integer().unwrap() as u64),
            #[cfg(feature="premium")] translation_token: String::from(main_config["translation_token"].as_str().unwrap()),

            server_invite: String::from(main_config["main_server_invite"].as_str().unwrap()),

            invite_channel: main_config["invite_channel"].clone().as_integer().unwrap() as u64,
            main_server: main_config["main_server"].as_integer().unwrap() as u64,
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
        String::from("[Main]"),
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
    let analytics = Arc::new(AnalyticsHandler::new(pool.clone()));
    {
        let analytics_sender = analytics.clone();
        tokio::spawn(async move {analytics_sender.loop_task().await});
    }

    let guilds_db = DatabaseHandler::new(pool.clone(), 0,
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
    let userinfo_db = DatabaseHandler::new(pool.clone(), 0,
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
    let nickname_db = DatabaseHandler::new(pool.clone(), [0, 0],
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

    let reqwest = reqwest::Client::new();
    let framework = poise::Framework::build()
        .token(token)
        .client_settings(move |f| {
            f.intents(
                serenity::GatewayIntents::GUILDS
                | serenity::GatewayIntents::GUILD_MESSAGES
                | serenity::GatewayIntents::DIRECT_MESSAGES
                | serenity::GatewayIntents::GUILD_VOICE_STATES
                | serenity::GatewayIntents::GUILD_MEMBERS,
            )
            .register_songbird()
        })
        .user_data_setup(move |_ctx, _ready, _framework| {Box::pin(async move {Ok(Data {
                guilds_db, userinfo_db, nickname_db, analytics, lavalink, webhooks, start_time, reqwest, config,
                owner_id: application_info.owner.id,

                #[cfg(feature="premium")]
                voices: crate::funcs::get_supported_languages(),
                #[cfg(feature="premium")]
                jwt_token: parking_lot::Mutex::new(String::new()),
                #[cfg(feature="premium")]
                jwt_expire: parking_lot::Mutex::new(std::time::SystemTime::now()),
                #[cfg(feature="premium")]
                service_acc: serenity::json::prelude::from_str(
                    &std::fs::read_to_string(std::env::var("GOOGLE_APPLICATION_CREDENTIALS").unwrap()).unwrap()
                ).unwrap()
        })})})
        .options(poise::FrameworkOptions {
            allowed_mentions: Some(
                serenity::CreateAllowedMentions::default()
                    .parse(serenity::ParseValue::Users)
                    .replied_user(true)
                    .to_owned()
            ),
            pre_command: |ctx| Box::pin(async move {
                ctx.data().analytics.log(match ctx {
                    poise::Context::Prefix(_) => "on_command",
                    poise::Context::Application(_) => "on_slash_command",
                })
            }),
            #[cfg(feature="premium")]
            command_check: Some(|ctx| Box::pin(premium_check(ctx))),
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
                ..Default::default()
            },
            // Add all the commands, this ordering is important as it is shown on the help command
            commands: vec![
                commands::main::join(), commands::main::skip(), commands::main::leave(),

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
                        commands::settings::language(), commands::settings::server_language(), commands::settings::prefix(),
                        #[cfg(feature="premium")] commands::settings::translation(),
                        #[cfg(feature="premium")] commands::settings::translation_lang(),
                        commands::settings::nick(), commands::settings::repeated_characters()
                    ],
                    ..commands::settings::set()
                },
                commands::settings::setup(), commands::settings::languages(),

                commands::help::help(),
                commands::owner::dm(), commands::owner::close(), commands::owner::debug(), commands::owner::register(),
            ],..Default::default()
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
            let [mut s1, mut s2] = [
                tokio::signal::windows::ctrl_c().unwrap(),
                tokio::signal::windows::ctrl_break().unwrap()
            ];

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

            tts_result?;
            support_result?;
            mention_result?;
        }
        poise::Event::VoiceStateUpdate { old, new } => {
            // If (on leave) the bot should also leave as it is alone
            let bot_id = ctx.cache.current_user_id();
            let guild_id = new.guild_id.ok_or("no guild_id")?;
            let bot_voice_client = songbird::get(ctx).await.unwrap().get(guild_id);
            if bot_voice_client.is_some()
                && old.is_some() && new.channel_id.is_none() // user left vc
                && !new.member // user other than bot leaving
                    .as_ref()
                    .map(|m| m.user.id == bot_id)
                    .unwrap_or(false)
                && !ctx.cache // filter out bots from members
                    .guild_channel(old.as_ref().unwrap().channel_id.unwrap())
                    .ok_or("no channel")?
                    .members(&ctx.cache).await?
                    .iter().any(|m| !m.user.bot)
            {
                songbird::get(ctx).await.unwrap().remove(guild_id).await?;
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
                data.config.main_server,
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
            let incomplete: &serenity::GuildUnavailable = incomplete;
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
            info!("{} has connected!", data_about_bot.user.name)
        }
        poise::Event::Resume { event: _ } => {
            data.analytics.log("on_resumed")
        }
        _ => {}
    }

    Ok(())
}

#[cfg(feature="premium")]
async fn premium_check(ctx: structs::Context<'_>) -> Result<bool, Error> {
    let author = ctx.author();
    let guild = match ctx.guild() {
        Some(guild) => guild,
        None => return Ok(true)
    };

    if ctx.framework().options().owners.contains(&author.id) || ["donate", "add_premium"].contains(&ctx.command().name) {
        return Ok(true)
    };

    let data = ctx.data();
    let premium_user: Option<i64> = data 
        .guilds_db.get(guild.id.into()).await?
        .get("premium_user");

    let mut main_msg = format!("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`{}donate`)", ctx.prefix());
    let footer_msg = "If this is an error, please contact Gnome!#6669.";

    let ctx_discord = ctx.discord();
    if let Some(premium_user_id) = premium_user {
        match guild.member(ctx_discord, premium_user_id as u64).await {
            Ok(premium_member) => {
                if premium_member.roles.contains(&data.config.patreon_role) {
                    return Ok(true)
                } else if premium_member.user.id == author.id {
                    main_msg = format!("Hey, you have purchased TTS Bot Premium however have not activated it, please run the `{}activate` command!", ctx.prefix())
                }
            },
            Err(_) => main_msg = format!("Hey, you are not in {} so TTS Bot Premium cannot validate your membership!", data.config.server_invite)
        };

    }

    warn!(
        "{}#{} | {} failed premium check in {} | {}",
        author.name, author.discriminator, author.id, guild.name, guild.id
    );

    let permissions = ctx.author_permissions().await?;
    if permissions.send_messages() {
        ctx.send(|b| {
            if permissions.embed_links() {
                b.embed(|e| {
                    e.title("TTS Bot Premium");
                    e.description(main_msg);
                    e.colour(constants::NETURAL_COLOUR);
                    e.footer(|f| f.text(footer_msg));
                    e.thumbnail(ctx_discord.cache.current_user_field(|u| u.face()))
                })
            } else {
                b.content(format!("{}\n{}", main_msg, footer_msg))
            }
        }).await?;
    }

    Ok(false)
}

async fn _on_error(error: poise::FrameworkError<'_, Data, Error>) -> Result<(), Error> {
    match error {
        poise::FrameworkError::Command { error, ctx } => {
            let command = ctx.command();
            let (error, fix) = match error {
                Error::GuildOnly => (
                    format!("{} cannot be used in private messages", command.qualified_name),
                    Some(format!(
                        "try running it on a server with {} in",
                        ctx.discord().cache.current_user_field(|b| b.name.clone())
                    )),
                ),
                Error::Tts(resp) => {
                    error!("TTS Generation Error: {:?}", resp);
                    dbg!(resp.text().await.unwrap_or_else(|_| String::from("")));

                    ("I failed to generate TTS".to_owned(), None)
                }
                Error::Unexpected(error) => {
                    error!("Error in {}: {:?}", command.qualified_name, error);
                    ("an unknown error occurred".to_owned(), None)
                },
                Error::DebugLog(_) => unreachable!(),
            };
            ctx.send_error(&error, fix).await?;
        }
        poise::FrameworkError::Listener{error, event} => {
            if let Error::DebugLog(msg) = error {
                debug!(msg);
            } else {
                error!("Error in event handler: `{}`: `{:?}`", event.name(), error)
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
                reason = Some(format!("I cannot find the member: `{}`", argument()))
            } else if error.is::<serenity::GuildChannelParseError>() {
                reason = Some(format!("I cannot find the channel: `{}`", argument()))
            } else if error.is::<std::num::ParseIntError>() {
                reason = Some(format!("I cannot convert `{}` to a number", argument()))
            } else if error.is::<std::str::ParseBoolError>() {
                reason = Some(format!("I cannot convert `{}` to True/False", argument()))
            }

            ctx.send_error(
                &reason.unwrap_or_else(|| String::from("you typed the command wrong")),
                Some(fix.unwrap_or_else(|| format!("check out {}help {}", ctx.prefix(), ctx.command().name)))
            ).await?;
        },
        poise::FrameworkError::CooldownHit { remaining_cooldown, ctx } => {
            let cooldown_response = ctx.send_error(
                &format!("{} is on cooldown", ctx.command().name),
                Some(format!("try again in {:.1} seconds", remaining_cooldown.as_secs_f32()))
            ).await?;

            if let poise::Context::Prefix(ctx) = ctx {
                if let Some(poise::ReplyHandle::Prefix(error_message)) = cooldown_response {
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

        poise::FrameworkError::NotAnOwner{ctx: _} => {},
        poise::FrameworkError::CommandCheckFailed { error, ctx } => {
            if let Some(error) = error {
                error!("Premium Check Error: {:?}", error);
                ctx.send_error("an unknown error occurred during the premium check", None).await?;
            }
        },
        poise::FrameworkError::Setup { error } => panic!("{:#?}", error),
        poise::FrameworkError::CommandStructureMismatch { description: _, ctx: _ } => {},   
    }

    Ok(())
}
async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    _on_error(error).await.unwrap_or_else(|err| error!("on_error: {:?}", err))
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

    let voice: String;
    let lavalink_client = &data.lavalink;
    let content = match run_checks(
        ctx, message, lavalink_client,
        channel as u64, prefix, autojoin, bot_ignore
    ).await? {
        None => return Ok(()),
        Some(content) => {
            let member = guild.member(ctx, message.author.id.0).await?;
            voice = parse_voice(
                guilds_db, &data.userinfo_db,
                message.author.id,
                Some(guild.id),
            ).await?;

            let nickname_row = nicknames.get([guild.id.into(), message.author.id.into()]).await?;
            let nickname: Option<String> = nickname_row.get("name");

            clean_msg(
                content, member, &message.attachments, &voice,
                xsaid, repeated_limit as usize, nickname,
            )?
        }
    };

    #[cfg(feature="premium")] {
        let target_lang: Option<String> = guild_row.get("target_lang");
        let query_json = funcs::generate_google_json(&
            if let Some(target_lang) = target_lang {
                if guild_row.get("to_translate") {
                    funcs::translate(&content, &target_lang, data).await?.unwrap_or(content)
                } else {content}
            } else {content},
            &voice
        )?;

        let mut query = url::Url::parse("tts://")?;
        query.query_pairs_mut()
            .append_pair("config", &query_json.to_string())
            .finish();

        let tracks = lavalink_client.get_tracks(&query).await?.tracks;
        let track = tracks.first().ok_or(format!("Couldn't fetch URL: {query}"))?;

        lavalink_client.play(guild.id, track.to_owned()).queue().await?;
    }
    #[cfg(not(feature="premium"))]{
        for url in crate::funcs::parse_url(&content, &voice) {
            let tracks = lavalink_client.get_tracks(&url).await?.tracks;
            let track = tracks.first().ok_or(format!("Couldn't fetch URL: {url}"))?;
            lavalink_client.play(guild.id, track.to_owned()).queue().await?;
        }
    }

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

            data.analytics.log("on_dm");

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
                    welcome_msg.pin(ctx).await?
                }

                info!("{}#{} just got the 'Welcome to support DMs' message", message.author.name, message.author.discriminator);                
            }
        }
        _ => {}
    }

    Ok(())
}
