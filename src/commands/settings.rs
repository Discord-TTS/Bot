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

use poise::serenity_prelude as serenity;

use crate::constants::{NETURAL_COLOUR, OPTION_SEPERATORS};
use crate::structs::{Context, Error};
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

    #[cfg(feature="premium")] let voice_lang = "Voice";
    #[cfg(not(feature="premium"))] let voice_lang = "Language";

    let format_voice = |voice: String| {
        #[cfg(feature="premium")] {
            let (lang, variant) = voice.split_once(' ').unwrap();
            let gender = &ctx.data().voices[lang][variant];
            format!("{lang} - {variant} ({gender})")
        } #[cfg(not(feature="premium"))] {
            voice
        }
    };

    let xsaid: bool = guild_row.get("xsaid");
    let prefix: String = guild_row.get("prefix");
    let autojoin: bool = guild_row.get("auto_join");
    let msg_length: i16 = guild_row.get("msg_length");
    let bot_ignore: bool = guild_row.get("bot_ignore");
    let repeated_chars: i16 = guild_row.get("repeated_chars");
    let audience_ignore: bool = guild_row.get("audience_ignore");

    let none = String::from("none");
    let nickname = nickname_row.get::<&str, Option<String>>("name").unwrap_or_else(|| none.clone());

    let user_voice = userinfo_row.get::<&str, Option<String>>("voice").map(format_voice).unwrap_or_else(|| none.clone());
    let default_voice = guild_row.get::<&str, Option<String>>("default_voice").map(format_voice).unwrap_or_else(|| none.clone());

    let [sep1, sep2, sep3, sep4] = OPTION_SEPERATORS;
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
{sep2} Default Server {voice_lang}: `{default_voice}`
{sep2} Ignore audience' messages: `{audience_ignore}`

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        "), false);
        if cfg!(feature="premium") {
            let to_translate: bool = guild_row.get("to_translate");
            let target_lang = guild_row.get::<&str, Option<String>>("target_lang").unwrap_or(none);
            e.field("**Translation Settings**", format!("
{sep4} Translation: `{to_translate}`
{sep4} Translation Language: `{target_lang}`
            "), false);
        }
        e.field("**User Specific**", format!("
{sep3} {voice_lang}: `{user_voice}`
{sep3} Nickname: `{nickname}`
        "), false)
    })}).await?;

    Ok(())
}


#[cfg(feature="premium")]
struct MenuPaginator<'a> {
    index: usize,
    ctx: Context<'a>,
    pages: Vec<String>,
    current_lang: String,
}

#[cfg(feature="premium")]
impl<'a> MenuPaginator<'a> {
    pub fn new(ctx: Context<'a>, pages: Vec<String>, current_lang: String) -> Self {
        Self {
            ctx,
            pages,
            current_lang,
            index: 0,
        }
    }

    
    fn create_page<'b>(&self, embed: &'b mut serenity::CreateEmbed, page: &str) -> &'b mut serenity::CreateEmbed {
        let author = self.ctx.author();
        let ctx_discord = self.ctx.discord();
        let cache = &ctx_discord.cache;
        let (bot_id, bot_name) = cache.current_user_field(|u| (u.id, u.name.clone()));

        embed.title(format!("{bot_name} Languages"));
        embed.description(format!("**Currently Supported Languages**\n{page}"));
        embed.field("Current Language used", self.current_lang.clone(), false);
        embed.author(|a| {
            a.name(author.name.clone());
            a.icon_url(author.face())
        });
        embed.footer(|f| {f.text(random_footer(
            Some(self.ctx.prefix()),
            Some(&self.ctx.data().config.server_invite),
            Some(bot_id.into())
        ))})
    }

    fn create_action_row<'b>(&self, builder: &'b mut serenity::CreateActionRow, disabled: bool) -> &'b mut serenity::CreateActionRow {
        for emoji in ["⏮️", "◀", "⏹️", "▶️", "⏭️"] {
            builder.create_button(|b| {
                b.custom_id(emoji);
                b.style(serenity::ButtonStyle::Primary);
                b.emoji(serenity::ReactionType::Unicode(String::from(emoji)));
                b.disabled(
                    disabled ||
                    (["⏮️", "◀"].contains(&emoji) && self.index == 0) ||
                    (["▶️", "⏭️"].contains(&emoji) && self.index == (self.pages.len() - 1))
                )
            });
        };
        builder
    }

    async fn create_message(&self) -> Result<serenity::Message, Error> {
        let message = self.ctx.channel_id().send_message(&self.ctx.discord().http, |b| {
            b.embed(|e| self.create_page(e, &self.pages[self.index]));
            b.components(|c| c.create_action_row(|r| self.create_action_row(r, false)))
        }).await?;

        Ok(message)
    }

    async fn edit_message(&self, message: &mut serenity::Message, disable: bool) -> Result<(), Error> {
        message.edit(self.ctx.discord(), |b| {
            b.embed(|e| self.create_page(e, &self.pages[self.index]));
            b.components(|c| c.create_action_row(|r| self.create_action_row(r, disable)))
        }).await?;

        Ok(())
    }


    pub async fn start(mut self) -> Result<(), Error> {
        let ctx_discord = self.ctx.discord();
        let mut message = self.create_message().await?;

        loop {
            let collector = message.await_component_interaction(&ctx_discord.shard)
                .author_id(self.ctx.author().id)
                .collect_limit(1)
                ;
            let interaction = match collector.await {
                Some(interaction) => interaction,
                None => break
            };
            
            let data = &interaction.data;
            match &data.custom_id[..] {
                "⏮️" => {
                    self.index = 0;
                    self.edit_message(&mut message, false).await?;
                },
                "◀" => {
                    self.index -= 1;
                    self.edit_message(&mut message, false).await?;
                },
                "⏹️" => {
                    self.edit_message(&mut message, true).await?;
                    interaction.defer(&ctx_discord.http).await?;
                    break
                },
                "▶️" => {
                    self.index += 1;
                    self.edit_message(&mut message, false).await?;
                },
                "⏭️" => {
                    self.index = self.pages.len() - 1;
                    self.edit_message(&mut message, false).await?;
                },
                _ => unreachable!()
            };
            interaction.defer(&self.ctx.discord().http).await?;
        }
        Ok(())
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
                    .await_component_interaction(&ctx_discord.shard)
                    .author_id(ctx.author().id)
                    .collect_limit(1)
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

fn to_enabled(value: bool) -> &'static str {
    if value {
        "Enabled"
    } else {
        "Disabled"
    }
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
/// Whether to use deepL translate to translate all TTS messages to the same language 
#[cfg(feature="premium")]
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("translate", "to_translate", "should_translate")
)]
pub async fn translation(ctx: Context<'_>, #[description="Whether to translate all messages to the same language"] value: Option<bool>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let value = bool_button(ctx, value).await?;
    ctx.data().guilds_db.set_one(guild_id, "to_translate", &value).await?;
    ctx.say(format!("Translation is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Changes the default language messages are read in
#[cfg(not(feature="premium"))]
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

    let to_send = if crate::funcs::get_supported_languages().contains_key(&language) {
        ctx.data().guilds_db.set_one(guild_id, "default_voice", &language).await?;
        format!("Default language for this server is now: {}", language)
    } else {
        format!("**Error**: Invalid language, do `{}voices`", ctx.prefix())
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the default language messages are read in
#[cfg(feature="premium")]
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("server_language", "defaultlang", "default_lang", "defaultlang", "slang", "serverlanguage")
)]
pub async fn server_voice(
    ctx: Context<'_>,
    #[description="The default language to read messages in"] mut language: String,
    #[description="The default variant of this language to use"] mut variant: Option<String>
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    variant = variant.map(|s| s.to_uppercase());
    if let Some((lang, accent)) = language.split_once('-') {
        language = format!("{}-{}", lang, accent.to_uppercase());
    }

    let data = ctx.data();
    if let Some((variant, gender)) = get_voice(&ctx, &data.voices, &language, variant.as_ref()).await? {
        ctx.data().guilds_db.set_one(guild_id, "default_voice", &format!("{} {}", language, variant)).await?;
        ctx.say(format!("Changed the server default voice to: {language} - {variant} ({gender})")).await?;
    }

    Ok(())
}

#[cfg(feature="premium")]
async fn get_translation_langs(reqwest: &reqwest::Client, token: &str) -> Result<Vec<String>, Error> {
    Ok(
        reqwest
            .get(format!("{}/languages", crate::constants::TRANSLATION_URL))
            .query(&serenity::json::prelude::json!({
                "type": "target",
                "auth_key": token
            }))
            .send().await?
            .error_for_status()?
            .json::<Vec<crate::structs::DeeplVoice>>().await?
            .iter().map(|v| v.language.to_lowercase()).collect()
    )
}

/// Changes the target language for translation
#[cfg(feature="premium")]
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("tlang", "tvoice", "target_lang", "target_voice", "target_language")
)]
pub async fn translation_lang(
    ctx: Context<'_>,
    #[description="The language to translate all TTS messages to"] lang: Option<String>
) -> Result<(), Error> {
    use itertools::Itertools;

    let data = ctx.data();
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let translation_langs = get_translation_langs(&data.reqwest, &data.config.translation_token).await?;
    match lang {
        Some(lang) if translation_langs.contains(&lang) => {
            data.guilds_db.set_one(guild_id, "target_lang", &lang).await?;
            ctx.say(format!(
                "The target translation language is now: {lang}{}",
                if !data.guilds_db.get(guild_id).await?.get::<&str, bool>("to_translate") {
                    format!(". You may want to enable translation with `{}set translation on`", ctx.prefix())
                } else {
                    String::new()
                }
            )).await?;
        },
        _ => {
            ctx.send(|b| b.embed(|e| {
                e.title("DeepL Translation - Supported languages");
                e.description(format!("```{}```", translation_langs.iter().join(", ")))
            })).await?;
        }
    }

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

/// Changes the max repetion of a character (0 = off)
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("repeated_chars", "repeated_letters", "chars")
)]
pub async fn repeated_characters(ctx: Context<'_>, #[description="The max repeated characters"] chars: u8) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let to_send = {
        if chars > 100 {
            String::from("**Error**: Cannot set the max repeated characters above 100")
        } else if chars < 20 && chars != 0 {
            String::from("**Error**: Cannot set the max repeated characters below 5")
        } else {
            ctx.data().guilds_db.set_one(guild_id, "repeated_chars", &(chars as i16)).await?;
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

/// Changes the multiplier for how fast to speak
#[cfg(feature="premium")]
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
    aliases("speed", "speed_multiplier", "speaking_rate_multiplier", "speaking_speed", "tts_speed")
)]
pub async fn speaking_rate(
    ctx: Context<'_>,
    #[description="The speed to speak at (0.25-4.0)"] #[min=0.25] #[max=4.0] multiplier: f32
) -> Result<(), Error> {
    let to_send = {
        if multiplier > 4.0 {
            String::from("**Error**: Cannot set the speaking rate multiplier above 4x")
        } else if multiplier < 0.25 {
            String::from("**Error**: Cannot set the speaking rate multiplier below 0.25x")
        } else {
            ctx.data().userinfo_db.set_one(ctx.author().id.into(), "speaking_rate", &multiplier).await?;
            format!("The speaking rate multiplier is now: {multiplier}")
        }
    };

    ctx.say(to_send).await?;
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
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
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

            let interaction = message.await_component_interaction(&ctx_discord.shard)
                .author_id(ctx.author().id)
                .collect_limit(1)
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

/// Changes the language your messages are read in, full list in `-voices`
#[cfg(not(feature="premium"))]
#[poise::command(
    category="Settings",
    aliases("lang", "voice"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn language(
    ctx: Context<'_>,
    #[description="The language to read messages in"] lang: String
) -> Result<(), Error> {
    let to_send = match crate::funcs::get_supported_languages().get(&lang) {
        Some(lang_name) => {
            ctx.data().userinfo_db.set_one(ctx.author().id.into(), "voice", &lang).await?;
            format!("Changed your language to: {}", lang_name)
        },
        None => format!("Invalid language, do `{}languages`", ctx.prefix())
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Lists all the language codes that TTS bot accepts
#[cfg(not(feature="premium"))]
#[poise::command(
    category="Settings",
    aliases("langs", "voices"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn languages(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();

    let supported_langs = crate::funcs::get_supported_languages();
    let current_lang: Option<String> = ctx.data().userinfo_db.get(author.id.into()).await?.get("voice");

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


#[cfg(feature="premium")]
async fn get_voice<'a>(
    ctx: &'a Context<'a>,
    voices: &'a crate::structs::VoiceData,
    language: &'a str, variant: Option<&'a String>
) -> Result<Option<(&'a String, &'a crate::structs::Gender)>, Error> {
    let voice = voices.get(language).and_then(|variants| match variant {
        Some(variant) => variants.get(variant).map(|g| (variant, g)),
        None => variants.iter().next()
    });

    if voice.is_none() {
        let author = ctx.author();
        let none = String::from("None");
        ctx.send(|b| {b.embed(|e| {
            e.title(format!("Cannot find voice `{language} - {}`", variant.unwrap_or(&none)));
            e.footer(|f| f.text(format!("Try {}voices for a full list!", ctx.prefix())));
            e.author(|a| {
                a.name(format!("{}#{}", author.name, author.discriminator));
                a.icon_url(author.face())
            })
        })}).await?;
    }
    Ok(voice)
}

/// Changes the voice your messages are read in, full list in `p-voices`
#[cfg(feature="premium")]
#[poise::command(
    category="Settings",
    aliases("lang", "languages"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn voice(
    ctx: Context<'_>,
    #[description="The language to read messages in"] mut language: String,
    #[description="The variant of this language to use"] mut variant: Option<String>
) -> Result<(), Error> {
    variant = variant.map(|s| s.to_uppercase());
    if let Some((lang, accent)) = language.split_once('-') {
        language = format!("{}-{}", lang, accent.to_uppercase());
    }

    let data = ctx.data();
    if let Some((variant, gender)) = get_voice(&ctx, &data.voices, &language, variant.as_ref()).await? {
        data.userinfo_db.set_one(ctx.author().id.into(), "voice", &format!("{} {}", language, variant)).await?;
        ctx.say(format!("Changed your voice to: {language} - {variant} ({gender})")).await?;
    };

    Ok(())
}

/// Lists all the voices that TTS Bot Premium accepts
#[cfg(feature="premium")]
#[poise::command(
    category="Settings",
    aliases("langs", "languages"),
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn voices(ctx: Context<'_>) -> Result<(), Error> {
    let http = &ctx.discord().http; 
    if let poise::Context::Application(ctx) = ctx {
        if let poise::ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction) = ctx.interaction {
            interaction.create_interaction_response(http, |b| {
                b.kind(serenity::InteractionResponseType::ChannelMessageWithSource);
                b.interaction_response_data(|b| b.content("Loading!"))
            }).await?;
            
            interaction.delete_original_interaction_response(http).await?;
        }
    }

    let data = ctx.data();
    let pages: Vec<String> = data.voices.iter().map(|(language, variants)| {
        variants.iter().map(|(variant, gender)| {
            format!("{} - {variant} ({gender})\n", language)
        }).collect()
    }).collect();

    let lang_variant = crate::funcs::parse_voice(&data.guilds_db, &data.userinfo_db, ctx.author().id, ctx.guild_id()).await?;
    let (lang, variant) = lang_variant.split_once(' ').unwrap();

    let variant = String::from(variant);
    let (variant, gender) = get_voice(&ctx, &data.voices, lang, Some(&variant)).await?.unwrap();
    MenuPaginator::new(ctx, pages, format!("{} {variant} ({gender})", lang)).start().await
}


#[cfg(feature="premium")] pub use server_voice as server_language;
#[cfg(feature="premium")] pub use voices as languages;
#[cfg(feature="premium")] pub use voice as language;
