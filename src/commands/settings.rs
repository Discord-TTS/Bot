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

use std::collections::BTreeMap;

use poise::serenity_prelude as serenity;

use crate::constants::*;
use crate::random_footer;

/// Displays the current settings!
#[poise::command(category="Settings", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")]
pub async fn settings(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();
    let author_id = ctx.author().id.into();

    let data = ctx.data();
    let ctx_discord = ctx.discord();

    let guild_row = data.guilds_db.get(guild_id).await?;
    let userinfo_row = data.userinfo_db.get(author_id).await?;
    let nickname_row = data.nickname_db.get([guild_id, author_id]).await?;

    let channel_name = guild_row.get::<&str, Option<i64>>("channel")
        .and_then(|c| 
            ctx_discord.cache.guild_channel_field(c as u64, |c| c.name.clone())
        )
        .unwrap_or_else(|| String::from("has not been set up yet"));

    let xsaid: bool = guild_row.get("xsaid");
    let prefix: String = guild_row.get("prefix");
    let autojoin: bool = guild_row.get("auto_join");
    let msg_length: i16 = guild_row.get("msg_length");
    let bot_ignore: bool = guild_row.get("bot_ignore");
    let repeated_chars: i16 = guild_row.get("repeated_chars");
    let audience_ignore: bool = guild_row.get("audience_ignore");

    let nickname = nickname_row.get::<&str, Option<String>>("name").unwrap_or_else(|| String::from("none"));
    let user_lang = userinfo_row.get::<&str, Option<String>>("lang").unwrap_or_else(|| String::from("none"));
    let default_lang = guild_row.get::<&str, Option<String>>("default_lang").unwrap_or_else(|| String::from("none"));

    let [sep1, sep2, sep3] = OPTION_SEPERATORS;
    ctx.send(|b| {b.embed(|e| {
        e.title("Current Settings");
        e.url(&data.config.server_invite);
        e.colour(NETURAL_COLOUR);
        e.footer(|f| {
            f.text(format!(concat!(
                "Change these settings with {prefix}set property value!\n",
                "None = setting has not been set yet!"
            ), prefix=prefix))
        });

        e.field("**General Server Settings**", format!("
{sep1} Setup Channel: `#{channel_name}`
{sep1} Command Prefix: `{prefix}`
{sep1} Auto Join: `{autojoin}`
        "), false);
        e.field("**TTS Settings**", format!("
{sep2} <User> said: message `{xsaid}`
{sep2} Ignore bot's messages: `{bot_ignore}`
{sep2} Default Server Language: `{default_lang}`
{sep2} Ignore audience' messages: `{audience_ignore}`

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        "), false);
        e.field("**User Specific**", format!("
{sep3} Language: `{user_lang}`
{sep3} Nickname: `{nickname}`
        "), false)
    })}).await?;

    Ok(())
}

fn get_supported_languages() -> BTreeMap<String, String> {
    let raw_json = std::include_str!("../data/langs.json");
    serenity::json::prelude::from_str(raw_json).unwrap()
}

fn to_enabled(value: bool) -> &'static str {
    if value {
        "Enabled"
    } else {
        "Disabled"
    }
}

async fn bool_button(ctx: Context<'_>, value: Option<bool>) -> Result<bool, Error> {
    Ok(
        match value {
            Some(value) => value,
            None => {
                let message = ctx.send(|b| {
                    b.content("What would you like to set this to?");
                    b.components(|c| {
                        c.create_action_row(|r| {
                            r.create_button(|b| {
                                b.style(serenity::ButtonStyle::Success);
                                b.custom_id("True");
                                b.label("True")
                            });
                            r.create_button(|b| {
                                b.style(serenity::ButtonStyle::Danger);
                                b.custom_id("False");
                                b.label("False")
                            })
                        })
                    })
                }).await?.unwrap().message().await?;

                let ctx_discord = ctx.discord();
                let interaction = message
                    .await_component_interaction(&ctx_discord.shard).collect_limit(1)
                    .await.unwrap();

                interaction.defer(&ctx_discord.http).await?;
                match &*interaction.data.custom_id {
                    "True" => true,
                    "False" => false,
                    _ => unreachable!()
                }
            }
        }
    )
}

/// Changes a setting!
#[poise::command(category="Settings", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")]
pub async fn set(ctx: Context<'_>, ) -> Result<(), Error> {
    crate::commands::help::_help(ctx, Some(String::from("set"))).await
}

/// Owner only: used to block a user from dms
#[poise::command(
    category="Settings",
    owners_only,
    prefix_command, hide_in_help,
    required_bot_permissions="SEND_MESSAGES"
)]
pub async fn block(
    ctx: Context<'_>,
    user: serenity::UserId,
    value: bool
) -> Result<(), Error> {
    ctx.data().userinfo_db.set_one(user.into(), "dm_blocked", &value).await?;
    ctx.say("Done!").await?;
    Ok(())
}

/// Makes the bot say "<user> said" before each message
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES"
)]
pub async fn xsaid(
    ctx: Context<'_>,
    #[description="Whether to say \"<user> said\" before each message"] value: Option<bool>
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let value = bool_button(ctx, value).await?;
    ctx.data().guilds_db.set_one(guild_id, "xsaid", &value).await?;
    ctx.say(format!("xsaid is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Makes the bot join the voice channel automatically when a message is sent in the setup channel
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("auto_join")
)]
pub async fn autojoin(
    ctx: Context<'_>,
    #[description="Whether to autojoin voice channels"] value: Option<bool>
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let value = bool_button(ctx, value).await?;
    ctx.data().guilds_db.set_one(guild_id, "auto_join", &value).await?;
    ctx.say(format!("Auto Join is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Makes the bot ignore messages sent by bots and webhooks 
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("bot_ignore", "ignore_bots", "ignorebots")
)]
pub async fn botignore(
    ctx: Context<'_>,
    #[description="Whether to ignore messages sent by bots and webhooks"] value: Option<bool>
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let value = bool_button(ctx, value).await?;
    ctx.data().guilds_db.set_one(guild_id, "bot_ignore", &value).await?;
    ctx.say(format!("Ignoring bots is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Changes the language your messages are read in, full list in `-voices`
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("lang", "voice")
)]
pub async fn language(
    ctx: Context<'_>,
    #[description="The language to read messages in"] lang: String
) -> Result<(), Error> {
    let to_send = match get_supported_languages().get(&lang) {
        Some(lang_name) => {
            ctx.data().userinfo_db.set_one(ctx.author().id.into(), "lang", &lang).await?;
            format!("Changed your language to: {}", lang_name)
        },
        None => format!("Invalid language, do `{}languages`", ctx.prefix())
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the default language messages are read in
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("defaultlang", "default_lang", "defaultlang", "slang", "serverlanguage")
)]
pub async fn server_language(
    ctx: Context<'_>,
    #[description="The default languages to read messages in"] language: String
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let to_send = if get_supported_languages().contains_key(&language) {
        ctx.data().guilds_db.set_one(guild_id, "default_lang", &language).await?;
        format!("Default language for this server is now: {}", language)
    } else {
        format!("**Error**: Invalid language, do `{}voices`", ctx.prefix())
    };

    ctx.say(to_send).await?;
    Ok(())
}


/// Changes the prefix used before commands
#[poise::command(
    category="Settings",
    prefix_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn prefix(
    ctx: Context<'_>,
    #[description="The prefix to be used before commands"] #[rest] prefix: String
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let to_send = if prefix.len() <= 5 && prefix.matches(' ').count() <= 1 {
        ctx.data().guilds_db.set_one(guild_id, "prefix", &prefix).await?;
        format!("Command prefix for this server is now: {}", prefix)
    } else {
        String::from("**Error**: Invalid Prefix, please use 5 or less characters with maximum 1 space")
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Max repetion of a character (0 = off)
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("repeated_chars", "repeated_letters", "chars")
)]
pub async fn repeated_characters(ctx: Context<'_>, #[description="The max message time to read (in seconds)"] chars: u8) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let to_send = {
        if chars > 100 {
            String::from("**Error**: Cannot set the max repeated characters above 60 seconds")
        } else if chars < 20 {
            String::from("**Error**: Cannot set the max repeated characters below 20 seconds")
        } else {
            ctx.data().guilds_db.set_one(guild_id, "msg_length", &(chars as i16)).await?;
            format!("Max repeated characters is now: {chars}")
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Makes the bot ignore messages sent by members of the audience in stage channels
#[poise::command(
category="Settings",
prefix_command, slash_command,
required_permissions="ADMINISTRATOR",
required_bot_permissions="SEND_MESSAGES",
aliases("audience_ignore", "ignore_audience", "ignoreaudience")
)]
pub async fn audienceignore(
    ctx: Context<'_>,
    #[description="Whether to ignore messages sent by the audience"] value: Option<bool>
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let value = bool_button(ctx, value).await?;
    ctx.data().guilds_db.set_one(guild_id, "audience_ignore", &value).await?;
    ctx.say(format!("Ignoring audience is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Replaces your username in "<user> said" with a given name
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
    aliases("nick_name", "nickname", "name"),
)]
pub async fn nick(
    ctx: Context<'_>,
    #[description="The user to set the nick for, defaults to you"] user: Option<serenity::User>,
    #[description="The nickname to set"] #[rest] nickname: String
) -> Result<(), Error> {
    let ctx_discord = ctx.discord();
    let guild = ctx.guild().ok_or(Error::GuildOnly)?;

    let author = ctx.author();
    let user = user.unwrap_or_else(|| author.clone());

    if author.id != user.id && !guild.member(ctx_discord, author).await?.permissions(ctx_discord)?.administrator() {
        ctx.say("**Error**: You need admin to set other people's nicknames!").await?;
        return Ok(())
    }
    
    let to_send = if nickname.contains('<') && nickname.contains('>') {
        String::from("**Error**: You can't have mentions/emotes in your nickname!")
    } else {
        let data = ctx.data();
        let (r1, r2) = tokio::join!(
            data.guilds_db.create_row(guild.id.into()),
            data.userinfo_db.create_row(user.id.into())
        ); r1?; r2?;

        data.nickname_db.set_one([guild.id.into(), user.id.into()], "name", &nickname).await?;
        format!("Changed {}'s nickname to {}", user.name, nickname)
    };

    ctx.say(to_send).await?;
    Ok(())
}


fn can_send(guild: &serenity::Guild, channel: &serenity::GuildChannel, member: &serenity::Member) -> bool {
    const REQUIRED_PERMISSIONS: serenity::Permissions = serenity::Permissions::from_bits_truncate(
        serenity::Permissions::SEND_MESSAGES.bits() | serenity::Permissions::READ_MESSAGES.bits()
    );

    guild.user_permissions_in(channel, member)
        .map(|p| (REQUIRED_PERMISSIONS - p).is_empty())
        .unwrap_or(false)
}


/// Setup the bot to read messages from the given channel
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn setup(
    ctx: Context<'_>,
    #[description="The channel for the bot to read messages from"] #[channel_types("Text")]
    channel: Option<serenity::GuildChannel>
) -> Result<(), Error> {
    let guild = ctx.guild().ok_or(Error::GuildOnly)?;

    let ctx_discord = ctx.discord();
    let cache = &ctx_discord.cache;

    let author = ctx.author();
    let bot_user = cache.current_user();

    let channel: u64 = match channel {
        Some(channel) => channel.id.into(),
        None => {
            let author_member = guild.member(ctx_discord, author).await?;
            let bot_member = guild.member(ctx_discord, bot_user.id).await?;

            let mut text_channels: Vec<&serenity::GuildChannel> = guild.channels.values()
                .filter_map(|c| {match c {
                    serenity::Channel::Guild(channel) => Some(channel),
                    _ => None
                }})
                .filter(|c| {
                    c.kind == serenity::ChannelType::Text &&
                    can_send(&guild, c, &author_member) &&
                    can_send(&guild, c, &bot_member)
                })
                .collect();

            if text_channels.is_empty() {
                ctx.say("**Error** This server doesn't have any text channels that we both have Read/Send Messages in!").await?;
                return Ok(())
            };

            text_channels.sort_by(|f, s| Ord::cmp(&f.position, &s.position));

            let message = ctx.send(|b| {
                b.content("Select a channel!");
                b.components(|c| {
                    for chunked_channels in text_channels.chunks(25) {
                        c.create_action_row(|r| {
                            r.create_select_menu(|s| {
                                s.custom_id("select::channel");
                                s.options(|os| {
                                    for channel in chunked_channels {
                                        os.create_option(|o| {
                                            o.label(&channel.name);
                                            o.value(channel.id)
                                        });
                                    };
                                    os
                                });
                                s
                            });
                            r
                        });
                    };
                    c
                })
            }).await?.unwrap().message().await?;

            let interaction = message
                .await_component_interaction(&ctx_discord.shard).collect_limit(1)
                .await.unwrap();

            interaction.defer(&ctx_discord.http).await?;
            interaction.data.values[0].parse().unwrap()
        }
    };

    let data = ctx.data();
    data.guilds_db.set_one(guild.id.into(), "channel", &(channel as i64)).await?;
    ctx.send(|b| b.embed(|e| {
        e.title(format!("{} has been setup!", bot_user.name));
        e.thumbnail(bot_user.face());
        e.description(format!("
TTS Bot will now accept commands and read from <#{channel}>.
Just do `{}join` and start talking!
        ", ctx.prefix()));

        e.footer(|f| {f.text(random_footer(
            Some(&String::from(ctx.prefix())),
            Some(&data.config.server_invite),
            Some(cache.current_user_id().0)
        ))});
        e.author(|a| {
            a.name(&author.name);
            a.icon_url(author.face())
        })
    })).await?;

    Ok(())
}

/// Lists all the language codes that TTS bot accepts
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn languages(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();

    let supported_langs = get_supported_languages();
    let current_lang: Option<String> = ctx.data().userinfo_db.get(author.id.into()).await?.get("lang");

    let langs_string = supported_langs.keys().map(|l| format!("`{l}`, ")).collect::<String>();

    let cache = &ctx.discord().cache;
    ctx.send(|b| {b.embed(|e| {
        e.title(format!("{} Languages", cache.current_user_field(|u| u.name.clone())));
        e.footer(|f| f.text(random_footer(
            Some(&String::from(ctx.prefix())),
            Some(&ctx.data().config.server_invite),
            Some(cache.current_user_id().0)
        )));
        e.author(|a| {
            a.name(author.name.clone());
            a.icon_url(author.face())
        });
        e.field(
            "Currently Supported Languages", 
            langs_string.strip_suffix(", ").unwrap_or(&langs_string),
            true
        );
        e.field(
            "Current Language used",
            current_lang.as_ref()
                .map(|l| format!("{} | {}", &supported_langs[l], l))
                .unwrap_or_else(|| String::from("None")),
            false
        )
    })}).await?;

    Ok(())
}
