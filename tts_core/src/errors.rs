use std::{borrow::Cow, sync::Arc};

use ::serenity::all::GenericGuildChannelRef;
use anyhow::{Error, Result};
use sha2::Digest;
use tracing::error;

use self::serenity::{
    small_fixed_array::FixedString, CreateActionRow, CreateButton, CreateInteractionResponse,
    FullEvent as Event,
};
use poise::serenity_prelude as serenity;

use crate::{
    common::push_permission_names,
    constants,
    opt_ext::OptionTryUnwrap,
    structs::{Context, Data, FrameworkContext},
    traits::PoiseContextExt,
};

const VIEW_TRACEBACK_CUSTOM_ID: &str = "error::traceback::view";

#[derive(sqlx::FromRow)]
struct ErrorRow {
    pub message_id: i64,
}

#[derive(sqlx::FromRow)]
struct TracebackRow {
    pub traceback: String,
}

#[must_use]
pub const fn blank_field() -> (&'static str, Cow<'static, str>, bool) {
    ("\u{200B}", Cow::Borrowed("\u{200B}"), true)
}

fn hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    Vec::from(&*hasher.finalize())
}

fn truncate_error(error: &Error) -> String {
    let mut long_err = error.to_string();

    // Avoid char boundary panics with utf8 chars
    let mut new_len = 256;
    while !long_err.is_char_boundary(new_len) {
        new_len -= 1;
    }

    long_err.truncate(new_len);
    long_err
}

async fn fetch_update_occurrences(
    data: &Data,
    error: &Error,
) -> Result<Option<(String, Vec<u8>)>, Error> {
    let traceback = format!("{error:?}");
    let traceback_hash = hash(traceback.as_bytes());

    let query =
        "UPDATE errors SET occurrences = occurrences + 1 WHERE traceback_hash = $1 RETURNING ''";
    let result = sqlx::query_as::<_, ()>(query)
        .bind(traceback_hash.clone())
        .fetch_optional(&data.pool)
        .await?;

    if result.is_some() {
        Ok(None)
    } else {
        Ok(Some((traceback, traceback_hash)))
    }
}

async fn insert_traceback(
    http: &serenity::Http,
    data: &Data,
    embed: serenity::CreateEmbed<'_>,
    traceback: String,
    traceback_hash: Vec<u8>,
) -> Result<()> {
    let buttons = [CreateButton::new(VIEW_TRACEBACK_CUSTOM_ID)
        .label("View Traceback")
        .style(serenity::ButtonStyle::Danger)];

    let embeds = [embed];
    let components = [CreateActionRow::buttons(&buttons)];

    let builder = serenity::ExecuteWebhook::default()
        .embeds(&embeds)
        .components(&components);

    let message = data
        .webhooks
        .errors
        .execute(http, true, builder)
        .await?
        .try_unwrap()?;

    let ErrorRow {
        message_id: db_message_id,
    } = sqlx::query_as(
        "INSERT INTO errors(traceback_hash, traceback, message_id)
        VALUES($1, $2, $3)

        ON CONFLICT (traceback_hash)
        DO UPDATE SET occurrences = errors.occurrences + 1
        RETURNING errors.message_id",
    )
    .bind(traceback_hash)
    .bind(traceback)
    .bind(message.id.get() as i64)
    .fetch_one(&data.pool)
    .await?;

    if message.id != db_message_id as u64 {
        data.webhooks
            .errors
            .delete_message(http, None, message.id)
            .await?;
    }

    Ok(())
}

pub async fn handle_unexpected<'a>(
    poise_context: FrameworkContext<'_>,
    event: &'a str,
    error: Error,
    // Split out logic if not reliant on this field, to prevent monomorphisation bloat
    extra_fields: impl IntoIterator<Item = (&str, Cow<'a, str>, bool)>,
    author_name: Option<&str>,
    icon_url: Option<&str>,
) -> Result<()> {
    let data = poise_context.user_data();
    let ctx = poise_context.serenity_context;

    let Some((traceback, traceback_hash)) = fetch_update_occurrences(&data, &error).await? else {
        return Ok(());
    };

    let (cpu_usage, mem_usage) = {
        let mut system = data.system_info.lock();
        system.refresh_specifics(
            sysinfo::RefreshKind::new().with_memory(sysinfo::MemoryRefreshKind::new().with_ram()),
        );

        (
            sysinfo::System::load_average().five.to_string(),
            (system.used_memory() / 1024).to_string(),
        )
    };

    let before_fields = [
        ("Event", Cow::Borrowed(event), true),
        (
            "Bot User",
            Cow::Owned(ctx.cache.current_user().name.to_string()),
            true,
        ),
        blank_field(),
    ];

    let shards = poise_context.shard_manager.shards_instantiated().await;
    let after_fields = [
        ("CPU Usage (5 minutes)", Cow::Owned(cpu_usage), true),
        ("System Memory Usage", Cow::Owned(mem_usage), true),
        ("Shard Count", Cow::Owned(shards.len().to_string()), true),
    ];

    let mut embed = serenity::CreateEmbed::default()
        .title(truncate_error(&error))
        .colour(constants::RED);

    for (title, mut value, inline) in before_fields
        .into_iter()
        .chain(extra_fields)
        .chain(after_fields)
    {
        if value != "\u{200B}" {
            let value = value.to_mut();
            value.insert(0, '`');
            value.push('`');
        };

        embed = embed.field(title, value, inline);
    }

    if let Some(author_name) = author_name {
        let mut author_builder = serenity::CreateEmbedAuthor::new(author_name);
        if let Some(icon_url) = icon_url {
            author_builder = author_builder.icon_url(icon_url);
        }

        embed = embed.author(author_builder);
    }

    insert_traceback(&ctx.http, &data, embed, traceback, traceback_hash).await
}

pub async fn handle_unexpected_default(
    framework: FrameworkContext<'_>,
    name: &str,
    result: Result<()>,
) -> Result<()> {
    let Some(error) = result.err() else {
        return Ok(());
    };

    handle_unexpected(framework, name, error, [], None, None).await
}

// Listener Handlers
pub async fn handle_message(
    poise_context: FrameworkContext<'_>,
    message: &serenity::Message,
    result: Result<impl Send + Sync>,
) -> Result<()> {
    let ctx = poise_context.serenity_context;
    let Some(error) = result.err() else {
        return Ok(());
    };

    let mut extra_fields = Vec::with_capacity(3);
    if let Some(guild_id) = message.guild_id {
        if let Some(guild_name) = ctx.cache.guild(guild_id).map(|g| g.name.to_string()) {
            extra_fields.push(("Guild", Cow::Owned(guild_name), true));
        }

        extra_fields.push(("Guild ID", Cow::Owned(guild_id.to_string()), true));
    }

    extra_fields.push((
        "Channel Type",
        Cow::Borrowed(channel_type(&message.channel(ctx).await?)),
        true,
    ));
    handle_unexpected(
        poise_context,
        "MessageCreate",
        error,
        extra_fields,
        Some(&message.author.name),
        Some(&message.author.face()),
    )
    .await
}

pub async fn handle_member(
    framework: FrameworkContext<'_>,
    member: &serenity::Member,
    result: Result<(), Error>,
) -> Result<()> {
    let Some(error) = result.err() else {
        return Ok(());
    };

    let extra_fields = [
        ("Guild", Cow::Owned(member.guild_id.to_string()), true),
        ("Guild ID", Cow::Owned(member.guild_id.to_string()), true),
        ("User ID", Cow::Owned(member.user.id.to_string()), true),
    ];

    handle_unexpected(framework, "GuildMemberAdd", error, extra_fields, None, None).await
}

pub async fn handle_guild(
    name: &str,
    framework: FrameworkContext<'_>,
    guild: Option<&serenity::Guild>,
    result: Result<()>,
) -> Result<()> {
    let Some(error) = result.err() else {
        return Ok(());
    };

    handle_unexpected(
        framework,
        name,
        error,
        [],
        guild.as_ref().map(|g| g.name.as_str()),
        guild.and_then(serenity::Guild::icon_url).as_deref(),
    )
    .await
}

// Command Error handlers
async fn handle_cooldown(
    ctx: Context<'_>,
    remaining_cooldown: std::time::Duration,
) -> Result<(), Error> {
    let msg = format!(
        "`/{}` is on cooldown, please try again in {:.1} seconds!",
        ctx.command().name,
        remaining_cooldown.as_secs_f32()
    );

    let cooldown_response = ctx.send_error(msg).await?;

    if let poise::Context::Prefix(ctx) = ctx {
        if let Some(error_reply) = cooldown_response {
            // Never actually fetches, as Prefix already has message.
            let error_message = error_reply.into_message().await?;
            tokio::time::sleep(remaining_cooldown).await;

            let ctx_discord = ctx.serenity_context();
            error_message.delete(&ctx_discord.http, None).await?;

            let bot_user_id = ctx_discord.cache.current_user().id;
            let has_permissions = {
                let Some(guild) = ctx.guild() else {
                    return Ok(());
                };

                let bot_member = guild.members.get(&bot_user_id).try_unwrap()?;
                let permissions = match guild.channel(error_message.channel_id) {
                    Some(GenericGuildChannelRef::Channel(channel)) => {
                        guild.user_permissions_in(channel, bot_member)
                    }
                    Some(GenericGuildChannelRef::Thread(thread)) => {
                        let parent = guild.channels.get(&thread.parent_id).try_unwrap()?;
                        guild.user_permissions_in(parent, bot_member)
                    }
                    None => return Err(anyhow::anyhow!("Can't find channel for cooldown message")),
                };

                permissions.manage_messages()
            };

            if has_permissions {
                let reason = "Deleting command invocation that hit cooldown";
                ctx.msg.delete(&ctx_discord.http, Some(reason)).await?;
            }
        }
    };

    Ok(())
}

async fn handle_argparse(
    ctx: Context<'_>,
    error: Box<dyn std::error::Error + Send + Sync>,
    input: Option<String>,
) -> Result<(), Error> {
    let reason = if let Some(input) = input {
        let reason = if error.is::<serenity::MemberParseError>() {
            "I cannot find the member: `{}`"
        } else if error.is::<serenity::GuildParseError>() {
            "I cannot find the server: `{}`"
        } else if error.is::<serenity::GuildChannelParseError>() {
            "I cannot find the channel: `{}`"
        } else if error.is::<std::num::ParseIntError>() {
            "I cannot convert `{}` to a number"
        } else if error.is::<std::str::ParseBoolError>() {
            "I cannot convert `{}` to True/False"
        } else {
            "I cannot understand your message"
        };

        Cow::Owned(reason.replace("{}", &input))
    } else {
        Cow::Borrowed("You missed an argument to the command")
    };

    ctx.send_error(format!(
        "{reason}, please check out `/help {}`",
        ctx.command().qualified_name
    ))
    .await?;
    Ok(())
}

const fn channel_type(channel: &serenity::Channel) -> &'static str {
    use self::serenity::{Channel, ChannelType};

    match channel {
        Channel::Guild(channel) => match channel.base.kind {
            ChannelType::Text | ChannelType::News => "Text Channel",
            ChannelType::Voice => "Voice Channel",
            ChannelType::NewsThread => "News Thread Channel",
            ChannelType::PublicThread => "Public Thread Channel",
            ChannelType::PrivateThread => "Private Thread Channel",
            _ => "Unknown Channel Type",
        },
        Channel::Private(_) => "Private Channel",
        _ => "Unknown Channel Type",
    }
}

pub async fn handle(error: poise::FrameworkError<'_, Data, Error>) -> Result<()> {
    match error {
        poise::FrameworkError::DynamicPrefix { error, .. } => {
            error!("Error in dynamic_prefix: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            let author = ctx.author();
            let command = ctx.command();

            let mut extra_fields = vec![
                ("Command", command.name.clone(), true),
                (
                    "Slash Command",
                    Cow::Owned(matches!(ctx, poise::Context::Application(..)).to_string()),
                    true,
                ),
                (
                    "Channel Type",
                    Cow::Borrowed(channel_type(
                        &ctx.channel_id().to_channel(&ctx, ctx.guild_id()).await?,
                    )),
                    true,
                ),
            ];

            if let Some(guild) = ctx.guild() {
                extra_fields.extend([
                    ("Guild", Cow::Owned(guild.name.to_string()), true),
                    ("Guild ID", Cow::Owned(guild.id.to_string()), true),
                    blank_field(),
                ]);
            }

            handle_unexpected(
                ctx.framework(),
                "command",
                error,
                extra_fields,
                Some(&author.name),
                Some(&author.face()),
            )
            .await?;

            let msg = "An unknown error occurred, please report this on the support server!";
            ctx.send_error(msg).await?;
        }
        poise::FrameworkError::ArgumentParse {
            error, ctx, input, ..
        } => handle_argparse(ctx, error, input).await?,
        poise::FrameworkError::CooldownHit {
            remaining_cooldown,
            ctx,
            ..
        } => handle_cooldown(ctx, remaining_cooldown).await?,

        poise::FrameworkError::PermissionFetchFailed { ctx, .. } => {
            error!(
                "Could not fetch permissions for channel {} in guild {:?}",
                ctx.channel_id(),
                ctx.guild_id()
            );
        }
        poise::FrameworkError::MissingBotPermissions {
            missing_permissions,
            ctx,
            ..
        } => {
            let mut msg = String::from("I cannot run this command as I am missing permissions, please ask an administrator of the server to give me: ");
            push_permission_names(&mut msg, missing_permissions);

            ctx.send_error(msg).await?;
        }
        poise::FrameworkError::MissingUserPermissions {
            missing_permissions,
            ctx,
            ..
        } => {
            let msg = if let Some(missing_permissions) = missing_permissions {
                let mut msg = String::from("You cannot run this command as you are missing permissions, please ask an administrator of the server to give you: ");
                push_permission_names(&mut msg, missing_permissions);
                Cow::Owned(msg)
            } else {
                Cow::Borrowed("You cannot run this command as you are missing permissions.")
            };

            ctx.send_error(msg).await?;
        }

        poise::FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            if let Some(error) = error {
                error!("Premium Check Error: {:?}", error);

                let msg = "An unknown error occurred during the premium check, please report this on the support server!";
                ctx.send_error(msg).await?;
            }
        }

        poise::FrameworkError::EventHandler {
            error,
            event,
            framework,
            ..
        } => {
            #[allow(non_snake_case)]
            fn Err<E>(error: E) -> Result<(), E> {
                Result::Err(error)
            }

            match event {
                Event::Message { new_message } => {
                    handle_message(framework, new_message, Err(error)).await?;
                }
                Event::GuildMemberAddition { new_member } => {
                    handle_member(framework, new_member, Err(error)).await?;
                }
                Event::GuildCreate { guild, .. } => {
                    handle_guild("GuildCreate", framework, Some(guild), Err(error)).await?;
                }
                Event::GuildDelete { full, .. } => {
                    handle_guild("GuildDelete", framework, full.as_ref(), Err(error)).await?;
                }
                Event::VoiceStateUpdate { .. } => {
                    handle_unexpected_default(framework, "VoiceStateUpdate", Err(error)).await?;
                }
                Event::InteractionCreate { .. } => {
                    handle_unexpected_default(framework, "InteractionCreate", Err(error)).await?;
                }
                Event::Ready { .. } => {
                    handle_unexpected_default(framework, "Ready", Err(error)).await?;
                }
                _ => {
                    tracing::warn!("Unhandled {} error: {:?}", event.snake_case_name(), error);
                }
            }
        }
        poise::FrameworkError::CommandStructureMismatch { .. }
        | poise::FrameworkError::DmOnly { .. }
        | poise::FrameworkError::NsfwOnly { .. }
        | poise::FrameworkError::NotAnOwner { .. }
        | poise::FrameworkError::UnknownInteraction { .. }
        | poise::FrameworkError::SubcommandRequired { .. }
        | poise::FrameworkError::UnknownCommand { .. }
        | poise::FrameworkError::NonCommandMessage { .. } => {}
        poise::FrameworkError::GuildOnly { ctx, .. } => {
            let error = format!("`/{}` cannot be used in private messages, please run this command in a server channel.", ctx.command().qualified_name);
            ctx.send_error(error).await?;
        }
        poise::FrameworkError::CommandPanic { .. } => panic!("Command panicked!"),
    }

    Ok(())
}

pub async fn interaction_create(
    framework: FrameworkContext<'_>,
    interaction: &serenity::Interaction,
) -> Result<(), Error> {
    if let serenity::Interaction::Component(interaction) = interaction {
        if interaction.data.custom_id == VIEW_TRACEBACK_CUSTOM_ID {
            handle_traceback_button(framework, interaction).await?;
        };
    };

    Ok(())
}

pub async fn handle_traceback_button(
    framework: FrameworkContext<'_>,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let row: Option<TracebackRow> =
        sqlx::query_as("SELECT traceback FROM errors WHERE message_id = $1")
            .bind(interaction.message.id.get() as i64)
            .fetch_optional(&framework.user_data().pool)
            .await?;

    let mut response_data = serenity::CreateInteractionResponseMessage::default().ephemeral(true);
    response_data = if let Some(TracebackRow { traceback }) = row {
        response_data.files([serenity::CreateAttachment::bytes(
            traceback.into_bytes(),
            "traceback.txt",
        )])
    } else {
        response_data.content("No traceback found.")
    };

    interaction
        .create_response(
            &framework.serenity_context.http,
            CreateInteractionResponse::Message(response_data),
        )
        .await?;
    Ok(())
}

struct TrackErrorHandler<Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)>> {
    ctx: serenity::Context,
    shard_manager: Arc<serenity::ShardManager>,
    extra_fields: Iter,
    author_name: FixedString<u8>,
    icon_url: String,
}

#[serenity::async_trait]
impl<Iter> songbird::EventHandler for TrackErrorHandler<Iter>
where
    Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)> + Clone + Send + Sync,
{
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let songbird::EventContext::Track([(state, _)]) = ctx {
            if let songbird::tracks::PlayMode::Errored(error) = state.playing.clone() {
                // HACK: Cannot get reference to options from here, so has to be faked.
                // This is fine because the options are not used in the error handler.
                let framework_context = FrameworkContext {
                    serenity_context: &self.ctx,
                    shard_manager: &self.shard_manager,
                    options: &poise::FrameworkOptions::default(),
                };

                let author_name = Some(self.author_name.as_str());
                let icon_url = Some(self.icon_url.as_str());

                let result = handle_unexpected(
                    framework_context,
                    "TrackError",
                    error.into(),
                    self.extra_fields.clone(),
                    author_name,
                    icon_url,
                )
                .await;

                if let Err(err_err) = result {
                    tracing::error!("Songbird unhandled track error: {err_err}");
                }
            }
        }

        Some(songbird::Event::Cancel)
    }
}

/// Registers a track to be handled by the error handler, arguments other than the
/// track are passed to [`handle_unexpected`] if the track errors.
pub fn handle_track<Iter>(
    ctx: serenity::Context,
    shard_manager: Arc<serenity::ShardManager>,
    extra_fields: Iter,
    author_name: FixedString<u8>,
    icon_url: String,

    track: &songbird::tracks::TrackHandle,
) -> Result<(), songbird::error::ControlError>
where
    Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)>
        + Clone
        + Send
        + Sync
        + 'static,
{
    track.add_event(
        songbird::Event::Track(songbird::TrackEvent::Error),
        TrackErrorHandler {
            ctx,
            shard_manager,
            extra_fields,
            author_name,
            icon_url,
        },
    )
}
