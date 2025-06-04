mod owner;
mod setup;
mod voice_paginator;

use std::{borrow::Cow, collections::HashMap, fmt::Write, sync::atomic::Ordering};

use aformat::{aformat, ToArrayString};
use arrayvec::ArrayString;

use poise::serenity_prelude as serenity;
use serenity::{builder::*, small_fixed_array::FixedString, Mentionable};

use tts_core::{
    common::{confirm_dialog, random_footer},
    constants::{GTTS_DISABLED_ERROR, OPTION_SEPERATORS, PREMIUM_NEUTRAL_COLOUR},
    database::{self, Compact},
    require_guild,
    structs::{
        ApplicationContext, Command, CommandResult, Context, Data, Error, Result, SpeakingRateInfo,
        TTSMode, TTSModeChoice,
    },
    traits::PoiseContextExt,
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
    let none_str = "none";

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let userinfo_row = data.userinfo_db.get(author_id.into()).await?;
    let nickname_row = data
        .nickname_db
        .get([guild_id.into(), author_id.into()])
        .await?;

    let channel_mention = if let Some(channel) = guild_row.channel
        && require_guild!(ctx).channels.contains_key(&channel)
    {
        &*channel.mention().to_arraystring()
    } else {
        none_str
    };

    let prefix = &guild_row.prefix;
    let guild_mode = guild_row.voice_mode;
    let nickname = nickname_row.name.as_deref().unwrap_or(none_str);
    let target_lang = guild_row.target_lang.as_deref().unwrap_or(none_str);
    let required_role = guild_row
        .required_role
        .map(|r| r.mention().to_arraystring());

    let user_mode = if data.is_premium_simple(ctx.http(), guild_id).await? {
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

        match user_voice_row.voice.as_ref() {
            Some(voice) => format_voice(&data, voice, currently_set_voice_mode),
            None => Cow::Borrowed(none_str),
        }
    };

    let (speaking_rate, speaking_rate_kind) = if let Some(mode) = user_mode {
        let user_voice_row = data.user_voice_db.get((author_id.into(), mode)).await?;
        let (default, kind) = match mode.speaking_rate_info() {
            Some(info) => (info.default, info.kind),
            None => ("1.0", "x"),
        };

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

    let xsaid = guild_row.xsaid();
    let autojoin = guild_row.auto_join();
    let msg_length = guild_row.msg_length;
    let bot_ignore = guild_row.bot_ignore();
    let skip_emoji = guild_row.skip_emoji();
    let guild_mode: &str = guild_mode.into();
    let to_translate = guild_row.to_translate();
    let require_voice = guild_row.require_voice();
    let text_in_voice = guild_row.text_in_voice();
    let audience_ignore = guild_row.audience_ignore();
    let voice_mode = user_mode.map(Into::into).unwrap_or(none_str);
    let role_mention = required_role.as_deref().unwrap_or(none_str);
    let required_prefix = guild_row.required_prefix.as_deref().unwrap_or(none_str);
    let repeated_chars = match guild_row.repeated_chars {
        Some(chars) => &chars.to_arraystring(),
        None => "Disabled",
    };

    ctx.send(poise::CreateReply::default().embed(CreateEmbed::default()
        .title("Current Settings")
        .colour(neutral_colour)
        .url(data.config.main_server_invite.as_str())
        .footer(CreateEmbedFooter::new(
            "Change these settings with `/set {property} {value}`!\nNone = setting has not been set yet!"
        ))
        .field("**General Server Settings**", format!("
{sep1} Setup Channel: {channel_mention}
{sep1} Required Role: {role_mention}
{sep1} Command Prefix: `{prefix}`
{sep1} Auto Join: `{autojoin}`"), false)
        .field("**TTS Settings**", format!("
{sep2} <User> said: message: `{xsaid}`
{sep2} Ignore bot's messages: `{bot_ignore}`
{sep2} Ignore audience messages: `{audience_ignore}`
{sep2} Require users in voice channel: `{require_voice}`
{sep2} Required prefix for TTS: `{required_prefix}`
{sep2} Read from Text in Voice channels: `{text_in_voice}`
{sep2} Skip emojis when reading messages: `{skip_emoji}`

**{sep2} Default Server Voice Mode: `{guild_mode}`**
**{sep2} Default Server Voice: `{default_voice}`**

{sep2} Max Time to Read: `{msg_length} seconds`
{sep2} Max Repeated Characters: `{repeated_chars}`
        "),        false)
        .field("**Translation Settings (Premium Only)**", format!("
{sep4} Translation: `{to_translate}`
{sep4} Translation Language: `{target_lang}`
        ")
        ,false)
        .field("**User Specific**", format!("
{sep3} Voice: `{user_voice}`
{sep3} Voice Mode: `{voice_mode}`
{sep3} Nickname: `{nickname}`
{sep3} Speaking Rate: `{speaking_rate}{speaking_rate_kind}`
        "),
        false)
    )).await?;

    Ok(())
}

async fn voice_autocomplete<'a>(
    ctx: ApplicationContext<'a>,
    searching: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    let data = ctx.data();
    let Ok((_, mode)) = data
        .parse_user_or_guild(
            ctx.http(),
            ctx.interaction.user.id,
            ctx.interaction.guild_id,
        )
        .await
    else {
        return serenity::CreateAutocompleteResponse::new();
    };

    let voices: &mut dyn Iterator<Item = _> = match mode {
        TTSMode::gTTS => &mut data
            .gtts_voices
            .iter()
            .map(|(k, v)| (v.to_string(), k.to_string())),
        TTSMode::eSpeak => &mut data
            .espeak_voices
            .iter()
            .map(|voice| (voice.to_string(), voice.to_string())),
        TTSMode::Polly => &mut data.polly_voices.values().map(|voice| {
            let name = format!(
                "{} - {} ({})",
                voice.name, voice.language_name, voice.gender
            );

            (name, voice.id.to_string())
        }),
        TTSMode::gCloud => &mut data.gcloud_voices.iter().flat_map(|(language, variants)| {
            variants.iter().map(move |(variant, gender)| {
                (
                    format!("{language} {variant} ({gender})"),
                    format!("{language} {variant}"),
                )
            })
        }),
    };

    let searching_lower = searching.to_lowercase();
    let mut voices: Vec<_> = voices
        .map(|(label, value)| (label.to_lowercase(), label, value))
        .collect();

    voices.sort_by_cached_key(|(l_lower, _, _)| strsim::levenshtein(l_lower, &searching_lower));
    voices.sort_by_key(|(l_lower, _, _)| !l_lower.contains(&searching_lower));

    serenity::CreateAutocompleteResponse::new().set_choices(
        voices
            .into_iter()
            .take(25)
            .map(|(_, label, value)| serenity::AutocompleteChoice::new(label, value))
            .collect::<Vec<_>>(),
    )
}

#[expect(clippy::unused_async)]
async fn translation_languages_autocomplete<'a>(
    ctx: ApplicationContext<'a>,
    searching: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    let data = ctx.serenity_context().data_ref::<Data>();
    let languages = data.translation_languages.iter();
    let mut filtered_languages: Vec<_> = languages
        .filter(|(_, name)| name.starts_with(searching))
        .collect();

    filtered_languages.sort_by_cached_key(|(label, _)| strsim::levenshtein(label, searching));
    serenity::CreateAutocompleteResponse::new().set_choices(
        filtered_languages
            .into_iter()
            .take(25)
            .map(|(value, name)| serenity::AutocompleteChoice::new(name, value.as_str()))
            .collect::<Vec<_>>(),
    )
}

async fn bool_button(ctx: Context<'_>, value: Option<bool>) -> Result<Option<bool>, Error> {
    if let Some(value) = value {
        Ok(Some(value))
    } else {
        confirm_dialog(ctx, "What would you like to set this to?", "True", "False").await
    }
}

enum Target {
    Guild,
    User,
}

async fn can_change_mode(
    ctx: &Context<'_>,
    mode: Option<TTSMode>,
    guild_is_premium: bool,
) -> Result<bool> {
    let data = ctx.data();
    let Some(mode) = mode else { return Ok(true) };

    if data.config.gtts_disabled.load(Ordering::Relaxed) && mode == TTSMode::gTTS {
        ctx.send_error(GTTS_DISABLED_ERROR).await?;
        return Ok(false);
    }

    if mode.is_premium() && !guild_is_premium {
        ctx.send(poise::CreateReply::default().embed(CreateEmbed::default()
            .title("TTS Bot Premium")
            .colour(PREMIUM_NEUTRAL_COLOUR)
            .thumbnail(data.premium_avatar_url.as_str())
            .footer(CreateEmbedFooter::new(
                "If this server has purchased premium, please run the `/premium_activate` command to link yourself to this server!"
            ))
            .description(aformat!("
                The `{mode}` TTS Mode is only for TTS Bot Premium subscribers, please check out the `/premium` command!
            ").as_str())
        )).await?;

        Ok(false)
    } else {
        Ok(true)
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
    let (_, mode) = data
        .parse_user_or_guild(ctx.http(), author_id, Some(guild_id))
        .await?;
    Ok(if let Some(voice) = voice {
        if check_valid_voice(&data, &voice, mode) {
            general_db.create_row(key).await?;
            voice_db
                .set_one((key, mode), "voice", voice.as_str())
                .await?;

            let name = get_voice_name(&data, &voice, mode).unwrap_or(&voice);
            Cow::Owned(match target {
                Target::Guild => format!("Changed the server voice to: {name}"),
                Target::User => format!("Changed your voice to {name}"),
            })
        } else {
            Cow::Borrowed("Invalid voice, do `/voices`")
        }
    } else {
        voice_db.delete((key, mode)).await?;
        Cow::Borrowed(match target {
            Target::Guild => "Reset the server voice",
            Target::User => "Reset your voice",
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
    }

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

fn check_prefix(prefix: &str) -> Result<ArrayString<5>, &'static str> {
    if prefix.len() <= 5 && prefix.matches(' ').count() <= 1 {
        Ok(ArrayString::from(prefix).unwrap())
    } else {
        Err("**Error**: Invalid Prefix, please use 5 or less characters with maximum 1 space")
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

fn replace_bool(original: &str, value: bool) -> String {
    original.replace("{}", if value { "enabled" } else { "disabled" })
}

async fn generic_bool_command(
    ctx: Context<'_>,
    key: &'static str,
    value: Option<bool>,
    resp: &'static str,
) -> CommandResult {
    let Some(value) = bool_button(ctx, value).await? else {
        return Ok(());
    };

    let guilds_db = &ctx.data().guilds_db;
    let guild_id = ctx.guild_id().unwrap();

    guilds_db.set_one(guild_id.into(), key, &value).await?;
    ctx.say(replace_bool(resp, value)).await?;

    Ok(())
}

macro_rules! create_bool_command {
    (
        $description:literal,
        $name:ident,
        $key:literal,
        aliases($( $aliases:literal ),*),
        $($extra:tt)*
    ) => {
        pub fn $name() -> Command {
            const RESPONSE: &str = concat!("The setting `", $key, "` is now {}.");
            #[poise::command(prefix_command)]
            pub async fn prefix_bool(ctx: Context<'_>, value: Option<bool>) -> CommandResult {
                generic_bool_command(ctx, $key, value, RESPONSE).await
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
            pub async fn slash_bool(ctx: Context<'_>, #[description = "True or False?"] value: bool) -> CommandResult {
                generic_bool_command(ctx, $key, Some(value), RESPONSE).await
            }

            Command {
                prefix_action: prefix_bool().prefix_action,
                name: Cow::Borrowed(stringify!($name)),
                ..slash_bool()
            }
        }
    }
}

create_bool_command!(
    "Makes the bot say \"<user> said\" before each message",
    xsaid,
    "xsaid",
    aliases(),
);
create_bool_command!(
    "Makes the bot join the voice channel automatically when a message is sent in the setup channel",
    autojoin,
    "auto_join",
    aliases("auto_join"),
);
create_bool_command!(
    "Makes the bot ignore messages sent by bots and webhooks",
    botignore,
    "bot_ignore",
    aliases("bot_ignore", "ignore_bots", "ignorebots"),
);
create_bool_command!(
    "Makes the bot require people to be in the voice channel to TTS",
    require_voice,
    "require_voice",
    aliases("voice_require", "require_in_vc"),
);
create_bool_command!(
    "Makes the bot ignore messages sent by members of the audience in stage channels",
    audience_ignore,
    "audience_ignore",
    aliases("audienceignore", "ignore_audience", "ignoreaudience"),
);
create_bool_command!(
    "Makes the bot read messages from text-in-voice channels",
    text_in_voice,
    "text_in_voice",
    aliases(),
);
create_bool_command!(
    "Makes the bot skip emoji within messages",
    skip_emoji,
    "skip_emoji",
    aliases("skip_emojis"),
);
create_bool_command!(
    "Makes the bot translate all TTS messages to the same language",
    translation,
    "to_translate",
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

    let resp = "Experimental new message formatting is now: {}";
    ctx.say(replace_bool(resp, value)).await?;
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
                        "Are you sure you want to change the required role?",
                        format!("No, keep {currently_required_role} as the required role."),
                    )
                } else {
                    (
                        "Are you sure you want to set the required role?",
                        format!("No, keep {} usable by everyone.", current_user.name),
                    )
                },
            )
        } else if let Some(currently_required_role) = currently_required_role {
            Some((
                "Are you sure you want to remove the required role?",
                format!(
                    "No, keep {} restricted to {currently_required_role}.",
                    current_user.name
                ),
            ))
        } else {
            None
        }
    };

    let Some((question, negative)) = response else {
        let msg = "**Error:** Cannot reset the required role if there isn't one set!";
        ctx.say(msg).await?;
        return Ok(());
    };

    let Some(response) = confirm_dialog(ctx, question, "Yes, I'm sure.", &negative).await? else {
        return Ok(());
    };

    if response {
        ctx.data()
            .guilds_db
            .set_one(
                guild_id.into(),
                "required_role",
                &required_role.as_ref().map(|r| r.id.get() as i64),
            )
            .await?;

        let msg: &str = {
            let bot_name = &cache.current_user().name;
            if let Some(required_role) = required_role {
                &aformat!(
                    "{bot_name} now requires {} to use.",
                    required_role.mention()
                )
            } else {
                &aformat!("{bot_name} is now usable by everyone!")
            }
        };

        ctx.say(msg).await
    } else {
        ctx.say("Cancelled!").await
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
    #[description = "The required prefix for TTS"] tts_prefix: Option<String>,
) -> CommandResult {
    // Fix up some people being a little silly.
    let mistakes = ["none", "null", "true", "false"];
    let prefix = match tts_prefix.as_deref().map(check_prefix) {
        None => None,
        Some(Ok(p)) if mistakes.contains(&p.as_str()) => None,
        Some(Ok(p)) => Some(p),
        Some(Err(err)) => {
            ctx.say(err).await?;
            return Ok(());
        }
    };

    let guild_id = ctx.guild_id().unwrap();
    ctx.data()
        .guilds_db
        .set_one(guild_id.into(), "required_prefix", &tts_prefix)
        .await?;

    let msg = if let Some(tts_prefix) = prefix {
        &aformat!("The required prefix for TTS is now: {tts_prefix}")
    } else {
        "Reset your required prefix."
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
    #[description = "The TTS Mode to change to"] mode: Option<TTSModeChoice>,
) -> CommandResult {
    let data = ctx.data();
    let guild_id = ctx.guild_id().unwrap();

    let mode = mode.map(TTSMode::from);
    let guild_is_premium = data.is_premium_simple(ctx.http(), guild_id).await?;
    if !can_change_mode(&ctx, mode, guild_is_premium).await? {
        return Ok(());
    }

    data.guilds_db
        .set_one(guild_id.into(), "voice_mode", mode)
        .await?;

    let response = if let Some(mode) = mode {
        &aformat!("Set your server's voice mode to: {mode}")
    } else {
        "Reset your server's voice mode"
    };

    ctx.say(response).await?;
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

    let to_say = if target_lang.as_ref().is_none_or(|target_lang| {
        data.translation_languages
            .contains_key(target_lang.as_str())
    }) {
        data.guilds_db
            .set_one(guild_id, "target_lang", &target_lang)
            .await?;
        if let Some(target_lang) = target_lang {
            let mut to_say = format!("The target translation language is now: `{target_lang}`");
            if !data.guilds_db.get(guild_id).await?.to_translate() {
                to_say.push_str("\nYou may want to enable translation with `/set translation on`");
            }

            Cow::Owned(to_say)
        } else {
            Cow::Borrowed("Reset the target translation language")
        }
    } else {
        Cow::Borrowed("Invalid translation language, do `/translation_languages`")
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
    let to_send = match check_prefix(&prefix) {
        Err(err) => err,
        Ok(prefix) => {
            ctx.data()
                .guilds_db
                .set_one(ctx.guild_id().unwrap().into(), "prefix", prefix.as_str())
                .await?;

            &aformat!("Command prefix for this server is now: {prefix}")
        }
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
    let to_send = if chars > 100 {
        "**Error**: Cannot set the max repeated characters above 100"
    } else if chars < 5 && chars != 0 {
        "**Error**: Cannot set the max repeated characters below 5"
    } else {
        let guild_id = ctx.guild_id().unwrap().into();
        ctx.data()
            .guilds_db
            .set_one(guild_id, "repeated_chars", &(chars as i16))
            .await?;

        &aformat!("Max repeated characters is now: {chars}")
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
    let to_send = if seconds > 60 {
        "**Error**: Cannot set the max length of messages above 60 seconds"
    } else if seconds < 10 {
        "**Error**: Cannot set the max length of messages below 10 seconds"
    } else {
        ctx.data()
            .guilds_db
            .set_one(
                ctx.guild_id().unwrap().into(),
                "msg_length",
                &(seconds as i16),
            )
            .await?;

        &aformat!("Max message length is now: {seconds} seconds")
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

    let (_, mode) = data
        .parse_user_or_guild(ctx.http(), author.id, ctx.guild_id())
        .await?;
    let Some(speaking_rate_info) = mode.speaking_rate_info() else {
        let msg = aformat!("**Error**: Cannot set speaking rate for the {mode} mode");
        ctx.say(&*msg).await?;
        return Ok(());
    };

    let kind = speaking_rate_info.kind();
    let SpeakingRateInfo { min, max, .. } = speaking_rate_info;
    let to_send: &str = if speaking_rate > max {
        &aformat!("**Error**: Cannot set the speaking rate multiplier above {max}{kind}")
    } else if speaking_rate < min {
        &aformat!("**Error**: Cannot set the speaking rate multiplier below {min}{kind}")
    } else {
        data.userinfo_db.create_row(author.id.get() as i64).await?;
        data.user_voice_db
            .set_one(
                (author.id.get() as i64, mode),
                "speaking_rate",
                &speaking_rate,
            )
            .await?;

        &aformat!("Your speaking rate is now: {speaking_rate}{kind}")
    };

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
    let user = user.as_ref().unwrap_or(author);

    if author.id != user.id && !ctx.author_permissions()?.manage_nicknames() {
        ctx.say("**Error**: You need permission to set other people's nicknames!")
            .await?;
        return Ok(());
    }

    let data = ctx.data();

    let to_send = if let Some(nick) = nickname {
        if nick.contains('<') && nick.contains('>') {
            "**Error**: You can't have mentions/emotes in your nickname!"
        } else {
            tokio::try_join!(
                data.guilds_db.create_row(guild_id.into()),
                data.userinfo_db.create_row(user.id.into())
            )?;

            data.nickname_db
                .set_one([guild_id.into(), user.id.into()], "name", &nick)
                .await?;

            &format!("Changed {}'s nickname to {nick}", user.name)
        }
    } else {
        data.nickname_db
            .delete([guild_id.into(), user.id.into()])
            .await?;

        &aformat!("Reset {}'s nickname", &user.name)
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
    let guild_id = ctx.guild_id().unwrap();

    let mode = mode.map(TTSMode::from);
    let guild_is_premium = data.is_premium_simple(ctx.http(), guild_id).await?;
    if !can_change_mode(&ctx, mode, guild_is_premium).await? {
        return Ok(());
    }

    let key = if guild_is_premium {
        "premium_voice_mode"
    } else {
        "voice_mode"
    };

    data.userinfo_db
        .set_one(ctx.author().id.into(), key, mode)
        .await?;

    let response = if let Some(mode) = mode {
        &aformat!("Set your voice mode to: {mode}")
    } else {
        "Reset your voice mode"
    };

    ctx.say(response).await?;
    Ok(())
}

/// Changes the voice your messages are read in, full list in `/voices`
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
    voice: Option<FixedString<u8>>,
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
        voice,
        Target::User,
    )
    .await?;

    ctx.say(to_send).await?;
    Ok(())
}

/// Lists all the languages that TTS bot accepts for Deepl translation
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
            &aformat!("{} Translation Languages", &current_user.name),
            current_user.id,
        )
    };

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::default()
                .title(embed_title.as_str())
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
    let http = ctx.http();
    let cache = ctx.cache();
    let author = ctx.author();
    let guild_id = ctx.guild_id();

    let mode = match mode {
        Some(mode) => TTSMode::from(mode),
        None => data.parse_user_or_guild(http, author.id, guild_id).await?.1,
    };

    let voices = {
        let run_paginator = |current_voice, pages| async {
            let footer = random_footer(&data.config.main_server_invite, cache.current_user().id);

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
        let embed_title = aformat!("{} Voices | Mode: `{mode}`", &cache.current_user().name);

        (embed_title, current_user.id)
    };

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::default()
                .title(embed_title.as_str())
                .footer(CreateEmbedFooter::new(random_footer(
                    &data.config.main_server_invite,
                    client_id,
                )))
                .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
                .field("Currently supported voices", &voices, true)
                .field(
                    "Current voice used",
                    user_voice_row.voice.as_deref().unwrap_or("None"),
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
        .parse_user_or_guild(ctx.http(), ctx.author().id, ctx.guild_id())
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
        .parse_user_or_guild(ctx.http(), ctx.author().id, ctx.guild_id())
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
                    name: Cow::Borrowed("channel"),
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
                text_in_voice(),
                skip_emoji(),
                owner::block(),
                owner::bot_ban(),
                owner::gtts_disabled(),
                use_new_formatting(),
            ],
            ..set()
        },
    ]
}
