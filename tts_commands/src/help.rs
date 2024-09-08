use std::{borrow::Cow, fmt::Write};

use indexmap::IndexMap;

use self::serenity::CreateEmbed;
use poise::serenity_prelude as serenity;

use tts_core::{
    require,
    structs::{ApplicationContext, Command, CommandResult, Context},
    traits::PoiseContextExt,
};

enum HelpCommandMode<'a> {
    Root,
    Group(&'a Command),
    Command(&'a Command),
}

fn get_command_mapping(commands: &[Command]) -> IndexMap<&str, Vec<&Command>> {
    let mut mapping = IndexMap::new();

    for command in commands {
        if !command.hide_in_help {
            let category = command.category.as_deref().unwrap_or("Uncategoried");
            mapping
                .entry(category)
                .or_insert_with(Vec::new)
                .push(command);
        }
    }

    mapping
}

fn format_params(buf: &mut String, command: &Command) {
    for p in &command.parameters {
        let name = &p.name;
        if p.required {
            write!(buf, " <{name}>").unwrap();
        } else {
            write!(buf, " [{name}]").unwrap();
        }
    }
}

fn show_group_description(group: &IndexMap<&str, Vec<&Command>>) -> String {
    let mut buf = String::with_capacity(group.len()); // Major underestimation, but it's better than nothing
    for (category, commands) in group {
        writeln!(buf, "**__{category}__**").unwrap();
        for c in commands {
            let name = &c.qualified_name;
            let description = c.description.as_deref().unwrap_or("no description");

            write!(buf, "`/{name}").unwrap();
            format_params(&mut buf, c);
            writeln!(buf, "`: {description}").unwrap();
        }
    }

    buf
}

#[allow(clippy::unused_async)]
pub async fn autocomplete(ctx: ApplicationContext<'_>, searching: &str) -> Vec<String> {
    fn flatten_commands(result: &mut Vec<String>, commands: &[Command], searching: &str) {
        for command in commands {
            if command.owners_only || command.hide_in_help {
                continue;
            }

            if command.subcommands.is_empty() {
                if command.qualified_name.starts_with(searching) {
                    result.push(command.qualified_name.clone());
                }
            } else {
                flatten_commands(result, &command.subcommands, searching);
            }
        }
    }

    let commands = &ctx.framework.options().commands;
    let mut result = Vec::with_capacity(commands.len());

    flatten_commands(&mut result, commands, searching);

    result.sort_by_key(|a| strsim::levenshtein(a, searching));
    result
}

/// Shows TTS Bot's commands and descriptions of them
#[poise::command(
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
async fn help(
    ctx: Context<'_>,
    #[rest]
    #[description = "The command to get help with"]
    #[autocomplete = "autocomplete"]
    command: Option<String>,
) -> CommandResult {
    command_func(ctx, command.as_deref()).await
}

pub async fn command_func(ctx: Context<'_>, command: Option<&str>) -> CommandResult {
    let framework_options = ctx.framework().options();
    let commands = &framework_options.commands;

    let remaining_args: String;
    let mode = match command {
        None => HelpCommandMode::Root,
        Some(command) => {
            let mut subcommand_iterator = command.split(' ');

            let top_level_command = subcommand_iterator.next().unwrap();
            let Some((mut command_obj, _, _)) =
                poise::find_command(commands, top_level_command, true, &mut Vec::new())
            else {
                let msg = format!("No command called {top_level_command} found!");
                ctx.say(msg).await?;
                return Ok(());
            };

            remaining_args = subcommand_iterator.collect();
            if !remaining_args.is_empty() {
                (command_obj, _, _) = require!(
                    poise::find_command(
                        &command_obj.subcommands,
                        &remaining_args,
                        true,
                        &mut Vec::new()
                    ),
                    {
                        let group_name = &command_obj.name;
                        let msg = format!("The group {group_name} does not have a subcommand called {remaining_args}!");

                        ctx.say(msg).await?;
                        Ok(())
                    }
                );
            };

            if command_obj.owners_only && !framework_options.owners.contains(&ctx.author().id) {
                ctx.say("This command is only available to the bot owner!")
                    .await?;
                return Ok(());
            }

            if command_obj.subcommands.is_empty() {
                HelpCommandMode::Command(command_obj)
            } else {
                HelpCommandMode::Group(command_obj)
            }
        }
    };

    let neutral_colour = ctx.neutral_colour().await;
    let embed = CreateEmbed::default()
        .title(match mode {
            HelpCommandMode::Root => format!("{} Help!", ctx.cache().current_user().name),
            HelpCommandMode::Group(c) | HelpCommandMode::Command(c) => {
                format!("{} Help!", c.qualified_name)
            }
        })
        .description(match &mode {
            HelpCommandMode::Root => show_group_description(&get_command_mapping(commands)),
            HelpCommandMode::Command(command_obj) => {
                let mut msg = format!(
                    "{}\n```/{}",
                    command_obj
                        .description
                        .as_deref()
                        .unwrap_or("Command description not found!"),
                    command_obj.qualified_name
                );

                format_params(&mut msg, command_obj);
                msg.push_str("```\n");

                if !command_obj.parameters.is_empty() {
                    msg.push_str("__**Parameter Descriptions**__\n");
                    command_obj.parameters.iter().for_each(|p| {
                        let name = &p.name;
                        let description = p.description.as_deref().unwrap_or("no description");
                        writeln!(msg, "`{name}`: {description}").unwrap();
                    });
                };

                msg
            }
            HelpCommandMode::Group(group) => show_group_description(&{
                let mut map = IndexMap::new();
                map.insert(
                    group.qualified_name.as_ref(),
                    group.subcommands.iter().collect(),
                );
                map
            }),
        })
        .colour(neutral_colour)
        .author(
            serenity::CreateEmbedAuthor::new(ctx.author().name.as_str())
                .icon_url(ctx.author().face()),
        )
        .footer(serenity::CreateEmbedFooter::new(match mode {
            HelpCommandMode::Group(c) => Cow::Owned(format!(
                "Use `/help {} [command]` for more info on a command",
                c.qualified_name
            )),
            HelpCommandMode::Command(_) | HelpCommandMode::Root => {
                Cow::Borrowed("Use `/help [command]` for more info on a command")
            }
        }));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

// /set calls /help set
pub use command_func as command;
pub fn commands() -> [Command; 1] {
    [help()]
}
