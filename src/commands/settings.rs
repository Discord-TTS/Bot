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

use crate::funcs::{get_gtts_voices, get_espeak_voices, netural_colour, parse_user_or_guild};
use crate::constants::{OPTION_SEPERATORS, PREMIUM_NEUTRAL_COLOUR};
use crate::structs::{Context, Error, TTSMode, Data};
use crate::{random_footer, database};


/// Displays the current settings!
#[poise::command(category="Settings", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")]
pub async fn settings(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?;
    let author_id = ctx.author().id;

    let data = ctx.data();
    let ctx_discord = ctx.discord();

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let userinfo_row = data.userinfo_db.get(author_id.into()).await?;
    let nickname_row = data.nickname_db.get([guild_id.into(), author_id.into()]).await?;

    let channel_name = guild_row.get::<&str, Option<i64>>("channel")
        .and_then(|c| ctx_discord.cache.guild_channel_field(c as u64, |c| c.name.clone()).map(Cow::Owned))
        .unwrap_or(Cow::Borrowed("has not been set up yet"));

    let xsaid: bool = guild_row.get("xsaid");
    let prefix: String = guild_row.get("prefix");
    let autojoin: bool = guild_row.get("auto_join");
    let msg_length: i16 = guild_row.get("msg_length");
    let bot_ignore: bool = guild_row.get("bot_ignore");
    let to_translate: bool = guild_row.get("to_translate");
    let repeated_chars: i16 = guild_row.get("repeated_chars");
    let audience_ignore: bool = guild_row.get("audience_ignore");

    let guild_mode: TTSMode = guild_row.get("voice_mode");
    let user_mode: Option<TTSMode> = userinfo_row.get("voice_mode");

    let format_voice = |voice: String, mode: TTSMode| {
        if mode == TTSMode::Premium {
            let (lang, variant) = voice.split_once(' ').unwrap();
            let gender = &data.premium_voices[lang][variant];
            format!("{lang} - {variant} ({gender})")
        } else {
            voice
        }
    };

    let none = String::from("none");

    let default_voice = format_voice(data.guild_voice_db.get((guild_id.into(), guild_mode)).await?.get("voice"), guild_mode);
    let user_voice = match user_mode {
        Some(mode) => format_voice(data.user_voice_db.get((author_id.into(), mode)).await?.get("voice"), mode),
        None => none.clone()
    };

    let target_lang = guild_row.get::<_, Option<String>>("target_lang").unwrap_or_else(|| none.clone());
    let nickname = nickname_row.get::<_, Option<String>>("name").unwrap_or_else(|| none.clone());

    let netural_colour = netural_colour(crate::premium_check(data, Some(guild_id)).await?.is_none());
    let [sep1, sep2, sep3, sep4] = OPTION_SEPERATORS;
    ctx.send(|b| {b.embed(|e| {
        e.title("Current Settings");
        e.url(&data.config.server_invite);
        e.colour(netural_colour);
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
{sep2} <User> said: message: `{xsaid}`
{sep2} Ignore bot's messages: `{bot_ignore}`
{sep2} Ignore audience messages: `{audience_ignore}`

**{sep2} Default Server Voice Mode: `{guild_mode}`**
**{sep2} Default Server Voice: `{default_voice}`**

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        "), false);
        e.field("**Translation Settings (Premium Only)**", format!("
{sep4} Translation: `{to_translate}`
{sep4} Translation Language: `{target_lang}`
        "), false);
        e.field("**User Specific**", format!("
{sep3} Voice: `{user_voice}`
{sep3} Voice Mode: `{}`
{sep3} Nickname: `{nickname}`
        ", user_mode.map_or(none, |v| format!("{:#}", v))), false)
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
        if let Some(value) = value {
            value
        } else {
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
    )
}
#[allow(clippy::too_many_arguments)]
async fn change_mode<T: database::CacheKeyTrait + std::hash::Hash + std::cmp::Eq + Send + Sync + Copy>(
    ctx: &Context<'_>,
    general_db: &database::Handler<T>,
    guild_id: serenity::GuildId,
    key: T, mode: Option<TTSMode>,
    target: &str
) -> Result<Option<Cow<'static, str>>, Error> {
    let data = ctx.data();
    if mode == Some(TTSMode::Premium) && crate::premium_check(data, Some(guild_id)).await?.is_some() {
        ctx.send(|b| b.embed(|e| {
            e.title("TTS Bot Premium");
            e.colour(PREMIUM_NEUTRAL_COLOUR);
            e.thumbnail(data.premium_avatar_url.clone());
            e.url("https://www.patreon.com/Gnome_the_Bot_Maker");
            e.footer(|f| f.text("If this is an error, please contact Gnome!#6669."));
            e.description(format!("
                The `Premium` TTS Mode is only for TTS Bot Premium subscribers, please check out the `{prefix}premium` command!
                If this server has purchased premium, please run the `{prefix}activate` command to link yourself to this server!
            ", prefix=ctx.prefix()))
        })).await?;
        Ok(None)
    } else {
        general_db.set_one(key, "voice_mode", &mode).await?;
        Ok(Some(match mode {
            Some(mode) => Cow::Owned(format!("Changed {target} TTS Mode to: {}", mode)),
            None => Cow::Borrowed("Reset {target} mode")
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn change_voice<T>(
    ctx: &Context<'_>,
    general_db: &database::Handler<T>,
    voice_db: &database::Handler<(T, TTSMode)>,
    author_id: serenity::UserId, guild_id: serenity::GuildId,
    key: T, voice: Option<String>,
    target: &str,
) -> Result<String, Error>
    where T: database::CacheKeyTrait + std::hash::Hash + std::cmp::Eq + Send + Sync + Copy,
    (T, TTSMode): database::CacheKeyTrait
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
        TTSMode::Gtts => get_gtts_voices().contains_key(&voice),
        TTSMode::Espeak => get_espeak_voices(&data.reqwest, data.config.tts_service.clone()).await?.contains(&voice),
        TTSMode::Premium => {
            let mut voice = voice.split_whitespace();
            let language = match voice.next() {
                Some(i) => i,
                None => return Ok(false)
            };

            data.premium_voices.contains_key(language)
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

#[derive(poise::ChoiceParameter)]
pub enum TTSModeServerChoice {
    // Name to show in slash command invoke           Aliases for prefix
    #[name="Google Translate TTS (female) (default)"] #[name="gtts"]       Gtts,
    #[name="eSpeak TTS (male)"]                       #[name="espeak"]     Espeak,
    #[name="Premium TTS (changable)"]                 #[name="premium"]    Premium
}

#[derive(poise::ChoiceParameter)]
pub enum TTSModeChoice {
    // Name to show in slash command invoke           Aliases for prefix
    #[name="eSpeak TTS (male)"]                       #[name="espeak"]     Espeak,
    #[name="Premium TTS (changable)"]                 #[name="premium"]    Premium
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

/// Changes the default mode for TTS that messages are read in
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("server_voice_mode", "server_tts_mode", "server_ttsmode")
)]
pub async fn server_mode(
    ctx: Context<'_>,
    #[description="The TTS Mode to change to"] mode: TTSModeServerChoice
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?;

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
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES",
    aliases("defaultlang", "default_lang", "defaultlang", "slang", "serverlanguage")
)]
pub async fn server_voice(
    ctx: Context<'_>,
    #[description="The default voice to read messages in"] voice: String
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?;

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
    category="Settings",
    check="crate::premium_command_check",
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

/// Changes the target language for translation
#[poise::command(
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
) -> Result<(), Error> {

    let data = ctx.data();
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?.into();

    let translation_langs = get_translation_langs(
        &data.reqwest,
        data.config.translation_token.as_ref().expect("Tried to do translation without token set in config!")
    ).await?;

    match lang {
        Some(lang) if translation_langs.contains(&lang) => {
            data.guilds_db.set_one(guild_id, "target_lang", &lang).await?;
            ctx.say(format!(
                "The target translation language is now: {lang}{}",
                if data.guilds_db.get(guild_id).await?.get::<&str, bool>("to_translate") {
                    String::new()
                } else {
                    format!(". You may want to enable translation with `{}set translation on`", ctx.prefix())
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
        } else if chars < 5 && chars != 0 {
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
#[poise::command(
    category="Settings",
    check="crate::premium_command_check",
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
    #[description="The nickname to set, leave blank to reset"] #[rest] nickname: Option<String>
) -> Result<(), Error> {
    let ctx_discord = ctx.discord();
    let guild = ctx.guild().ok_or(Error::GuildOnly)?;

    let author = ctx.author();
    let user = user.unwrap_or_else(|| author.clone());

    if author.id != user.id && !guild.member(ctx_discord, author).await?.permissions(ctx_discord)?.administrator() {
        ctx.say("**Error**: You need admin to set other people's nicknames!").await?;
        return Ok(())
    }

    let data = ctx.data();

    let to_send =
        if let Some(nick) = nickname {
            if nick.contains('<') && nick.contains('>') {
                String::from("**Error**: You can't have mentions/emotes in your nickname!")
            } else {
                let (r1, r2) = tokio::join!(
                    data.guilds_db.create_row(guild.id.into()),
                    data.userinfo_db.create_row(user.id.into())
                ); r1?; r2?;

                data.nickname_db.set_one([guild.id.into(), user.id.into()], "name", &nick).await?;
                format!("Changed {}'s nickname to {}", user.name, nick)
            }
        } else {
            data.nickname_db.delete([guild.id.into(), user.id.into()]).await?;
            format!("Reset {}'s nickname", user.name)
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

            let message = ctx.send(|b| {
                b.content("Select a channel!");
                b.components(|c| {
                    for (i, chunked_channels) in text_channels.chunks(25).enumerate() {
                        c.create_action_row(|r| {
                            r.create_select_menu(|s| {
                                s.custom_id(format!("select::channels::{i}"));
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

/// Changes the default mode for TTS that messages are read in
#[poise::command(
    category="Settings",
    prefix_command, slash_command,
    required_permissions="ADMINISTRATOR",
    required_bot_permissions="SEND_MESSAGES | EMBED_LINKS",
    aliases("voice_mode", "server_mode", "server_tts_mode", "server_ttsmode")
)]
pub async fn mode(
    ctx: Context<'_>,
    #[description="The TTS Mode to change to"] mode: TTSModeChoice
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?;

    let data = ctx.data();
    let to_send = change_mode(
        &ctx, &data.userinfo_db,
        guild_id, ctx.author().id.into(),
        Some(TTSMode::from(mode)), "your"
    ).await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the voice your messages are read in, full list in `-voices`
#[poise::command(
    category="Settings",
    aliases("language", "voice"),
    prefix_command, slash_command,
    required_bot_permissions="SEND_MESSAGES",
)]
pub async fn voice(
    ctx: Context<'_>,
    #[description="The voice to read messages in, leave blank to reset"] voice: Option<String>
) -> Result<(), Error> {
    let data = ctx.data();
    let author_id = ctx.author().id;
    let guild_id = ctx.guild_id().ok_or(Error::GuildOnly)?;

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
pub async fn voices(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let data = ctx.data();

    let (_, mode) = parse_user_or_guild(data, author.id, ctx.guild_id()).await?;
    let voices: String = {
        let mut supported_langs = match mode {
            TTSMode::Gtts => crate::funcs::get_gtts_voices().into_iter().map(|(k, _)| k).collect(),
            TTSMode::Espeak => crate::funcs::get_espeak_voices(&data.reqwest, data.config.tts_service.clone()).await?,
            TTSMode::Premium => return list_premium_voices(ctx).await
        };

        supported_langs.sort_unstable();
        supported_langs.into_iter().map(|l| format!("`{l}`, ")).collect()
    };

    let cache = &ctx.discord().cache;
    let current_lang: Option<String> = data.user_voice_db.get((author.id.into(), mode)).await?.get("voice");
    ctx.send(|b| {b.embed(|e| {
        e.title(format!("{} Voices | Mode: `{}`", cache.current_user_field(|u| u.name.clone()), mode));
        e.footer(|f| f.text(random_footer(
            Some(&String::from(ctx.prefix())),
            Some(&data.config.server_invite),
            Some(cache.current_user_id().0)
        )));
        e.author(|a| {
            a.name(author.name.clone());
            a.icon_url(author.face())
        });
        e.field(
            "Currently supported voices",
            voices.strip_suffix(", ").unwrap_or(&voices),
            true
        );
        e.field(
            "Current voice used",
            current_lang.as_ref().map_or("None", std::ops::Deref::deref),
            false
        )
    })}).await?;

    Ok(())
}


pub async fn list_premium_voices(ctx: Context<'_>) -> Result<(), Error> {
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
    let pages: Vec<String> = data.premium_voices.iter().map(|(language, variants)| {
        variants.iter().map(|(variant, gender)| {
            format!("{language} - {variant} ({gender})\n")
        }).collect()
    }).collect();

    let (lang_variant, _) = parse_user_or_guild(data, ctx.author().id, ctx.guild_id()).await?;
    let (lang, variant) = lang_variant.split_once(' ').unwrap();

    let variant = String::from(variant);
    let gender = &data.premium_voices[lang][&variant];
    MenuPaginator::new(ctx, pages, format!("{lang} {variant} ({gender})")).start().await
}
