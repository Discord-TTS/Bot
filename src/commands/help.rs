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

use indexmap::IndexMap;

use crate::{structs::{Context, Command, CommandResult, PoiseContextExt, ApplicationContext}, macros::require};

enum HelpCommandMode<'a> {
    Root,
    Group(&'a Command),
    Command(&'a Command),
}

fn get_command_mapping(commands: &[Command]) -> IndexMap<&str, Vec<&Command>> {
    let mut mapping = IndexMap::new();

    for command in commands {
        if !command.hide_in_help {
            let commands = mapping
                .entry(command.category.unwrap_or("Uncategoried"))
                .or_insert_with(Vec::new);

            commands.push(command);
        }
    }

    mapping
}

fn format_params(command: &Command) -> String {
    command.parameters.iter().map(|p| {
        if p.required {
            format!("<{}> ", p.name)
        } else {
            format!("[{}] ", p.name)
        }
    }).collect()
}

fn show_group_description(group: &IndexMap<&str, Vec<&Command>>) -> String {
    group.iter().map(|(category, commands)| {
        format!("**__{category}__**\n{}\n", commands.iter().map(|c| {
            let params = format_params(c);
            if params.is_empty() {
                format!("`{}`: {}\n", c.qualified_name, c.inline_help.unwrap())
            } else {
                format!("`{} {}`: {}\n", c.qualified_name, params, c.inline_help.unwrap())
            }
        }).collect::<String>()
    )}).collect::<String>()
}

#[allow(clippy::unused_async)]
async fn help_autocomplete(ctx: ApplicationContext<'_>, searching: String) -> Vec<String> {
    fn flatten_commands(commands: &[Command], searching: &str) -> Vec<String> {
        let mut result = Vec::new();

        for command in commands {
            if command.owners_only || command.hide_in_help {
                continue
            }

            if command.subcommands.is_empty() {
                if command.qualified_name.starts_with(searching) {
                    result.push(command.qualified_name.clone());
                }
            } else {
                result.extend(flatten_commands(&command.subcommands, searching));
            }
        }

        result
    }

    let mut result: Vec<String> = flatten_commands(&ctx.framework.options().commands, &searching);
    result.sort_by_key(|a| strsim::levenshtein(a, &searching));
    result
}

/// Shows TTS Bot's commands and descriptions of them 
#[poise::command(
    prefix_command, slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn help(
    ctx: Context<'_>,
    #[rest] #[description="The command to get help with"] #[autocomplete="help_autocomplete"] command: Option<String>
) -> CommandResult {
    _help(ctx, command.as_deref()).await
}

pub async fn _help(ctx: Context<'_>, command: Option<&str>) -> CommandResult {
    let framework_options = ctx.framework().options();
    let commands = &framework_options.commands;

    let remaining_args: String;
    let mode = match command {
        None => HelpCommandMode::Root,
        Some(command) => {
            let mut subcommand_iterator = command.split(' ');

            let top_level_command = subcommand_iterator.next().unwrap();
            let (mut command_obj, _, _) = require!(poise::find_command(commands, top_level_command, true), {
                ctx.say(ctx.gettext("No command called {} found!").replace("{}", top_level_command)).await?;
                Ok(())
            });

            remaining_args = subcommand_iterator.collect();
            if !remaining_args.is_empty() {
                (command_obj, _, _) = require!(poise::find_command(&command_obj.subcommands, &remaining_args, true), {
                    ctx.say(ctx
                        .gettext("The group {group_name} does not have a subcommand called {subcommand_name}!")
                        .replace("{subcommand_name}", &remaining_args).replace("{group_name}", command_obj.name)
                    ).await.map(drop).map_err(Into::into)
                });
            };

            if command_obj.owners_only && !framework_options.owners.contains(&ctx.author().id) {
                ctx.say(ctx.gettext("This command is only available to the bot owner!")).await?;
                return Ok(())
            }

            if command_obj.subcommands.is_empty() {
                HelpCommandMode::Command(command_obj)
            } else {
                HelpCommandMode::Group(command_obj)
            }
        }
    };

    let neutral_colour = ctx.neutral_colour().await;
    ctx.send(|b| {b.embed(|e| {e
        .title(ctx.gettext("{command_name} Help!").replace("{command_name}", &match &mode {
            HelpCommandMode::Root => ctx.discord().cache.current_user_field(|u| u.name.clone()),
            HelpCommandMode::Group(c) | HelpCommandMode::Command(c) => format!("`{}`", c.qualified_name) 
        }))
        .description(match &mode {
            HelpCommandMode::Root => show_group_description(&get_command_mapping(commands)),
            HelpCommandMode::Command(command_obj) => {
                let mut msg = format!("{}\n```/{} {}```\n",
                    command_obj.inline_help.unwrap_or_else(|| ctx.gettext("Command description not found!")),
                    command_obj.qualified_name, format_params(command_obj),
                );

                if !command_obj.parameters.is_empty() {
                    msg.push_str(ctx.gettext("__**Parameter Descriptions**__\n"));
                    msg.push_str(&command_obj.parameters.iter().map(|p|
                        format!("`{}`: {}\n", p.name, p.description.unwrap_or_else(|| ctx.gettext("no description")))
                    ).collect::<String>());
                };

                msg
            },
            HelpCommandMode::Group(group) => show_group_description(&{
                let mut map: IndexMap<&str, Vec<&Command>> = IndexMap::new();
                map.insert(&group.qualified_name, group.subcommands.iter().collect());
                map
            }),
        })
        .colour(neutral_colour)
        .author(|a| {
            a.name(ctx.author().name.clone());
            a.icon_url(ctx.author().face())
        })
        .footer(|f| f.text(match mode {
            HelpCommandMode::Group(c) => ctx
                .gettext("Use `/help {command_name} [command]` for more info on a command")
                .replace("{command_name}", &c.qualified_name),
            HelpCommandMode::Command(_) |HelpCommandMode::Root => ctx
                .gettext("Use `/help [command]` for more info on a command")
                .to_string()
        }))
    })}).await?;

    Ok(())
}
