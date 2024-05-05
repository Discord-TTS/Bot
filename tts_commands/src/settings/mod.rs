// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

mod setup;
mod voice_paginator;

use std::{borrow::Cow, collections::HashMap, fmt::Write, num::NonZeroU8};

use arrayvec::ArrayString;
use poise::serenity_prelude as serenity;
use serenity::{
    builder::*,
    small_fixed_array::{FixedString, TruncatingInto},
    Mentionable,
};
use to_arraystring::ToArrayString;

use tts_core::{
    common::{confirm_dialog, random_footer},
    constants::{OPTION_SEPERATORS, PREMIUM_NEUTRAL_COLOUR},
    database::{self, Compact},
    require, require_guild,
    structs::{
        ApplicationContext, Command, CommandResult, Context, Data, Error, Result, SpeakingRateInfo,
        TTSMode, TTSModeChoice,
    },
    traits::PoiseContextExt,
    translations::GetTextContextExt,
};

use self::voice_paginator::MenuPaginator;

fn format_voice<'a>(data: &Data, voice: &'a str, mode: TTSMode) -> Cow<'a, str> {
    if mode == TTSMode::gCloud {
        let (lang, variant) = voice.split_once(' ').unwrap();
        let gender = &data.gcloud_voices[lang][variant];
        Cow::Owned(format!("{lang} - {variant} ({gender})"))
    } else if mode == TTSMode::Polly {
        let voice = &data.polly_voices[voice];
        Cow::Owned(format!(
            "{} - {} ({})",
            voice.name, voice.language_name, voice.gender
        ))
    } else {
        Cow::Borrowed(voice)
    }
}

/// Displays the current settings!
#[poise::command(
    category = "Settings",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn settings(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let author_id = ctx.author().id;

    let data = ctx.data();
    let none_str = ctx.gettext("none");

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let userinfo_row = data.userinfo_db.get(author_id.into()).await?;
    let nickname_row = data
        .nickname_db
        .get([guild_id.into(), author_id.into()])
        .await?;

    let channel_mention = if let Some(channel) = guild_row.channel
        && require_guild!(ctx).channels.contains_key(&channel)
    {
        Cow::Owned(channel.mention().to_string())
    } else {
        Cow::Borrowed(none_str)
    };

    let prefix = &guild_row.prefix;
    let guild_mode = guild_row.voice_mode;
    let nickname = nickname_row.name.as_deref().unwrap_or(none_str);
    let target_lang = guild_row.target_lang.as_deref().unwrap_or(none_str);
    let required_role = guild_row.required_role.map(|r| r.mention().to_string());

    let user_mode = if data.is_premium_simple(guild_id).await? {
        userinfo_row.premium_voice_mode
    } else {
        userinfo_row.voice_mode
    };

    let guild_voice_row = data
        .guild_voice_db
        .get((guild_id.into(), guild_mode))
        .await?;
    let default_voice = {
        if guild_voice_row.guild_id.is_none() {
            Cow::Borrowed(guild_mode.default_voice())
        } else {
            format_voice(&data, &guild_voice_row.voice, guild_mode)
        }
    };

    let user_voice_row;
    let user_voice = {
        let currently_set_voice_mode = user_mode.unwrap_or(guild_mode);
        user_voice_row = data
            .user_voice_db
            .get((author_id.into(), currently_set_voice_mode))
            .await?;

        user_voice_row
            .voice
            .as_ref()
            .map_or(Cow::Borrowed(none_str), |voice| {
                format_voice(&data, voice, currently_set_voice_mode)
            })
    };

    let (speaking_rate, speaking_rate_kind) = if let Some(mode) = user_mode {
        let user_voice_row = data.user_voice_db.get((author_id.into(), mode)).await?;
        let (default, kind) = mode
            .speaking_rate_info()
            .map_or(("1.0", "x"), |info| (info.default, info.kind));

        (
            user_voice_row
                .speaking_rate
                .map(f32::to_arraystring)
                .unwrap_or(ArrayString::from(default)?),
            kind,
        )
    } else {
        (ArrayString::from("1.0").unwrap(), "x")
    };

    let neutral_colour = ctx.neutral_colour().await;
    let [sep1, sep2, sep3, sep4] = OPTION_SEPERATORS;

    ctx.send(poise::CreateReply::default().embed(CreateEmbed::default()
        .title("Current Settings")
        .colour(neutral_colour)
        .url(data.config.main_server_invite.as_str())
        .footer(CreateEmbedFooter::new(ctx.gettext(
            "Change these settings with `/set {property} {value}`!\nNone = setting has not been set yet!"
        )))

        .field(ctx.gettext("**General Server Settings**"), &ctx.gettext("
{sep1} Setup Channel: {channel_mention}
{sep1} Required Role: {role_mention}
{sep1} Command Prefix: `{prefix}`
{sep1} Auto Join: `{autojoin}`
        ")
            .replace("{sep1}", sep1)
            .replace("{prefix}", prefix)
            .replace("{channel_mention}", &channel_mention)
            .replace("{autojoin}", &guild_row.auto_join().to_arraystring())
            .replace("{role_mention}", required_role.as_deref().unwrap_or(none_str)),
        false)
        .field("**TTS Settings**", &ctx.gettext("
{sep2} <User> said: message: `{xsaid}`
{sep2} Ignore bot's messages: `{bot_ignore}`
{sep2} Ignore audience messages: `{audience_ignore}`
{sep2} Require users in voice channel: `{require_voice}`
{sep2} Required prefix for TTS: `{required_prefix}`

**{sep2} Default Server Voice Mode: `{guild_mode}`**
**{sep2} Default Server Voice: `{default_voice}`**

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        ")
            .replace("{sep2}", sep2)
            .replace("{xsaid}", &guild_row.xsaid().to_arraystring())
            .replace("{bot_ignore}", &guild_row.bot_ignore().to_arraystring())
            .replace("{audience_ignore}", &guild_row.audience_ignore().to_arraystring())
            .replace("{require_voice}", &guild_row.require_voice().to_arraystring())
            .replace("{required_prefix}", guild_row.required_prefix.as_deref().unwrap_or(none_str))
            .replace("{guild_mode}", guild_mode.into())
            .replace("{default_voice}", &default_voice)
            .replace("{msg_length}", &guild_row.msg_length.to_arraystring())
            .replace("{repeated_chars}", &guild_row.repeated_chars.map_or(0, NonZeroU8::get).to_arraystring()),
        false)
        .field(ctx.gettext("**Translation Settings (Premium Only)**"), &ctx.gettext("
{sep4} Translation: `{to_translate}`
{sep4} Translation Language: `{target_lang}`
        ")
            .replace("{sep4}", sep4)
            .replace("{to_translate}", &guild_row.to_translate().to_arraystring())
            .replace("{target_lang}", target_lang),
        false)
        .field("**User Specific**", &ctx.gettext("
{sep3} Voice: `{user_voice}`
{sep3} Voice Mode: `{voice_mode}`
{sep3} Nickname: `{nickname}`
{sep3} Speaking Rate: `{speaking_rate}{speaking_rate_kind}`
        ")
            .replace("{sep3}", sep3)
            .replace("{user_voice}", &user_voice)
            .replace("{voice_mode}", user_mode.map_or(none_str, #[allow(clippy::redundant_closure_for_method_calls)] |m| m.into()))
            .replace("{nickname}", nickname)
            .replace("{speaking_rate}", &speaking_rate)
            .replace("{speaking_rate_kind}", speaking_rate_kind),
        false)
    )).await?;

    Ok(())
}

async fn voice_autocomplete<'a>(
    ctx: ApplicationContext<'a>,
    searching: &'a str,
) -> Vec<serenity::AutocompleteChoice<'a>> {
    let data = ctx.data();
    let Ok((_, mode)) = data
        .parse_user_or_guild(ctx.interaction.user.id, ctx.interaction.guild_id)
        .await
    else {
        return Vec::new();
    };

    let (mut i1, mut i2, mut i3, mut i4);
    let voices: &mut dyn Iterator<Item = _> = match mode {
        TTSMode::gTTS => {
            i1 = data
                .gtts_voices
                .iter()
                .map(|(k, v)| (v.to_string(), k.to_string()));
            &mut i1
        }
        TTSMode::eSpeak => {
            i2 = data
                .espeak_voices
                .iter()
                .map(|voice| (voice.to_string(), voice.to_string()));
            &mut i2
        }
        TTSMode::Polly => {
            i3 = data.polly_voices.values().map(|voice| {
                let name = format!(
                    "{} - {} ({})",
                    voice.name, voice.language_name, voice.gender
                );

                (name, voice.id.to_string())
            });
            &mut i3
        }
        TTSMode::gCloud => {
            i4 = data.gcloud_voices.iter().flat_map(|(language, variants)| {
                variants.iter().map(move |(variant, gender)| {
                    (
                        format!("{language} {variant} ({gender})"),
                        format!("{language} {variant}"),
                    )
                })
            });
            &mut i4
        }
    };

    let searching_lower = searching.to_lowercase();
    let mut voices: Vec<_> = voices
        .map(|(label, value)| (label.to_lowercase(), label, value))
        .collect();

    voices.sort_by_cached_key(|(l_lower, _, _)| strsim::levenshtein(l_lower, &searching_lower));
    voices.sort_by_key(|(l_lower, _, _)| !l_lower.contains(&searching_lower));
    voices
        .into_iter()
        .map(|(_, label, value)| serenity::AutocompleteChoice::new(label, value))
        .collect()
}

#[allow(clippy::unused_async)]
async fn translation_languages_autocomplete<'a>(
    ctx: ApplicationContext<'a>,
    searching: &'a str,
) -> impl Iterator<Item = serenity::AutocompleteChoice<'a>> {
    let mut filtered_languages = ctx
        .data()
        .translation_languages
        .iter()
        .filter(|(_, name)| name.starts_with(searching))
        .map(|(value, name)| (value.to_string(), name.to_string()))
        .collect::<Vec<_>>();

    filtered_languages.sort_by_key(|(label, _)| strsim::levenshtein(label, searching));
    filtered_languages
        .into_iter()
        .map(|(value, name)| serenity::AutocompleteChoice::new(name, value))
}

async fn bool_button(ctx: Context<'_>, value: Option<bool>) -> Result<Option<bool>, Error> {
    if let Some(value) = value {
        Ok(Some(value))
    } else {
        confirm_dialog(
            ctx,
            ctx.gettext("What would you like to set this to?"),
            ctx.gettext("True"),
            ctx.gettext("False"),
        )
        .await
    }
}

enum Target {
    Guild,
    User,
}

#[allow(clippy::too_many_arguments)]
async fn change_mode<'a, CacheKey, RowT>(
    ctx: &'a Context<'a>,
    general_db: &'a database::Handler<CacheKey, RowT>,
    guild_id: serenity::GuildId,
    identifier: CacheKey,
    mode: Option<TTSMode>,
    target: Target,
    guild_is_premium: bool,
) -> Result<Option<Cow<'a, str>>, Error>
where
    CacheKey: database::CacheKeyTrait + Default + Send + Sync + Copy,
    RowT: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Compact + Send + Sync + Unpin,
{
    let data = ctx.data();
    if let Some(mode) = mode
        && mode.is_premium()
        && !data.is_premium_simple(guild_id).await?
    {
        ctx.send(poise::CreateReply::default().embed(CreateEmbed::default()
            .title("TTS Bot Premium")
            .colour(PREMIUM_NEUTRAL_COLOUR)
            .thumbnail(data.premium_avatar_url.as_str())
            .url("https://www.patreon.com/Gnome_the_Bot_Maker")
            .footer(CreateEmbedFooter::new(ctx.gettext(
                "If this server has purchased premium, please run the `/premium_activate` command to link yourself to this server!"
            )))
            .description(ctx.gettext("
                The `{mode_name}` TTS Mode is only for TTS Bot Premium subscribers, please check out the `/premium` command!
            ").replace("{mode_name}", mode.into()))
        )).await?;
        Ok(None)
    } else {
        let key = if guild_is_premium {
            "premium_voice_mode"
        } else {
            "voice_mode"
        };

        general_db.set_one(identifier, key, &mode).await?;
        Ok(Some(match mode {
            Some(mode) => Cow::Owned(
                match target {
                    Target::Guild => ctx.gettext("Changed the server TTS Mode to: {mode}"),
                    Target::User => ctx.gettext("Changed your TTS Mode to: {mode}"),
                }
                .replace("{mode}", mode.into()),
            ),
            None => Cow::Borrowed(match target {
                Target::Guild => ctx.gettext("Reset the server mode"),
                Target::User => ctx.gettext("Reset your mode"),
            }),
        }))
    }
}

#[allow(clippy::too_many_arguments)]
async fn change_voice<'a, T, RowT1, RowT2>(
    ctx: &'a Context<'a>,
    general_db: &'a database::Handler<T, RowT1>,
    voice_db: &'a database::Handler<(T, TTSMode), RowT2>,
    author_id: serenity::UserId,
    guild_id: serenity::GuildId,
    key: T,
    voice: Option<FixedString<u8>>,
    target: Target,
) -> Result<Cow<'a, str>, Error>
where
    RowT1: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Compact + Send + Sync + Unpin,
    RowT2: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Compact + Send + Sync + Unpin,

    T: database::CacheKeyTrait + Default + Send + Sync + Copy,
    (T, TTSMode): database::CacheKeyTrait,
{
    let data = ctx.data();
    let (_, mode) = data.parse_user_or_guild(author_id, Some(guild_id)).await?;
    Ok(if let Some(voice) = voice {
        if check_valid_voice(&data, &voice, mode) {
            general_db.create_row(key).await?;
            voice_db
                .set_one((key, mode), "voice", voice.as_str())
                .await?;

            let name = get_voice_name(&data, &voice, mode).unwrap_or(&voice);
            Cow::Owned(
                match target {
                    Target::Guild => ctx.gettext("Changed the server voice to: {voice}"),
                    Target::User => ctx.gettext("Changed your voice to {voice}"),
                }
                .replace("{voice}", name),
            )
        } else {
            Cow::Borrowed(ctx.gettext("Invalid voice, do `/voices`"))
        }
    } else {
        voice_db.delete((key, mode)).await?;
        Cow::Borrowed(match target {
            Target::Guild => ctx.gettext("Reset the server voice"),
            Target::User => ctx.gettext("Reset your voice"),
        })
    })
}

fn format_languages<'a>(mut iter: impl Iterator<Item = &'a FixedString<u8>>) -> String {
    let mut buf = String::with_capacity(iter.size_hint().0 * 2);
    if let Some(first_elt) = iter.next() {
        buf.push('`');
        buf.push_str(first_elt);
        buf.push('`');
        for elt in iter {
            buf.push_str(", `");
            buf.push_str(elt);
            buf.push('`');
        }
    };

    buf
}

fn get_voice_name<'a>(data: &'a Data, code: &str, mode: TTSMode) -> Option<&'a FixedString<u8>> {
    match mode {
        TTSMode::gTTS => data.gtts_voices.get(code),
        TTSMode::Polly => data.polly_voices.get(code).map(|n| &n.name),
        TTSMode::eSpeak | TTSMode::gCloud => None,
    }
}

fn check_valid_voice(data: &Data, code: &FixedString<u8>, mode: TTSMode) -> bool {
    match mode {
        TTSMode::gTTS | TTSMode::Polly => get_voice_name(data, code, mode).is_some(),
        TTSMode::eSpeak => data.espeak_voices.contains(code),
        TTSMode::gCloud => code
            .split_once(' ')
            .and_then(|(language, variant)| data.gcloud_voices.get(language).map(|l| (l, variant)))
            .is_some_and(|(ls, v)| ls.contains_key(v)),
    }
}

fn check_prefix<'a>(ctx: &'a Context<'_>, prefix: &str) -> Result<(), &'a str> {
    if prefix.len() <= 5 && prefix.matches(' ').count() <= 1 {
        Ok(())
    } else {
        Err(ctx.gettext(
            "**Error**: Invalid Prefix, please use 5 or less characters with maximum 1 space",
        ))
    }
}

/// Changes a setting!
#[poise::command(
    category = "Settings",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn set(ctx: Context<'_>) -> CommandResult {
    super::help::command(ctx, Some("set")).await
}

/// Owner only: used to block a user from dms
#[poise::command(
    prefix_command,
    category = "Settings",
    owners_only,
    hide_in_help,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn block(ctx: Context<'_>, user: serenity::UserId, value: bool) -> CommandResult {
    ctx.data()
        .userinfo_db
        .set_one(user.into(), "dm_blocked", &value)
        .await?;

    ctx.say(ctx.gettext("Done!")).await?;
    Ok(())
}

/// Owner only: used to block a user from dms
#[poise::command(
    prefix_command,
    category = "Settings",
    owners_only,
    hide_in_help,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn bot_ban(ctx: Context<'_>, user: serenity::UserId, value: bool) -> CommandResult {
    let user_id = user.into();
    let userinfo_db = &ctx.data().userinfo_db;

    userinfo_db.set_one(user_id, "bot_banned", &value).await?;
    if value {
        userinfo_db.set_one(user_id, "dm_blocked", &true).await?;
    }

    let msg = format!("Set bot ban status for user {user} to `{value}`.");
    ctx.say(msg).await?;

    Ok(())
}

fn replace_bool(ctx: Context<'_>, original: &str, value: bool) -> String {
    ctx.gettext(original).replace(
        "{}",
        if value {
            ctx.gettext("Enabled")
        } else {
            ctx.gettext("Disabled")
        },
    )
}

async fn generic_bool_command(
    ctx: Context<'_>,
    key: &'static str,
    value: Option<bool>,
    resp: &'static str,
) -> CommandResult {
    let value = require!(bool_button(ctx, value).await?, Ok(()));

    let guilds_db = &ctx.data().guilds_db;
    let guild_id = ctx.guild_id().unwrap();

    guilds_db.set_one(guild_id.into(), key, &value).await?;
    ctx.say(replace_bool(ctx, resp, value)).await?;

    Ok(())
}

macro_rules! create_bool_command {
    (
        $description:literal,
        $value_desc:literal,
        $name:ident,
        $key:literal,
        gettext($resp:literal),
        aliases($( $aliases:literal ),*),
        $($extra:tt)*
    ) => {
        pub fn $name() -> Command {
            #[poise::command(prefix_command)]
            pub async fn prefix_bool(ctx: Context<'_>, value: Option<bool>) -> CommandResult {
                generic_bool_command(ctx, $key, value, $resp).await
            }

            #[doc=$description]
            #[poise::command(
                category="Settings",
                aliases($($aliases,)*),
                guild_only, slash_command,
                required_permissions="ADMINISTRATOR",
                required_bot_permissions="SEND_MESSAGES",
                $($extra)*
            )]
            pub async fn slash_bool(ctx: Context<'_>, #[description=$value_desc] value: bool) -> CommandResult {
                generic_bool_command(ctx, $key, Some(value), $resp).await
            }

            Command {
                prefix_action: prefix_bool().prefix_action,
                name: String::from(stringify!($name)),
                ..slash_bool()
            }
        }
    }
}

create_bool_command!(
    "Makes the bot say \"<user> said\" before each message",
    "Whether to say \"<user> said\" before each message",
    xsaid,
    "xsaid",
    gettext("xsaid is now: {}"),
    aliases(),
);
create_bool_command!(
    "Makes the bot join the voice channel automatically when a message is sent in the setup channel",
    "Whether to automatically join voice channels",
    autojoin, "auto_join",
    gettext("Auto Join is now: {}"), aliases("auto_join"),
);
create_bool_command!(
    "Makes the bot ignore messages sent by bots and webhooks",
    "Whether to ignore messages sent by bots and webhooks",
    botignore,
    "bot_ignore",
    gettext("Ignoring bots is now: {}"),
    aliases("bot_ignore", "ignore_bots", "ignorebots"),
);
create_bool_command!(
    "Makes the bot require people to be in the voice channel to TTS",
    "Whether to require people to be in the voice channel to TTS",
    require_voice,
    "require_voice",
    gettext("Requiring users to be in voice channel for TTS is now: {}"),
    aliases("voice_require", "require_in_vc"),
);
create_bool_command!(
    "Makes the bot ignore messages sent by members of the audience in stage channels",
    "Whether to ignore messages sent by the audience",
    audience_ignore,
    "audience_ignore",
    gettext("Ignoring audience is now: {}"),
    aliases("audienceignore", "ignore_audience", "ignoreaudience"),
);
create_bool_command!(
    "Whether to use DeepL translate to translate all TTS messages to the same language ",
    "Whether to translate all messages to the same language",
    translation,
    "to_translate",
    gettext("Translation is now: {}"),
    aliases("translate", "to_translate", "should_translate"),
    check = "crate::premium_command_check",
);

/// Enables the experimental new message formatting
#[poise::command(
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
async fn use_new_formatting(
    ctx: Context<'_>,
    #[description = "Whether to use the experimental new message formatting"] value: bool,
) -> CommandResult {
    let id = ctx.author().id.into();
    let userinfo = &ctx.data().userinfo_db;

    userinfo.set_one(id, "use_new_formatting", value).await?;

    let resp = ctx.gettext("Experimental new message formatting is now: {}");
    ctx.say(replace_bool(ctx, resp, value)).await?;
    Ok(())
}

/// Changes the required role to use the bot.
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES",
    aliases("required_role", "require_role")
)]
pub async fn required_role(
    ctx: Context<'_>,
    #[description = "The required role for all bot usage"] required_role: Option<serenity::Role>,
) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let cache = ctx.cache();
    let data = ctx.data();

    let currently_required_role = data
        .guilds_db
        .get(guild_id.into())
        .await?
        .required_role
        .and_then(|r| {
            ctx.guild()
                .and_then(|g| g.roles.get(&r).map(|r| r.name.clone()))
        });

    let response = {
        let current_user = cache.current_user();
        if required_role.is_some() {
            Some(
                if let Some(currently_required_role) = currently_required_role {
                    (
                        ctx.gettext("Are you sure you want to change the required role?"),
                        ctx.gettext("No, keep {role_name} as the required role.")
                            .replace("{role_name}", &currently_required_role),
                    )
                } else {
                    (
                        ctx.gettext("Are you sure you want to set the required role?"),
                        ctx.gettext("No, keep {bot_name} usable by everyone.")
                            .replace("{bot_name}", &current_user.name),
                    )
                },
            )
        } else if let Some(currently_required_role) = currently_required_role {
            Some((
                ctx.gettext("Are you sure you want to remove the required role?"),
                ctx.gettext("No, keep {bot_name} restricted to {role_name}.")
                    .replace("{bot_name}", &current_user.name)
                    .replace("{role_name}", &currently_required_role),
            ))
        } else {
            None
        }
    };

    let (question, negative) = require!(response, {
        ctx.say("**Error:** Cannot reset the required role if there isn't one set!")
            .await?;
        Ok(())
    });

    if require!(
        confirm_dialog(ctx, question, ctx.gettext("Yes, I'm sure."), &negative).await?,
        Ok(())
    ) {
        ctx.data()
            .guilds_db
            .set_one(
                guild_id.into(),
                "required_role",
                &required_role.as_ref().map(|r| r.id.get() as i64),
            )
            .await?;
        ctx.say({
            let current_user = cache.current_user();
            if let Some(required_role) = required_role {
                ctx.gettext("{bot_name} now requires {required_role} to use.")
                    .replace("{required_role}", &required_role.mention().to_string())
                    .replace("{bot_name}", &current_user.name)
            } else {
                ctx.gettext("{bot_name} is now usable by everyone!")
                    .replace("{bot_name}", &current_user.name)
            }
        })
        .await
    } else {
        ctx.say(ctx.gettext("Cancelled!")).await
    }?;

    Ok(())
}

/// Changes the required prefix for TTS.
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES",
    aliases("required_role", "require_role")
)]
async fn required_prefix(
    ctx: Context<'_>,
    #[description = "The required prefix for TTS"] mut tts_prefix: Option<String>,
) -> CommandResult {
    if let Some(prefix) = &tts_prefix
        && let Err(err) = check_prefix(&ctx, prefix)
    {
        ctx.say(err).await?;
        return Ok(());
    }

    // Fix up some people being a little silly.
    let mistakes = ["none", "null", "true", "false"];
    if tts_prefix.as_deref().is_some_and(|p| mistakes.contains(&p)) {
        tts_prefix = None;
    }

    let guild_id = ctx.guild_id().unwrap();
    ctx.data()
        .guilds_db
        .set_one(guild_id.into(), "required_prefix", &tts_prefix)
        .await?;

    let msg = if let Some(tts_prefix) = tts_prefix {
        Cow::Owned(
            ctx.gettext("The required prefix for TTS is now: {}")
                .replace("{}", &tts_prefix),
        )
    } else {
        Cow::Borrowed(ctx.gettext("Reset your required prefix."))
    };

    ctx.say(msg).await?;
    Ok(())
}

/// Changes the default mode for TTS that messages are read in
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS",
    aliases("server_voice_mode", "server_tts_mode", "server_ttsmode")
)]
pub async fn server_mode(
    ctx: Context<'_>,
    #[description = "The TTS Mode to change to"] mode: TTSModeChoice,
) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();

    let data = ctx.data();
    let to_send = change_mode(
        &ctx,
        &data.guilds_db,
        guild_id,
        guild_id.into(),
        Some(TTSMode::from(mode)),
        Target::Guild,
        false,
    )
    .await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the default language messages are read in
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES",
    aliases(
        "defaultlang",
        "default_lang",
        "defaultlang",
        "slang",
        "serverlanguage"
    )
)]
pub async fn server_voice(
    ctx: Context<'_>,
    #[description = "The default voice to read messages in"]
    #[autocomplete = "voice_autocomplete"]
    #[rest]
    voice: FixedString<u8>,
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_voice(
        &ctx,
        &data.guilds_db,
        &data.guild_voice_db,
        ctx.author().id,
        guild_id,
        guild_id.into(),
        Some(voice),
        Target::Guild,
    )
    .await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the target language for translation
#[poise::command(
    guild_only,
    category = "Settings",
    check = "crate::premium_command_check",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS",
    aliases("tlang", "tvoice", "target_lang", "target_voice", "target_language")
)]
pub async fn translation_lang(
    ctx: Context<'_>,
    #[description = "The language to translate all TTS messages to"]
    #[autocomplete = "translation_languages_autocomplete"]
    target_lang: Option<String>,
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap().into();

    let to_say = if target_lang.as_ref().map_or(true, |target_lang| {
        data.translation_languages
            .contains_key(target_lang.as_str())
    }) {
        data.guilds_db
            .set_one(guild_id, "target_lang", &target_lang)
            .await?;
        if let Some(target_lang) = target_lang {
            let mut to_say = ctx
                .gettext("The target translation language is now: `{}`")
                .replace("{}", &target_lang);
            if !data.guilds_db.get(guild_id).await?.to_translate() {
                to_say.push_str(
                    ctx.gettext("\nYou may want to enable translation with `/set translation on`"),
                );
            };

            Cow::Owned(to_say)
        } else {
            Cow::Borrowed(ctx.gettext("Reset the target translation language"))
        }
    } else {
        Cow::Borrowed(ctx.gettext("Invalid translation language, do `/translation_languages`"))
    };

    ctx.say(to_say).await?;
    Ok(())
}

/// Changes the prefix used before commands
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn command_prefix(
    ctx: Context<'_>,
    #[description = "The prefix to be used before commands"]
    #[rest]
    prefix: FixedString<u8>,
) -> CommandResult {
    let to_send = if let Err(err) = check_prefix(&ctx, &prefix) {
        Cow::Borrowed(err)
    } else {
        ctx.data()
            .guilds_db
            .set_one(ctx.guild_id().unwrap().into(), "prefix", prefix.as_str())
            .await?;
        Cow::Owned(
            ctx.gettext("Command prefix for this server is now: {prefix}")
                .replace("{prefix}", &prefix),
        )
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the max repetion of a character (0 = off)
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES",
    aliases("repeated_chars", "repeated_letters", "chars")
)]
pub async fn repeated_characters(
    ctx: Context<'_>,
    #[description = "The max repeated characters"] chars: u8,
) -> CommandResult {
    let to_send = {
        if chars > 100 {
            Cow::Borrowed(
                ctx.gettext("**Error**: Cannot set the max repeated characters above 100"),
            )
        } else if chars < 5 && chars != 0 {
            Cow::Borrowed(ctx.gettext("**Error**: Cannot set the max repeated characters below 5"))
        } else {
            ctx.data()
                .guilds_db
                .set_one(
                    ctx.guild_id().unwrap().into(),
                    "repeated_chars",
                    &(chars as i16),
                )
                .await?;
            Cow::Owned(
                ctx.gettext("Max repeated characters is now: {}")
                    .replace("{}", &chars.to_arraystring()),
            )
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the max length of a TTS message in seconds
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES",
    aliases("max_length", "message_length")
)]
pub async fn msg_length(
    ctx: Context<'_>,
    #[description = "Max length of TTS message in seconds"] seconds: u8,
) -> CommandResult {
    let to_send = {
        if seconds > 60 {
            Cow::Borrowed(
                ctx.gettext("**Error**: Cannot set the max length of messages above 60 seconds"),
            )
        } else if seconds < 10 {
            Cow::Borrowed(
                ctx.gettext("**Error**: Cannot set the max length of messages below 10 seconds"),
            )
        } else {
            ctx.data()
                .guilds_db
                .set_one(
                    ctx.guild_id().unwrap().into(),
                    "msg_length",
                    &(seconds as i16),
                )
                .await?;
            Cow::Owned(
                ctx.gettext("Max message length is now: {} seconds")
                    .replace("{}", &seconds.to_arraystring()),
            )
        }
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the multiplier for how fast to speak
#[poise::command(
    category = "Settings",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES",
    aliases(
        "speed",
        "speed_multiplier",
        "speaking_rate_multiplier",
        "speaking_speed",
        "tts_speed"
    )
)]
pub async fn speaking_rate(
    ctx: Context<'_>,
    #[description = "The speed to speak at"]
    #[min = 0]
    #[max = 400.0]
    speaking_rate: f32,
) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();

    let (_, mode) = data.parse_user_or_guild(author.id, ctx.guild_id()).await?;
    let SpeakingRateInfo {
        min,
        max,
        default: _,
        kind,
    } = require!(mode.speaking_rate_info(), {
        ctx.say(
            ctx.gettext("**Error**: Cannot set speaking rate for the {mode} mode")
                .replace("{mode}", mode.into()),
        )
        .await?;
        Ok(())
    });

    let to_send = {
        if speaking_rate > max {
            ctx.gettext("**Error**: Cannot set the speaking rate multiplier above {max}{kind}")
                .replace("{max}", &max.to_arraystring())
        } else if speaking_rate < min {
            ctx.gettext("**Error**: Cannot set the speaking rate multiplier below {min}{kind}")
                .replace("{min}", &min.to_arraystring())
        } else {
            data.userinfo_db.create_row(author.id.get() as i64).await?;
            data.user_voice_db
                .set_one(
                    (author.id.get() as i64, mode),
                    "speaking_rate",
                    &speaking_rate,
                )
                .await?;
            ctx.gettext("Your speaking rate is now: {speaking_rate}{kind}")
                .replace("{speaking_rate}", &speaking_rate.to_arraystring())
        }
    }
    .replace("{kind}", kind);

    ctx.say(to_send).await?;
    Ok(())
}

/// Replaces your username in "<user> said" with a given name
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES",
    aliases("nick_name", "nickname", "name")
)]
pub async fn nick(
    ctx: Context<'_>,
    #[description = "The user to set the nick for, defaults to you"] user: Option<serenity::User>,
    #[description = "The nickname to set, leave blank to reset"]
    #[rest]
    nickname: Option<String>,
) -> CommandResult {
    let author = ctx.author();
    let guild_id = ctx.guild_id().unwrap();
    let user = user.map_or(Cow::Borrowed(author), Cow::Owned);

    if author.id != user.id
        && !guild_id
            .member(ctx, author.id)
            .await?
            .permissions(ctx.cache())?
            .administrator()
    {
        ctx.say(ctx.gettext("**Error**: You need admin to set other people's nicknames!"))
            .await?;
        return Ok(());
    }

    let data = ctx.data();

    let to_send = if let Some(nick) = nickname {
        if nick.contains('<') && nick.contains('>') {
            Cow::Borrowed(
                ctx.gettext("**Error**: You can't have mentions/emotes in your nickname!"),
            )
        } else {
            tokio::try_join!(
                data.guilds_db.create_row(guild_id.into()),
                data.userinfo_db.create_row(user.id.into())
            )?;

            data.nickname_db
                .set_one([guild_id.into(), user.id.into()], "name", &nick)
                .await?;
            Cow::Owned(
                ctx.gettext("Changed {user}'s nickname to {new_nick}")
                    .replace("{user}", &user.name)
                    .replace("{new_nick}", &nick),
            )
        }
    } else {
        data.nickname_db
            .delete([guild_id.into(), user.id.into()])
            .await?;
        Cow::Owned(
            ctx.gettext("Reset {user}'s nickname")
                .replace("{user}", &user.name),
        )
    };

    ctx.say(to_send).await?;
    Ok(())
}

/// Changes the voice mode that messages are read in for you
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS",
    aliases("voice_mode", "tts_mode", "ttsmode")
)]
pub async fn mode(
    ctx: Context<'_>,
    #[description = "The TTS Mode to change to, leave blank for server default"] mode: Option<
        TTSModeChoice,
    >,
) -> CommandResult {
    let data = ctx.data();
    let author_id = ctx.author().id.into();
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_mode(
        &ctx,
        &data.userinfo_db,
        guild_id,
        author_id,
        mode.map(TTSMode::from),
        Target::User,
        data.is_premium_simple(guild_id).await?,
    )
    .await?;

    if let Some(to_send) = to_send {
        ctx.say(to_send).await?;
    };
    Ok(())
}

/// Changes the voice your messages are read in, full list in `-voices`
#[poise::command(
    guild_only,
    category = "Settings",
    aliases("language", "voice"),
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn voice(
    ctx: Context<'_>,
    #[description = "The voice to read messages in, leave blank to reset"]
    #[autocomplete = "voice_autocomplete"]
    #[rest]
    voice: Option<String>,
) -> CommandResult {
    let data = ctx.data();
    let author_id = ctx.author().id;
    let guild_id = ctx.guild_id().unwrap();

    let to_send = change_voice(
        &ctx,
        &data.userinfo_db,
        &data.user_voice_db,
        author_id,
        guild_id,
        author_id.into(),
        voice.map(String::trunc_into),
        Target::User,
    )
    .await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Lists all the languages that TTS bot accepts for DeepL translation
#[poise::command(
    category = "Settings",
    prefix_command,
    slash_command,
    aliases("trans_langs", "translation_langs"),
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn translation_languages(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();
    let neutral_colour = ctx.neutral_colour().await;

    let (embed_title, client_id) = {
        let current_user = ctx.cache().current_user();
        (
            ctx.gettext("{} Translation Languages")
                .replace("{}", &current_user.name),
            current_user.id,
        )
    };

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::default()
                .title(embed_title)
                .colour(neutral_colour)
                .field(
                    "Currently Supported Languages",
                    format_languages(data.translation_languages.keys()),
                    false,
                )
                .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
                .footer(CreateEmbedFooter::new(random_footer(
                    &data.config.main_server_invite,
                    client_id,
                    ctx.current_catalog(),
                ))),
        ),
    )
    .await?;

    Ok(())
}

/// Lists all the voices that TTS bot accepts for the current mode
#[poise::command(
    category = "Settings",
    aliases("langs", "languages"),
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn voices(
    ctx: Context<'_>,
    #[description = "The mode to see the voices for, leave blank for current"] mode: Option<
        TTSModeChoice,
    >,
) -> CommandResult {
    let data = ctx.data();
    let cache = ctx.cache();
    let author = ctx.author();

    let mode = match mode {
        Some(mode) => TTSMode::from(mode),
        None => data.parse_user_or_guild(author.id, ctx.guild_id()).await?.1,
    };

    let voices = {
        let run_paginator = |current_voice, pages| async {
            let footer = random_footer(
                &data.config.main_server_invite,
                cache.current_user().id,
                ctx.current_catalog(),
            );

            let paginator = MenuPaginator::new(ctx, pages, current_voice, mode, footer);
            paginator.start().await?;
            Ok(())
        };

        match mode {
            TTSMode::gTTS => format_languages(data.gtts_voices.keys()),
            TTSMode::eSpeak => format_languages(data.espeak_voices.iter()),
            TTSMode::Polly => {
                let (current_voice, pages) = list_polly_voices(&ctx).await?;
                return run_paginator(current_voice, pages).await;
            }
            TTSMode::gCloud => {
                let (current_voice, pages) = list_gcloud_voices(&ctx).await?;
                return run_paginator(current_voice, pages).await;
            }
        }
    };

    let user_voice_row = data.user_voice_db.get((author.id.into(), mode)).await?;

    let (embed_title, client_id) = {
        let current_user = cache.current_user();
        let embed_title = ctx
            .gettext("{bot_user} Voices | Mode: `{mode}`")
            .replace("{bot_user}", &cache.current_user().name)
            .replace("{mode}", mode.into());

        (embed_title, current_user.id)
    };

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::default()
                .title(embed_title)
                .footer(CreateEmbedFooter::new(random_footer(
                    &data.config.main_server_invite,
                    client_id,
                    ctx.current_catalog(),
                )))
                .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
                .field(ctx.gettext("Currently supported voices"), &voices, true)
                .field(
                    ctx.gettext("Current voice used"),
                    user_voice_row
                        .voice
                        .as_ref()
                        .map_or_else(|| ctx.gettext("None"), std::ops::Deref::deref),
                    false,
                ),
        ),
    )
    .await?;

    Ok(())
}

async fn list_polly_voices(ctx: &Context<'_>) -> Result<(String, Vec<String>)> {
    let data = ctx.data();

    let (voice_id, mode) = data
        .parse_user_or_guild(ctx.author().id, ctx.guild_id())
        .await?;
    let voice = match mode {
        TTSMode::Polly => {
            let voice_id: &str = &voice_id;
            &data.polly_voices[voice_id]
        }
        _ => &data.polly_voices[TTSMode::Polly.default_voice()],
    };

    let mut lang_to_voices: HashMap<_, Vec<_>> = HashMap::new();
    for voice in data.polly_voices.values() {
        lang_to_voices
            .entry(&voice.language_name)
            .or_default()
            .push(voice);
    }

    let pages = lang_to_voices
        .into_values()
        .map(|voices| {
            let mut buf = String::with_capacity(voices.len() * 12);
            for voice in voices {
                writeln!(
                    buf,
                    "{} - {} ({})",
                    voice.id, voice.language_name, voice.gender
                )?;
            }

            anyhow::Ok(buf)
        })
        .collect::<Result<_>>()?;

    Ok((
        format!("{} - {} ({})", voice.id, voice.language_name, voice.gender),
        pages,
    ))
}

async fn list_gcloud_voices(ctx: &Context<'_>) -> Result<(String, Vec<String>)> {
    let data = ctx.data();

    let (lang_variant, mode) = data
        .parse_user_or_guild(ctx.author().id, ctx.guild_id())
        .await?;
    let (lang, variant) = match mode {
        TTSMode::gCloud => &lang_variant,
        _ => TTSMode::gCloud.default_voice(),
    }
    .split_once(' ')
    .unwrap();

    let pages = data
        .gcloud_voices
        .iter()
        .map(|(language, variants)| {
            let mut buf = String::with_capacity(variants.len() * 12);
            for (variant, gender) in variants {
                writeln!(buf, "{language} {variant} ({gender})")?;
            }

            anyhow::Ok(buf)
        })
        .collect::<Result<_>>()?;

    let gender = data.gcloud_voices[lang][variant];
    Ok((format!("{lang} {variant} ({gender})"), pages))
}

pub fn commands() -> [Command; 5] {
    [
        settings(),
        setup::setup(),
        voices(),
        translation_languages(),
        poise::Command {
            subcommands: vec![
                poise::Command {
                    name: String::from("channel"),
                    ..setup::setup()
                },
                xsaid(),
                autojoin(),
                required_role(),
                voice(),
                server_voice(),
                mode(),
                server_mode(),
                msg_length(),
                botignore(),
                translation(),
                translation_lang(),
                speaking_rate(),
                nick(),
                repeated_characters(),
                audience_ignore(),
                require_voice(),
                required_prefix(),
                command_prefix(),
                block(),
                bot_ban(),
                use_new_formatting(),
            ],
            ..set()
        },
    ]
}
