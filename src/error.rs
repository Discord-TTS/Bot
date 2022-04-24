use std::borrow::Cow;

use sysinfo::SystemExt;
use tracing::error;
use sha2::Digest;

use poise::serenity_prelude as serenity;
use serenity::json::prelude as json;

use crate::{
    structs::{Context, Error, Data, Framework, PoiseContextExt, Result},
    constants::{RED, VIEW_TRACEBACK_CUSTOM_ID},
    funcs::refresh_kind, macros::require
};

#[allow(clippy::module_name_repetitions)]
pub type CommandError = Error;

#[derive(sqlx::FromRow)]
struct ErrorRowWithOccurrences {
    pub message_id: i64,
    pub occurrences: i32,
}

#[derive(sqlx::FromRow)]
struct ErrorRow {
    pub message_id: i64
}

#[derive(sqlx::FromRow)]
struct TracebackRow {
    pub traceback: String
}


const fn blank_field() -> (&'static str, Cow<'static, str>, bool) {
    ("\u{200B}", Cow::Borrowed("\u{200B}"), true)
}

fn hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    Vec::from(&*hasher.finalize())
}

async fn handle_unexpected(
    ctx: &serenity::Context,
    framework: &Framework,
    event: &str,
    error: Error,
    extra_fields: Vec<(&str, Cow<'_, str>, bool)>,
    author_name: Option<String>,
    icon_url: Option<String>
) -> Result<()> {
    let data = framework.user_data().await;
    let error_webhook = &data.webhooks["errors"];

    let traceback = error.backtrace().to_string();
    let traceback_hash = hash(traceback.as_bytes());

    let short_error = error.to_string();
    let mut conn = data.pool.acquire().await?;

    if let Some(ErrorRowWithOccurrences{message_id, occurrences}) = sqlx::query_as("
        UPDATE errors SET occurrences = occurrences + 1
        WHERE traceback_hash = $1
        RETURNING message_id, occurrences
    ").bind(traceback_hash.clone()).fetch_optional(&mut conn).await? {
        let message_id = serenity::MessageId(message_id as u64);
        let mut message = error_webhook.get_message(&ctx.http, message_id).await?;
        let embed = &mut message.embeds[0];

        let footer = format!("This error has occurred {} times!", occurrences);
        embed.footer.as_mut().unwrap().text = footer;

        error_webhook.edit_message(ctx, message_id,  |m| {m.embeds(vec![
            json::to_value(embed).unwrap()
        ])}).await?;
    } else {
        let (cpu_usage, mem_usage) ={
            let mut system = data.system_info.lock();
            system.refresh_specifics(refresh_kind());

            (
                system.load_average().five.to_string(),
                (system.used_memory() / 1024).to_string()
            )
        };

        let before_fields = [
            ("Event", Cow::Borrowed(event), true),
            ("Bot User", Cow::Owned(ctx.cache.current_user_field(|u| u.name.clone())), true),
            blank_field(),
        ];

        let after_fields = [
            ("CPU Usage (5 minutes)", Cow::Owned(cpu_usage), true),
            ("System Memory Usage", Cow::Owned(mem_usage), true),
            ("Shard Count", Cow::Owned(framework.shard_manager().lock().await.shards_instantiated().await.len().to_string()), true),
        ];

        let embed = serenity::Embed::fake(|e| {
            before_fields.into_iter()
                .chain(extra_fields)
                .chain(after_fields)
                .for_each(|(title, value, inline)| {
                    e.field(
                        title, 
                        if value == "\u{200B}" {value.into_owned()} else {format!("`{value}`")},
                        inline
                    );
                });

            if let Some(author_name) = author_name {
                e.author(|a| {
                    if let Some(icon_url) = icon_url {
                        a.icon_url(icon_url);
                    }
                    a.name(author_name)
                });
            }

            e.footer(|f| f.text("This error has occurred 1 time!"));
            e.title(short_error);
            e.colour(RED)
        });

        let message = error_webhook.execute(&ctx.http, true, |b| {b
            .embeds(vec![embed])
            .components(|c| c.create_action_row(|a| a.create_button(|b| {b
                .label("View Traceback")
                .custom_id(VIEW_TRACEBACK_CUSTOM_ID)
                .style(serenity::ButtonStyle::Danger)
            })))
        }).await?.unwrap();

        let ErrorRow{message_id} = sqlx::query_as("
            INSERT INTO errors(traceback_hash, traceback, message_id)
            VALUES($1, $2, $3)

            ON CONFLICT (traceback_hash)
            DO UPDATE SET occurrences = errors.occurrences + 1
            RETURNING errors.message_id
        ",).bind(traceback_hash).bind(traceback).bind(message.id.0 as i64).fetch_one(&mut conn).await?;

        if message.id.0 != (message_id as u64) {
            error_webhook.delete_message(&ctx.http, message.id).await?;
        }
    };

    Ok(())
}

pub async fn handle_unexpected_default(ctx: &serenity::Context, framework: &Framework, name: &str, result: Result<()>) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    handle_unexpected(
        ctx, framework, name,
        error, Vec::new(),
        None, None
    ).await
}


// Listener Handlers
pub async fn handle_message<T>(ctx: &serenity::Context, framework: &Framework, message: &serenity::Message, result: Result<T>) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    let mut extra_fields = Vec::with_capacity(3);
    if let Some(guild_id) = message.guild_id {
        if let Some(guild_name) = ctx.cache.guild_field(guild_id, |g| g.name.clone()) {
            extra_fields.push(("Guild", Cow::Owned(guild_name), true));
        }

        extra_fields.push(("Guild ID", Cow::Owned(guild_id.0.to_string()), true));
    }

    extra_fields.push(("Channel Type", Cow::Borrowed(channel_type(&message.channel_id.to_channel(&ctx).await?)), true));
    handle_unexpected(
        ctx, framework, "MessageCreate",
        error, extra_fields,
        Some(message.author.name.clone()), Some(message.author.face())
    ).await
}

pub async fn handle_guild(name: &str, ctx: &serenity::Context, framework: &Framework, guild: Option<&serenity::Guild>, result: Result<()>) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    handle_unexpected(
        ctx, framework, name,
        error, Vec::new(),
        guild.as_ref().map(|g| g.name.clone()),
        guild.and_then(serenity::Guild::icon_url),
    ).await
}


// Command Error handlers
async fn handle_cooldown(ctx: Context<'_>, remaining_cooldown: std::time::Duration) -> Result<(), Error> {
    let cooldown_response = ctx.send_error(
        &ctx.gettext("{command_name} is on cooldown").replace("{command_name}", ctx.command().name),
        Some(&ctx.gettext("try again in {} seconds").replace("{}", &format!("{:.1}", remaining_cooldown.as_secs_f32())))
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
    };

    Ok(())
}

async fn handle_argparse(ctx: Context<'_>, error: Box<dyn std::error::Error + Send + Sync>, input: Option<String>) -> Result<(), Error> {
    let fix = None;
    let mut reason = None;

    if error.is::<serenity::MemberParseError>() {
        reason = Some(ctx.gettext("I cannot find the member: `{}`"));
    } else if error.is::<serenity::GuildParseError>() {
        reason = Some(ctx.gettext("I cannot find the server: `{}`"));
    } else if error.is::<serenity::GuildChannelParseError>() {
        reason = Some(ctx.gettext("I cannot find the channel: `{}`"));
    } else if error.is::<std::num::ParseIntError>() {
        reason = Some(ctx.gettext("I cannot convert `{}` to a number"));
    } else if error.is::<std::str::ParseBoolError>() {
        reason = Some(ctx.gettext("I cannot convert `{}` to True/False"));
    }

    ctx.send_error(
        reason.map(|r| r.replace("{}", &input.unwrap()).replace('`', "")).as_deref().unwrap_or("you typed the command wrong"),
        Some(&fix.unwrap_or_else(|| ctx
            .gettext("check out `{prefix}help {command}`")
            .replace("command", &ctx.command().qualified_name))
            .replace("{prefix}", ctx.prefix())
        )
    ).await?;

    Ok(())
}


const fn channel_type(channel: &serenity::Channel) -> &'static str {
    match channel {
        serenity::Channel::Guild(channel)  => match channel.kind {
            serenity::ChannelType::Text | serenity::ChannelType::News => "Text Channel",   
            serenity::ChannelType::Voice => "Voice Channel",
            serenity::ChannelType::NewsThread => "News Thread Channel",
            serenity::ChannelType::PublicThread => "Public Thread Channel",
            serenity::ChannelType::PrivateThread => "Private Thread Channel",
            _ => "Unknown Channel Type",
        },
        serenity::Channel::Private(_) => "Private Channel",
        serenity::Channel::Category(_) => "Category Channel??",
        _ => "Unknown Channel Type",
    }
}

pub async fn handle(error: poise::FrameworkError<'_, Data, CommandError>) -> Result<(), Error> {
    match error {
        poise::FrameworkError::DynamicPrefix { error } => error!("Error in dynamic_prefix: {:?}", error),
        poise::FrameworkError::Command { error, ctx } => {
            let author = ctx.author();
            let command = ctx.command();

            let mut extra_fields = vec![
                ("Command", Cow::Borrowed(command.name), true),
                ("Slash Command", Cow::Owned(matches!(ctx, poise::Context::Application(..)).to_string()), true),
                ("Channel Type", Cow::Borrowed(channel_type(&ctx.channel_id().to_channel(ctx.discord()).await?)), true),
            ];

            if let Some(guild) = ctx.guild() {
                extra_fields.extend([
                    ("Guild", Cow::Owned(guild.name), true),
                    ("Guild ID", Cow::Owned(guild.id.0.to_string()), true),
                    blank_field()
                ]);
            }

            handle_unexpected(
                ctx.discord(), ctx.framework(),
                "command", error, extra_fields,
                Some(author.name.clone()), Some(author.face())
            ).await?;

            ctx.send_error("an unknown error occurred", None).await?;
        }
        poise::FrameworkError::ArgumentParse { error, ctx, input } => handle_argparse(ctx, error, input).await?,
        poise::FrameworkError::CooldownHit { remaining_cooldown, ctx } => handle_cooldown(ctx, remaining_cooldown).await?,
        poise::FrameworkError::MissingBotPermissions{missing_permissions, ctx} => {
            ctx.send_error(
                &ctx.gettext("I cannot run `{command}` as I am missing permissions").replace("{command}", ctx.command().name),
                Some(&ctx.gettext("give me: {}").replace("{}", &missing_permissions.get_permission_names().join(", ")))
            ).await?;
        },
        poise::FrameworkError::MissingUserPermissions{missing_permissions, ctx} => {
            ctx.send_error(
                ctx.gettext("you cannot run this command"),
                missing_permissions.map(|missing_permissions| (ctx
                    .gettext("ask an administator for the following permissions: {}")
                    .replace("{}", &missing_permissions.get_permission_names().join(", "))
                )).as_deref()
            ).await?;
        },

        poise::FrameworkError::Setup { error } => panic!("{:#?}", error),
        poise::FrameworkError::CommandCheckFailed { error, ctx } => {
            if let Some(error) = error {
                error!("Premium Check Error: {:?}", error);
                ctx.send_error(ctx.gettext("an unknown error occurred during the premium check"), None).await?;
            }
        },

        poise::FrameworkError::Listener{..} => unreachable!("Listener error, but no listener???"),
        poise::FrameworkError::CommandStructureMismatch {description: _, ctx: _} |
        poise::FrameworkError::DmOnly {ctx: _ } |
        poise::FrameworkError::NsfwOnly {ctx: _} | 
        poise::FrameworkError::NotAnOwner{ctx: _} => {},
        poise::FrameworkError::GuildOnly {ctx} => {
            ctx.send_error(
                &ctx.gettext("{command_name} cannot be used in private messages").replace("{}", &ctx.command().qualified_name),
                Some(&ctx.discord().cache.current_user_field(|b| ctx
                    .gettext("try running it on a server with {bot_name} in")
                    .replace("{bot_name}", &b.name)
                ))
            ).await?;
        },
    }

    Ok(())
}

pub async fn handle_traceback_button(ctx: &serenity::Context, data: &Data, interaction: serenity::MessageComponentInteraction) -> Result<(), Error> {
    let row: Option<TracebackRow> = sqlx::query_as("SELECT traceback FROM errors WHERE message_id = $1")
        .bind(interaction.message.id.0 as i64)
        .fetch_optional(&data.pool)
        .await?;

    interaction.create_interaction_response(&ctx.http, |r| {r
        .kind(serenity::InteractionResponseType::ChannelMessageWithSource)
        .interaction_response_data(move |d| {
            d.ephemeral(true);

            if let Some(TracebackRow{traceback}) = row {
                d.files([serenity::AttachmentType::Bytes {
                    data: Cow::Owned(traceback.into_bytes()),
                    filename: String::from("traceback.txt")
                }])
            } else {
                d.content("No traceback found.")
            }
        })
    }).await?;

    Ok(())
}
