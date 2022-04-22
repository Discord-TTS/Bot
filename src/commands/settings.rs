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

use std::borrow::Cow;

use itertools::Itertools;

use poise::serenity_prelude as serenity;

use crate::structs::{Context, Result, Error, TTSMode, Data, TTSModeServerChoice, CommandResult, PoiseContextExt};
use crate::funcs::{get_gtts_voices, get_espeak_voices, random_footer, parse_user_or_guild};
use crate::constants::{OPTION_SEPERATORS, PREMIUM_NEUTRAL_COLOUR};
use crate::macros::{require_guild, require};
use crate::{database, funcs};

fn format_voice<'a>(data: &Data, voice: &'a str, mode: TTSMode) -> Cow<'a, str> {
    if mode == TTSMode::Premium {
        let (lang, variant) = voice.split_once(' ').unwrap();
        let gender = &data.premium_voices[lang][variant];
        Cow::Owned(format!("{lang} - {variant} ({gender})"))
    } else {
        Cow::Borrowed(voice)
    }
}

/// Displays the current settings!
#[poise::command(
    category="Settings",
    guild_only, prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")
]
pub async fn settings(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let author_id = ctx.author().id;

    let data = ctx.data();
    let ctx_discord = ctx.discord();

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let userinfo_row = data.userinfo_db.get(author_id.into()).await?;
    let nickname_row = data.nickname_db.get([guild_id.into(), author_id.into()]).await?;

    let default_channel_name = || Cow::Borrowed("has not been set up yet");
    let channel_name = if guild_row.channel == 0 {
        ctx_discord.cache
            .guild_channel_field(guild_row.channel as u64, |c| c.name.clone())
            .map_or_else(default_channel_name, Cow::Owned)
    } else {
        default_channel_name()
    };

    let xsaid = guild_row.xsaid;
    let prefix = &guild_row.prefix;
    let autojoin = guild_row.auto_join;
    let msg_length = guild_row.msg_length;
    let bot_ignore = guild_row.bot_ignore;
    let to_translate = guild_row.to_translate;
    let require_voice = guild_row.require_voice;
    let repeated_chars = guild_row.repeated_chars;
    let audience_ignore = guild_row.audience_ignore;

    let guild_mode = guild_row.voice_mode;
    let user_mode = userinfo_row.voice_mode;

    let guild_voice_row = data.guild_voice_db.get((guild_id.into(), guild_mode)).await?;
    let default_voice = {
        if guild_voice_row.guild_id == 0 {
            Cow::Borrowed(guild_mode.default_voice())
        } else {
            format_voice(data, &guild_voice_row.voice, guild_mode)
        }
    };

    let row;
    let (user_voice, speaking_rate, speaking_rate_kind) =
        if let Some(mode) = user_mode {
            row = data.user_voice_db.get((author_id.into(), mode)).await?;

            let (default, kind) = mode.speaking_rate_info()
                .map_or((1.0, "x"), |(_, d, _, k)| (d, k));

            (
                row.voice.as_deref().map_or(Cow::Borrowed("none"), |user_voice| format_voice(data, user_voice, mode)),
                Cow::Owned(row.speaking_rate.unwrap_or(default).to_string()),
                kind,
            )
        } else {
            (Cow::Borrowed("none"), Cow::Borrowed("1.0"), "x")
        };

    let target_lang = guild_row.target_lang.as_deref().unwrap_or("none");
    let nickname = nickname_row.name.as_deref().unwrap_or("none");

    let neutral_colour = ctx.neutral_colour().await;
    let [sep1, sep2, sep3, sep4] = OPTION_SEPERATORS;
    ctx.send(|b| {b.embed(|e| {e
        .title("Current Settings")
        .url(&data.config.main_server_invite)
        .colour(neutral_colour)
        .footer(|f| {
            f.text(format!(concat!(
                "Change these settings with {prefix}set property value!\n",
                "None = setting has not been set yet!"
            ), prefix=prefix))
        })

        .field("**General Server Settings**", format!("
{sep1} Setup Channel: `#{channel_name}`
{sep1} Command Prefix: `{prefix}`
{sep1} Auto Join: `{autojoin}`
        "), false)
        .field("**TTS Settings**", format!("
{sep2} <User> said: message: `{xsaid}`
{sep2} Ignore bot's messages: `{bot_ignore}`
{sep2} Ignore audience messages: `{audience_ignore}`
{sep2} Require users in voice channel: `{require_voice}`

**{sep2} Default Server Voice Mode: `{guild_mode}`**
**{sep2} Default Server Voice: `{default_voice}`**

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        "), false)
        .field("**Translation Settings (Premium Only)**", format!("
{sep4} Translation: `{to_translate}`
{sep4} Translation Language: `{target_lang}`
        "), false)
        .field("**User Specific**", format!("
{sep3} Voice: `{user_voice}`
{sep3} Voice Mode: `{}`
{sep3} Nickname: `{nickname}`
{sep3} Speaking Rate: `{speaking_rate}{speaking_rate_kind}`
        ", user_mode.map_or("none", Into::into)
        ), false)
    })}).await?;

    Ok(())
}


struct MenuPaginator<'a> {
    index: usize,
    ctx: Context<'a>,
    pages: Vec<String>,
    current_lang: String,
}

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

        embed
            .title(format!("{bot_name} Languages"))
            .description(format!("**Currently Supported Languages**\n{page}"))
            .field("Current Language used", &self.current_lang, false)
            .author(|a| {a
                .name(author.name.clone())
                .icon_url(author.face())
            })
            .footer(|f| {f.text(random_footer(
                self.ctx.prefix(), &self.ctx.data().config.main_server_invite, bot_id.into()
            ))})
    }

    fn create_action_row<'b>(&self, builder: &'b mut serenity::CreateActionRow, disabled: bool) -> &'b mut serenity::CreateActionRow {
        for emoji in ["⏮️", "◀", "⏹️", "▶️", "⏭️"] {
            builder.create_button(|b| {b
                .custom_id(emoji)
                .style(serenity::ButtonStyle::Primary)
                .emoji(serenity::ReactionType::Unicode(String::from(emoji)))
                .disabled(
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
        message.edit(self.ctx.discord(), |b| {b
            .embed(|e| self.create_page(e, &self.pages[self.index]))
            .components(|c| c.create_action_row(|r| self.create_action_row(r, disable)))
        }).await?;

        Ok(())
    }


    pub async fn start(mut self) -> Result<(), Error> {
        let ctx_discord = self.ctx.discord();
        let mut message = self.create_message().await?;

        loop {
            let collector = message
                .await_component_interaction(&ctx_discord.shard)
                .timeout(std::time::Duration::from_secs(60 * 5))
                .author_id(self.ctx.author().id)
                .collect_limit(1);

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

async fn bool_button(ctx: Context<'_>, value: Option<bool>) -> Result<Option<bool>, Error> {
    funcs::bool_button(ctx, "What would you like to set this to?", "True", "False", value).await
}

#[allow(clippy::too_many_arguments)]
async fn change_mode<CacheKey, RowT>(
    ctx: &Context<'_>,
    general_db: &database::Handler<CacheKey, RowT>,
    guild_id: serenity::GuildId,
    key: CacheKey, mode: Option<TTSMode>,
    target: &str
) -> Result<Option<String>, Error>
where
    CacheKey: database::CacheKeyTrait + std::hash::Hash + std::cmp::Eq + Send + Sync + Copy,
    RowT: for<'a> sqlx::FromRow<'a, sqlx::postgres::PgRow> + Send + Sync + Unpin,
{
    let data = ctx.data();
    if mode == Some(TTSMode::Premium) && crate::premium_check(ctx.discord(), data, Some(guild_id)).await?.is_some() {
        ctx.send(|b| b.embed(|e| {e
            .title("TTS Bot Premium")
            .colour(PREMIUM_NEUTRAL_COLOUR)
            .thumbnail(data.premium_avatar_url.clone())
            .url("https://www.patreon.com/Gnome_the_Bot_Maker")
            .footer(|f| f.text("If this is an error, please contact Gnome!#6669."))
            .description(format!("
                The `Premium` TTS Mode is only for TTS Bot Premium subscribers, please check out the `{prefix}premium` command!
                If this server has purchased premium, please run the `{prefix}activate` command to link yourself to this server!
            ", prefix=ctx.prefix()))
        })).await?;
        Ok(None)
    } else {
        general_db.set_one(key, "voice_mode", &mode).await?;
        Ok(Some(match mode {
            Some(mode) => format!("Changed {target} TTS Mode to: {mode}"),
            None => format!("Reset {target} mode")
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn change_voice<T, RowT1, RowT2>(
    ctx: &Context<'_>,
    general_db: &database::Handler<T, RowT1>,
    voice_db: &database::Handler<(T, TTSMode), RowT2>,
    author_id: serenity::UserId, guild_id: serenity::GuildId,
    key: T, voice: Option<String>,
    target: &str,
) -> Result<String, Error>
where
    RowT1: for<'a> sqlx::FromRow<'a, sqlx::postgres::PgRow> + Send + Sync + Unpin,
    RowT2: for<'a> sqlx::FromRow<'a, sqlx::postgres::PgRow> + Send + Sync + Unpin,

    T: database::CacheKeyTrait + std::hash::Hash + std::cmp::Eq + Send + Sync + Copy,
    (T, TTSMode): database::CacheKeyTrait,
{
    let (_, mode) = parse_user_or_guild(ctx.data(), author_id, Some(guild_id)).await?;
    Ok(if let Some(voice) = voice {
        if check_valid_voice(ctx.data(), voice.clone(), mode).await? {
            general_db.set_one(key, "voice_mode", &mode).await?;
            voice_db.set_one((key, mode), "voice", &voice).await?;
            format!("Changed {target} voice to: {voice}")
        } else {
            format!("Invalid voice, do `{}voices`", ctx.prefix())
        }
    } else {
        voice_db.delete((key, mode)).await?;
        format!("Reset {target} voice")
    })
}

async fn check_valid_voice(data: &Data, voice: String, mode: TTSMode) -> Result<bool, Error> {
    Ok(match mode {
        TTSMode::gTTS => get_gtts_voices().contains_key(&voice),
        TTSMode::eSpeak => get_espeak_voices(&data.reqwest, data.config.tts_service.clone()).await?.contains(&voice),
        TTSMode::Premium => {
            voice.split_once(' ')
                .and_then(|(language, variant)| data.premium_voices.get(language).map(|l| (l, variant)))
                .map_or(false, |(ls, v)| ls.contains_key(v))
        }
    })
}

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



const fn to_enabled(value: bool) -> &'static str {
    if value {
        "Enabled"
    } else {
        "Disabled"
    }
}

/// Changes a setting!
#[poise::command(category="Settings", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")]
pub async fn set(ctx: Context<'_>, ) -> CommandResult {
    crate::commands::help::_help(ctx, Some("set")).await
}

/// Owner only: used to block a user from dms
#[poise::command(
    prefix_command,
    category="Settings",
    owners_only, hide_in_help,
    required_bot_permissions="SEND_MESSAGES"
)]
pub async fn block(
    ctx: Context<'_>,
    user: serenity::UserId,
    value: bool
) -> CommandResult {
    ctx.data().userinfo_db.set_one(user.into(), "dm_blocked", &value).await?;
    ctx.say("Done!").await?;
    Ok(())
}

/// Makes the bot say "<user> said" before each message
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES"
)]
pub async fn xsaid(
    ctx: Context<'_>,
    #[description="Whether to say \"<user> said\" before each message"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "xsaid", &value).await?;
    ctx.say(format!("xsaid is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Makes the bot join the voice channel automatically when a message is sent in the setup channel
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("auto_join")
)]
pub async fn autojoin(
    ctx: Context<'_>,
    #[description="Whether to autojoin voice channels"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "auto_join", &value).await?;
    ctx.say(format!("Auto Join is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Makes the bot ignore messages sent by bots and webhooks 
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("bot_ignore", "ignore_bots", "ignorebots")
)]
pub async fn botignore(
    ctx: Context<'_>,
    #[description="Whether to ignore messages sent by bots and webhooks"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "bot_ignore", &value).await?;
    ctx.say(format!("Ignoring bots is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Makes the bot require people to be in the voice channel to TTS
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("voice_require", "require_in_vc")
)]
pub async fn require_voice(
    ctx: Context<'_>,
    #[description="Whether to require people to be in the voice channel to TTS"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "require_voice", &value).await?;
    ctx.say(format!("Requiring users to be in voice channel for TTS is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Changes the default mode for TTS that messages are read in
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("server_voice_mode", "server_tts_mode", "server_ttsmode")
)]
pub async fn server_mode(
    ctx: Context<'_>,
    #[description="The TTS Mode to change to"] mode: TTSModeServerChoice
) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();

    let data = ctx.data();
    let to_send = change_mode(
        &ctx, &data.guilds_db,
        guild_id, guild_id.into(),
        Some(TTSMode::from(mode)), "the server"
    ).await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the default language messages are read in
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("defaultlang", "default_lang", "defaultlang", "slang", "serverlanguage")
)]
pub async fn server_voice(
    ctx: Context<'_>,
    #[description="The default voice to read messages in"] #[rest] voice: String
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_voice(
        &ctx, &data.guilds_db, &data.guild_voice_db,
        ctx.author().id, guild_id, guild_id.into(), Some(voice),
        "the server"
    ).await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Whether to use DeepL translate to translate all TTS messages to the same language 
#[poise::command(
    guild_only,
    category="Settings",
    check="crate::premium_command_check",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("translate", "to_translate", "should_translate")
)]
pub async fn translation(ctx: Context<'_>, #[description="Whether to translate all messages to the same language"] value: Option<bool>) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "to_translate", &value).await?;
    ctx.say(format!("Translation is now: {}", to_enabled(value))).await?;

    Ok(())
}

/// Changes the target language for translation
#[poise::command(
    guild_only,
    category="Settings",
    check="crate::premium_command_check",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("tlang", "tvoice", "target_lang", "target_voice", "target_language")
)]
pub async fn translation_lang(
    ctx: Context<'_>,
    #[description="The language to translate all TTS messages to"] lang: Option<String>
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap().into();

    let translation_langs = get_translation_langs(
        &data.reqwest,
        data.config.translation_token.as_ref().expect("Tried to do translation without token set in config!")
    ).await?;

    match lang {
        Some(lang) if translation_langs.contains(&lang) => {
            data.guilds_db.set_one(guild_id, "target_lang", &lang).await?;
            ctx.say(format!(
                "The target translation language is now: {lang}{}",
                if data.guilds_db.get(guild_id).await?.to_translate {
                    String::new()
                } else {
                    format!(". You may want to enable translation with `{}set translation on`", ctx.prefix())
                }
            )).await?;
        },
        _ => {
            ctx.send(|b| b.embed(|e| {e
                .title("DeepL Translation - Supported languages")
                .description(format!("```{}```", translation_langs.iter().join(", ")))
            })).await?;
        }
    }

    Ok(())
}


/// Changes the prefix used before commands
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn prefix(
    ctx: Context<'_>,
    #[description="The prefix to be used before commands"] #[rest] prefix: String
) -> CommandResult {
    let to_send = if prefix.len() <= 5 && prefix.matches(' ').count() <= 1 {
        ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "prefix", &prefix).await?;
        Cow::Owned(format!("Command prefix for this server is now: {}", prefix))
    } else {
        Cow::Borrowed("**Error**: Invalid Prefix, please use 5 or less characters with maximum 1 space")
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the max repetion of a character (0 = off)
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("repeated_chars", "repeated_letters", "chars")
)]
pub async fn repeated_characters(ctx: Context<'_>, #[description="The max repeated characters"] chars: u8) -> CommandResult {
    let to_send = {
        if chars > 100 {
            Cow::Borrowed("**Error**: Cannot set the max repeated characters above 100")
        } else if chars < 5 && chars != 0 {
            Cow::Borrowed("**Error**: Cannot set the max repeated characters below 5")
        } else {
            ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "repeated_chars", &(chars as i16)).await?;
            Cow::Owned(format!("Max repeated characters is now: {chars}"))
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Makes the bot ignore messages sent by members of the audience in stage channels
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("audience_ignore", "ignore_audience", "ignoreaudience")
)]
pub async fn audienceignore(
    ctx: Context<'_>,
    #[description="Whether to ignore messages sent by the audience"] value: Option<bool>
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    ctx.data().guilds_db.set_one(ctx.guild_id().unwrap().into(), "audience_ignore", &value).await?;
    ctx.say(format!("Ignoring audience is now: {}", to_enabled(value))).await?;
    Ok(())
}

/// Changes the multiplier for how fast to speak
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
    aliases("speed", "speed_multiplier", "speaking_rate_multiplier", "speaking_speed", "tts_speed")
)]
pub async fn speaking_rate(
    ctx: Context<'_>,
    #[description="The speed to speak at"] #[min=0] #[max=400.0] speaking_rate: f32
) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();

    let (_, mode) = parse_user_or_guild(data, author.id, ctx.guild_id()).await?;
    let (min, _, max, kind) =
        if let Some(info) = mode.speaking_rate_info() {
            info
        } else {
            ctx.say(format!("**Error**: Cannot set speaking rate for the {mode} mode")).await?;
            return Ok(())
        };

    let to_send = {
        if speaking_rate > max {
            format!("**Error**: Cannot set the speaking rate multiplier above {max}{kind}")
        } else if speaking_rate < min {
            format!("**Error**: Cannot set the speaking rate multiplier below {min}{kind}")
        } else {
            data.user_voice_db.set_one((author.id.0 as i64, mode), "speaking_rate", &speaking_rate).await?;
            format!("Your speaking rate is now: {speaking_rate}{kind}")
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Replaces your username in "<user> said" with a given name
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
    aliases("nick_name", "nickname", "name"),
)]
pub async fn nick(
    ctx: Context<'_>,
    #[description="The user to set the nick for, defaults to you"] user: Option<serenity::User>,
    #[description="The nickname to set, leave blank to reset"] #[rest] nickname: Option<String>
) -> CommandResult {
    let ctx_discord = ctx.discord();
    let guild = require_guild!(ctx);

    let author = ctx.author();
    let user = user.map_or(Cow::Borrowed(author), Cow::Owned);

    if author.id != user.id && !guild.member(ctx_discord, author).await?.permissions(ctx_discord)?.administrator() {
        ctx.say("**Error**: You need admin to set other people's nicknames!").await?;
        return Ok(())
    }

    let data = ctx.data();

    let to_send =
        if let Some(nick) = nickname {
            if nick.contains('<') && nick.contains('>') {
                Cow::Borrowed("**Error**: You can't have mentions/emotes in your nickname!")
            } else {
                let (r1, r2) = tokio::join!(
                    data.guilds_db.create_row(guild.id.into()),
                    data.userinfo_db.create_row(user.id.into())
                ); r1?; r2?;

                data.nickname_db.set_one([guild.id.into(), user.id.into()], "name", &nick).await?;
                Cow::Owned(format!("Changed {}'s nickname to {}", user.name, nick))
            }
        } else {
            data.nickname_db.delete([guild.id.into(), user.id.into()]).await?;
            Cow::Owned(format!("Reset {}'s nickname", user.name))
        };

    ctx.say(to_send).await?;
    Ok(())
}


fn can_send(guild: &serenity::Guild, channel: &serenity::GuildChannel, member: &serenity::Member) -> bool {
    const REQUIRED_PERMISSIONS: serenity::Permissions = serenity::Permissions::from_bits_truncate(
        serenity::Permissions::SEND_MESSAGES.bits() | serenity::Permissions::VIEW_CHANNEL.bits()
    );

    guild.user_permissions_in(channel, member)
        .map(|p| (REQUIRED_PERMISSIONS - p).is_empty())
        .unwrap_or(false)
}


/// Setup the bot to read messages from the given channel
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn setup(
    ctx: Context<'_>,
    #[description="The channel for the bot to read messages from"] #[channel_types("Text")]
    channel: Option<serenity::GuildChannel>
) -> CommandResult {
    let guild = require_guild!(ctx);

    let ctx_discord = ctx.discord();
    let cache = &ctx_discord.cache;

    let author = ctx.author();
    let bot_user = cache.current_user();

    let channel: u64 =
        if let Some(channel) = channel {
            channel.id.into()
        } else {
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
            } else if text_channels.len() >= (25 * 5) {
                ctx.say(format!("**Error** This server has too many text channels to show in a menu! Please run `{}setup #channel`", ctx.prefix())).await?;
                return Ok(())
            };

            text_channels.sort_by(|f, s| Ord::cmp(&f.position, &s.position));

            let message = ctx.send(|b| {b
                .content("Select a channel!")
                .components(|c| {
                    for (i, chunked_channels) in text_channels.chunks(25).enumerate() {
                        c.create_action_row(|r| {
                            r.create_select_menu(|s| {s
                                .custom_id(format!("select::channels::{i}"))
                                .options(|os| {
                                    for channel in chunked_channels {
                                        os.create_option(|o| {o
                                            .label(&channel.name)
                                            .value(channel.id)
                                        });
                                    };
                                    os
                                })
                            })
                        });
                    };
                    c
                })
            }).await?.message().await?;

            let interaction = message
                .await_component_interaction(&ctx_discord.shard)
                .timeout(std::time::Duration::from_secs(60 * 5))
                .author_id(ctx.author().id)
                .collect_limit(1)
                .await;

            if let Some(interaction) = interaction {
                interaction.defer(&ctx_discord.http).await?;
                interaction.data.values[0].parse().unwrap()
            } else {
                // The timeout was hit
                return Ok(())
            }
        };

    let data = ctx.data();
    data.guilds_db.set_one(guild.id.into(), "channel", &(channel as i64)).await?;
    ctx.send(|b| b.embed(|e| {e
        .title(format!("{} has been setup!", bot_user.name))
        .thumbnail(bot_user.face())
        .description(format!("
TTS Bot will now accept commands and read from <#{channel}>.
Just do `{}join` and start talking!
        ", ctx.prefix()))

        .footer(|f| {f.text(random_footer(
            ctx.prefix(), &data.config.main_server_invite, cache.current_user_id().0
        ))})
        .author(|a| {
            a.name(&author.name);
            a.icon_url(author.face())
        })
    })).await?;

    Ok(())
}

/// Changes the voice mode that messages are read in for you
#[poise::command(
    guild_only,
    category="Settings",
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("voice_mode", "tts_mode", "ttsmode")
)]
pub async fn mode(
    ctx: Context<'_>,
    #[description="The TTS Mode to change to, leave blank for server default"] mode: Option<crate::structs::TTSModeChoice>
) -> CommandResult {
    let to_send = change_mode(
        &ctx, &ctx.data().userinfo_db,
        ctx.guild_id().unwrap(), ctx.author().id.into(),
        mode.map(TTSMode::from), "your"
    ).await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the voice your messages are read in, full list in `-voices`
#[poise::command(
    guild_only,
    category="Settings",
    aliases("language", "voice"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn voice(
    ctx: Context<'_>,
    #[description="The voice to read messages in, leave blank to reset"] #[rest] voice: Option<String>
) -> CommandResult {
    let data = ctx.data();
    let author_id = ctx.author().id;
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_voice(
        &ctx, &data.userinfo_db, &data.user_voice_db,
        author_id, guild_id, author_id.into(), voice,
        "your"
    ).await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Lists all the voices that TTS bot accepts for the current mode
#[poise::command(
    category="Settings",
    aliases("langs", "languages"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
)]
pub async fn voices(
    ctx: Context<'_>,
    #[description="The mode to see the voices for, leave blank for current"] mode: Option<TTSModeServerChoice>
) -> CommandResult {
    let author = ctx.author();
    let data = ctx.data();

    let mode = match mode {
        Some(mode) => TTSMode::from(mode),
        None => parse_user_or_guild(data, author.id, ctx.guild_id()).await?.1
    };

    let voices: String = {
        let mut supported_langs = match mode {
            TTSMode::gTTS => crate::funcs::get_gtts_voices().into_iter().map(|(k, _)| k).collect(),
            TTSMode::eSpeak => crate::funcs::get_espeak_voices(&data.reqwest, data.config.tts_service.clone()).await?,
            TTSMode::Premium => return list_premium_voices(ctx).await.map_err(Into::into)
        };

        supported_langs.sort_unstable();
        supported_langs.into_iter().map(|l| format!("`{l}`, ")).collect()
    };

    let cache = &ctx.discord().cache;
    let user_voice_row = data.user_voice_db.get((author.id.into(), mode)).await?;
    ctx.send(|b| {b.embed(|e| {e
        .title(format!("{} Voices | Mode: `{}`", cache.current_user_field(|u| u.name.clone()), mode))
        .footer(|f| f.text(random_footer(
            ctx.prefix(), &data.config.main_server_invite, cache.current_user_id().0
        )))
        .author(|a| {a
            .name(author.name.clone())
            .icon_url(author.face())
        })
        .field(
            "Currently supported voices",
            voices.strip_suffix(", ").unwrap_or(&voices),
            true
        )
        .field(
            "Current voice used",
            user_voice_row.voice.as_ref().map_or("None", std::ops::Deref::deref),
            false
        )
    })}).await?;

    Ok(())
}


pub async fn list_premium_voices(ctx: Context<'_>) -> Result<()> {
    let http = &ctx.discord().http; 
    if let poise::Context::Application(ctx) = ctx {
        if let poise::ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction) = ctx.interaction {
            interaction.create_interaction_response(http, |b| {b
                .kind(serenity::InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|b| b.content("Loading!"))
            }).await?;

            interaction.delete_original_interaction_response(http).await?;
        }
    }

    let data = ctx.data();
    let pages = data.premium_voices.iter().map(|(language, variants)| {
        variants.iter().map(|(variant, gender)| {
            format!("{language} - {variant} ({gender})\n")
        }).collect()
    }).collect();

    let (lang_variant, mode) = parse_user_or_guild(data, ctx.author().id, ctx.guild_id()).await?;
    let (lang, variant) = match mode {
        TTSMode::Premium => lang_variant.split_once(' ').unwrap(),
        _ => ("en-US", "A")
    };

    let variant = String::from(variant);
    let gender = data.premium_voices[lang][&variant];
    MenuPaginator::new(ctx, pages, format!("{lang} {variant} ({gender})")).start().await
}
