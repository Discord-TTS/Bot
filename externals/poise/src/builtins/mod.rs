//! Building blocks for common commands
//!
//! This file provides sample commands and utility functions like pagination or error handlers to
//! use as a starting point for the framework.

use std::fmt::{self, Display};

mod register;
pub use register::*;

mod paginate;
pub use paginate::*;

use crate::serenity_prelude::CreateAllowedMentions;
use crate::{CreateReply, serenity_prelude as serenity};

/// An error handler that logs errors via either the [`tracing`] crate or a Discord message. Set
/// up a logger like tracing subscriber (e.g., `tracing_subscriber::fmt::init()`) to see the logged
/// errors from this method.
///
/// If the user invoked the command incorrectly ([`crate::FrameworkError::ArgumentParse`]), the
/// command help is displayed and the user is directed to the help menu.
///
/// # Errors
///
/// Can return an error if sending the Discord error message failed. You can decide for yourself
/// how to handle this, for example:
/// ```rust,no_run
/// # async { let error: poise::FrameworkError<'_, (), &str> = todo!();
/// if let Err(e) = poise::builtins::on_error(error).await {
///     tracing::error!("Fatal error while sending error message: {e}");
/// }
/// # };
/// ```
pub async fn on_error<
    U: Send + Sync + 'static,
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
>(
    error: crate::FrameworkError<'_, U, E>,
) -> Result<(), serenity::Error> {
    match error {
        crate::FrameworkError::Command { ctx, error } => {
            let error = display_error(error).to_string();
            eprintln!("An error occured in a command: {error}");

            let mentions =
                CreateAllowedMentions::new().everyone(false).all_roles(false).all_users(false);

            ctx.send(
                CreateReply::default().content(error).allowed_mentions(mentions).ephemeral(true),
            )
            .await?;
        },
        crate::FrameworkError::SubcommandRequired { ctx } => {
            let subcommands =
                ctx.command().subcommands.iter().map(|s| &*s.name).collect::<Vec<_>>();
            let response = format!(
                "You must specify one of the following subcommands: {}",
                subcommands.join(", ")
            );
            ctx.send(CreateReply::default().content(response).ephemeral(true)).await?;
        },
        crate::FrameworkError::CommandPanic { ctx, payload: _ } => {
            // Not showing the payload to the user because it may contain sensitive info
            let embed = serenity::CreateEmbed::default()
                .title("Internal error")
                .color((255, 0, 0))
                .description("An unexpected internal error has occurred.");

            ctx.send(CreateReply::default().embed(embed).ephemeral(true)).await?;
        },
        crate::FrameworkError::ArgumentParse { ctx, input, error } => {
            // If we caught an argument parse error, give a helpful error message with the
            // command explanation if available
            let usage = match &ctx.command().help_text {
                Some(help_text) => &**help_text,
                None => "Please check the help menu for usage information",
            };
            let response = if let Some(input) = input {
                format!("**Cannot parse `{input}` as argument: {error}**\n{usage}")
            } else {
                format!("**{error}**\n{usage}")
            };

            let mentions =
                CreateAllowedMentions::new().everyone(false).all_roles(false).all_users(false);

            ctx.send(
                CreateReply::default().content(response).allowed_mentions(mentions).ephemeral(true),
            )
            .await?;
        },
        crate::FrameworkError::CommandStructureMismatch { ctx, description } => {
            tracing::error!(
                "Error: failed to deserialize interaction arguments for `/{}`: {}",
                ctx.command().name,
                description,
            );
        },
        crate::FrameworkError::CommandCheckFailed { ctx, error } => {
            if let Some(error) = error {
                tracing::error!(
                    "A command check failed in command {} for user {}: {}",
                    ctx.command().name,
                    ctx.author().name,
                    display_error(error),
                );
            } else {
                tracing::error!(
                    "A command check failed in command {} for user {}",
                    ctx.command().name,
                    ctx.author().name,
                );
            }
        },
        crate::FrameworkError::CooldownHit {
            remaining_cooldown,
            ctx,
        } => {
            let msg = format!(
                "You're too fast. Please wait {} seconds before retrying",
                remaining_cooldown.as_secs()
            );
            ctx.send(CreateReply::default().content(msg).ephemeral(true)).await?;
        },
        crate::FrameworkError::MissingBotPermissions {
            missing_permissions,
            ctx,
        } => {
            let msg = format!(
                "Command cannot be executed because the bot is lacking permissions: {missing_permissions}",
            );
            ctx.send(CreateReply::default().content(msg).ephemeral(true)).await?;
        },
        crate::FrameworkError::MissingUserPermissions {
            missing_permissions,
            ctx,
        } => {
            let response = if let Some(missing_permissions) = missing_permissions {
                format!(
                    "You're lacking permissions for `{}{}`: {}",
                    ctx.prefix(),
                    ctx.command().name,
                    missing_permissions,
                )
            } else {
                format!(
                    "You may be lacking permissions for `{}{}`. Not executing for safety",
                    ctx.prefix(),
                    ctx.command().name,
                )
            };
            ctx.send(CreateReply::default().content(response).ephemeral(true)).await?;
        },
        crate::FrameworkError::PermissionFetchFailed { ctx } => {
            ctx.say("An error occurred when fetching permissions.").await?;
        },
        crate::FrameworkError::NotAnOwner { ctx } => {
            let response = "Only bot owners can call this command";
            ctx.send(CreateReply::default().content(response).ephemeral(true)).await?;
        },
        crate::FrameworkError::GuildOnly { ctx } => {
            let response = "You cannot run this command in DMs.";
            ctx.send(CreateReply::default().content(response).ephemeral(true)).await?;
        },
        crate::FrameworkError::DmOnly { ctx } => {
            let response = "You cannot run this command outside DMs.";
            ctx.send(CreateReply::default().content(response).ephemeral(true)).await?;
        },
        crate::FrameworkError::NsfwOnly { ctx } => {
            let response = "You cannot run this command outside NSFW channels.";
            ctx.send(CreateReply::default().content(response).ephemeral(true)).await?;
        },
        crate::FrameworkError::DynamicPrefix { error, msg, .. } => {
            tracing::error!(
                "Dynamic prefix failed for message {:?}: {}",
                msg.content,
                display_error(error)
            );
        },
        crate::FrameworkError::UnknownCommand {
            msg, content_start, ..
        } => {
            let (prefix, msg_content) = msg.content.split_at(content_start.into());
            tracing::warn!(
                "Recognized prefix `{}`, but didn't recognize command name in `{}`",
                prefix,
                msg_content,
            );
        },
        crate::FrameworkError::UnknownInteraction { interaction, .. } => {
            tracing::warn!("received unknown interaction \"{}\"", interaction.data.name);
        },
        crate::FrameworkError::NonCommandMessage { error, .. } => {
            tracing::warn!("error in non-command message handler: {}", display_error(error));
        },
        crate::FrameworkError::__NonExhaustive(unreachable) => match unreachable {},
    }

    Ok(())
}

/// An autocomplete function that can be used for the command parameter in your help function.
///
/// See `examples/feature_showcase` for an example
#[expect(clippy::unused_async)] // Required for the return type
pub async fn autocomplete_command<'a, U: Send + Sync + 'static, E>(
    ctx: crate::Context<'a, U, E>,
    partial: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    let commands = ctx.framework().options.commands.iter();
    let filtered_commands = commands.filter(|cmd| cmd.name.starts_with(partial)).take(25);

    let choices: Vec<_> = filtered_commands
        .map(|cmd| serenity::AutocompleteChoice::from(cmd.name.as_ref()))
        .collect();

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Helper function to display an `impl Into<Box<dyn std::error::Error>>` and its chain of causes.
fn display_error(e: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> impl Display {
    struct DisplayError(Box<dyn std::error::Error + Send + Sync>);

    impl Display for DisplayError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut e: &(dyn std::error::Error + 'static) = &*self.0;

            Display::fmt(e, f)?;

            while let Some(new_e) = e.source() {
                f.write_str(": ")?;
                Display::fmt(new_e, f)?;
                e = new_e;
            }

            Ok(())
        }
    }

    DisplayError(e.into())
}
