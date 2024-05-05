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

use std::{borrow::Cow, cmp::Ordering};

use anyhow::Error;
use num_format::{Locale, ToFormattedString};

use poise::{
    serenity_prelude::{
        self as serenity, builder::*, small_fixed_array::FixedString, Mentionable as _,
    },
    CreateReply,
};

use to_arraystring::ToArrayString;
use tts_core::{
    common::{fetch_audio, prepare_url},
    constants::OPTION_SEPERATORS,
    opt_ext::OptionTryUnwrap,
    require_guild,
    structs::{ApplicationContext, Command, CommandResult, Context, IsPremium, TTSMode},
    traits::PoiseContextExt as _,
    translations::GetTextContextExt,
};

#[allow(clippy::trivially_copy_pass_by_ref)] // Required for generic type
fn cmp_float(a: &f64, b: &f64) -> Ordering {
    a.total_cmp(b)
}

/// Shows how long TTS Bot has been online
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn uptime(ctx: Context<'_>) -> CommandResult {
    let timestamp = ctx
        .data()
        .start_time
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let current_user_mention = {
        let current_user = ctx.cache().current_user();
        current_user.mention().to_string()
    };

    ctx.say(
        ctx.gettext("{user_mention} has been up since: <t:{timestamp}:R>")
            .replace("{user_mention}", &current_user_mention)
            .replace("{timestamp}", &timestamp.to_arraystring()),
    )
    .await?;

    Ok(())
}

/// Generates TTS and sends it in the current text channel!
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | ATTACH_FILES"
)]
pub async fn tts(
    ctx: Context<'_>,
    #[description = "The text to TTS"]
    #[rest]
    message: FixedString<u16>,
) -> CommandResult {
    let is_unnecessary_command_invoke = async {
        if !matches!(ctx, poise::Context::Prefix(_)) {
            return Ok(false);
        }

        let (guild_id, author_voice_cid, bot_voice_cid) = {
            if let Some(guild) = ctx.guild() {
                (
                    guild.id,
                    guild
                        .voice_states
                        .get(&ctx.author().id)
                        .and_then(|vc| vc.channel_id),
                    guild
                        .voice_states
                        .get(&ctx.cache().current_user().id)
                        .and_then(|vc| vc.channel_id),
                )
            } else {
                return Ok(false);
            }
        };

        if author_voice_cid.is_some() && author_voice_cid == bot_voice_cid {
            let setup_channel = ctx.data().guilds_db.get(guild_id.into()).await?.channel;
            if setup_channel == Some(ctx.channel_id()) {
                return Ok(true);
            }
        }

        Ok::<_, Error>(false)
    };

    if is_unnecessary_command_invoke.await? {
        ctx.say(ctx.gettext("You don't need to include the `/tts` for messages to be said!"))
            .await?;
        Ok(())
    } else {
        _tts(ctx, ctx.author(), &message).await
    }
}

async fn _tts(ctx: Context<'_>, author: &serenity::User, message: &str) -> CommandResult {
    let attachment = {
        let data = ctx.data();
        let guild_info = if let Some(guild_id) = ctx.guild_id() {
            Some((guild_id, data.is_premium_simple(guild_id).await?))
        } else {
            None
        };

        let (voice, mode) = data
            .parse_user_or_guild_with_premium(author.id, guild_info)
            .await?;

        let guild_row;
        let translation_lang = if let Some((guild_id, is_premium)) = guild_info {
            guild_row = data.guilds_db.get(guild_id.into()).await?;
            guild_row.target_lang(IsPremium::from(is_premium))
        } else {
            None
        };

        let author_name: String = author
            .name
            .chars()
            .filter(|char| char.is_alphanumeric())
            .collect();
        let speaking_rate = data.speaking_rate(author.id, mode).await?;

        let url = prepare_url(
            data.config.tts_service.clone(),
            message,
            &voice,
            mode,
            &speaking_rate,
            &u64::MAX.to_arraystring(),
            translation_lang,
        );

        let auth_key = data.config.tts_service_auth_key.as_deref();
        let audio = fetch_audio(&data.reqwest, url, auth_key)
            .await?
            .try_unwrap()?
            .bytes()
            .await?;

        serenity::CreateAttachment::bytes(
            audio.to_vec(),
            format!(
                "{author_name}-{}.{}",
                ctx.id(),
                match mode {
                    TTSMode::gTTS | TTSMode::gCloud | TTSMode::Polly => "mp3",
                    TTSMode::eSpeak => "wav",
                }
            ),
        )
    };

    ctx.send(
        CreateReply::default()
            .content(ctx.gettext("Generated some TTS!"))
            .attachment(attachment),
    )
    .await?;

    Ok(())
}

#[poise::command(
    category = "Extra Commands",
    hide_in_help,
    context_menu_command = "Speak with their voice!"
)]
pub async fn tts_speak_as(
    ctx: ApplicationContext<'_>,
    message: serenity::Message,
) -> CommandResult {
    _tts(ctx.into(), &message.author, &message.content).await
}

#[poise::command(
    category = "Extra Commands",
    hide_in_help,
    context_menu_command = "Speak with your voice!"
)]
pub async fn tts_speak(ctx: ApplicationContext<'_>, message: serenity::Message) -> CommandResult {
    _tts(ctx.into(), &ctx.interaction.user, &message.content).await
}

/// Shows various different stats
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn botstats(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let cache = ctx.cache();
    let bot_user_id = cache.current_user().id;

    let start_time = std::time::SystemTime::now();
    let [sep1, sep2, sep3, ..] = OPTION_SEPERATORS;

    let guild_ids = cache.guilds();
    let (total_guild_count, total_voice_clients, total_members) = {
        let guilds: Vec<_> = guild_ids.iter().filter_map(|id| cache.guild(*id)).collect();

        (
            guilds.len().to_arraystring(),
            guilds
                .iter()
                .filter(|g| g.voice_states.contains_key(&bot_user_id))
                .count()
                .to_arraystring(),
            guilds
                .into_iter()
                .map(|g| g.member_count)
                .sum::<u64>()
                .to_formatted_string(&Locale::en),
        )
    };

    #[allow(clippy::cast_precision_loss)]
    let scheduler_stats = {
        let scheduler = songbird::driver::get_default_scheduler();
        if let Ok(stats) = scheduler.worker_thread_stats().await
            && !stats.is_empty()
        {
            const NANOS_PER_MILLI: f64 = 1_000_000.0;
            let compute_time_iter = stats
                .iter()
                .map(|s| (s.last_compute_cost_ns() as f64) / NANOS_PER_MILLI);

            // Unwraps are safe due to !stats.is_empty()
            let min = compute_time_iter.clone().min_by(cmp_float).try_unwrap()?;
            let max = compute_time_iter.clone().max_by(cmp_float).try_unwrap()?;
            let avg = compute_time_iter.sum::<f64>() / (stats.len() as f64);

            let mixer_use =
                (scheduler.live_tasks() as f64 / scheduler.total_tasks() as f64) * 100.0;

            Cow::Owned(format!(
                "
With the songbird scheduler stats of
{sep3} Minimum compute cost: {min}
{sep3} Average compute cost: {avg}
{sep3} Maximum compute cost: {max}
{sep3} Mixer usage percentage: {mixer_use:.2}%"
            ))
        } else {
            Cow::Borrowed("")
        }
    };

    let shard_count = cache.shard_count();
    let ram_usage = {
        let mut system_info = data.system_info.lock();
        system_info.refresh_specifics(
            sysinfo::RefreshKind::new()
                .with_processes(sysinfo::ProcessRefreshKind::new().with_memory()),
        );

        let pid = sysinfo::get_current_pid().unwrap();
        system_info.process(pid).unwrap().memory() / 1024 / 1024
    };

    let neutral_colour = ctx.neutral_colour().await;
    let (embed_title, embed_thumbnail) = {
        let current_user = cache.current_user();

        let title = ctx
            .gettext("{bot_name}: Freshly rewritten in Rust!")
            .replace("{bot_name}", &current_user.name);
        let thumbnail = current_user.face();

        (title, thumbnail)
    };

    let time_to_fetch = start_time.elapsed()?.as_secs_f64() * 1000.0;
    let embed = CreateEmbed::default()
        .title(embed_title)
        .thumbnail(embed_thumbnail)
        .url(data.config.main_server_invite.clone())
        .colour(neutral_colour)
        .footer(CreateEmbedFooter::new(
            ctx.gettext(
                "Time to fetch: {time_to_fetch}ms
Support Server: {main_server_invite}
Repository: https://github.com/Discord-TTS/Bot",
            )
            .replace("{time_to_fetch}", &format!("{time_to_fetch:.2}"))
            .replace("{main_server_invite}", &data.config.main_server_invite),
        ))
        .description(
            ctx.gettext(
                "Currently in:
{sep2} {total_voice_clients} voice channels
{sep2} {total_guild_count} servers
Currently using:
{sep1} {shard_count} shards
{sep1} {ram_usage}MB of RAM{scheduler_stats}
and can be used by {total_members} people!",
            )
            .replace("{sep1}", sep1)
            .replace("{sep2}", sep2)
            .replace("{total_guild_count}", &total_guild_count)
            .replace("{total_voice_clients}", &total_voice_clients)
            .replace("{total_members}", &total_members)
            .replace("{shard_count}", &shard_count.get().to_arraystring())
            .replace("{ram_usage}", &format!("{ram_usage:.1}"))
            .replace("{scheduler_stats}", &scheduler_stats),
        );

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Shows the current setup channel!
#[poise::command(
    category = "Extra Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn channel(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let guild_row = ctx.data().guilds_db.get(guild_id.into()).await?;

    let msg = if let Some(channel) = guild_row.channel
        && require_guild!(ctx).channels.contains_key(&channel)
    {
        if channel == ctx.channel_id() {
            Cow::Borrowed(ctx.gettext("You are in the setup channel already!"))
        } else {
            let msg = ctx
                .gettext("The current setup channel is: <#{channel}>")
                .replace("{channel}", &channel.get().to_arraystring());

            Cow::Owned(msg)
        }
    } else {
        Cow::Borrowed(ctx.gettext("The channel hasn't been setup, do `/setup #textchannel`"))
    };

    ctx.say(msg).await?;
    Ok(())
}

/// Shows how you can help support TTS Bot's development and hosting!
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES",
    aliases("purchase", "donate")
)]
pub async fn premium(ctx: Context<'_>) -> CommandResult {
    ctx.say(ctx.gettext("
To support the development and hosting of TTS Bot and get access to TTS Bot Premium, including more modes (`/set mode`), many more voices (`/set voice`), and extra options such as TTS translation, see:
https://www.patreon.com/Gnome_the_Bot_Maker
    ")).await?;

    Ok(())
}

/// Gets current ping to discord!
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES",
    aliases("lag")
)]
pub async fn ping(ctx: Context<'_>) -> CommandResult {
    let ping_before = std::time::SystemTime::now();
    let ping_msg = ctx.say("Loading!").await?;

    let msg = ctx
        .gettext("Current Latency: {}ms")
        .replace("{}", &ping_before.elapsed()?.as_millis().to_arraystring());

    ping_msg
        .edit(ctx, CreateReply::default().content(msg))
        .await?;

    Ok(())
}

/// Sends the instructions to invite TTS Bot and join the support server!
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn invite(ctx: Context<'_>) -> CommandResult {
    let cache = ctx.cache();
    let config = &ctx.data().config;
    let bot_mention = cache.current_user().id.mention().to_string();

    let invite_channel = config.invite_channel;
    ctx.say(if ctx.guild_id() == Some(config.main_server) {
        ctx.gettext("Check out {channel_mention} to invite {bot_mention}!")
            .replace("{channel_mention}", &invite_channel.mention().to_string())
            .replace("{bot_mention}", &bot_mention)
    } else {
        let guild = cache.guild(config.main_server).try_unwrap()?;

        guild
            .channels
            .get(&invite_channel)
            .map(|c| {
                ctx.gettext(
                    "Join {server_invite} and look in #{channel_name} to invite {bot_mention}",
                )
                .replace("{channel_name}", &c.name)
                .replace("{bot_mention}", &bot_mention)
                .replace("{server_invite}", &config.main_server_invite)
            })
            .try_unwrap()?
    })
    .await?;
    Ok(())
}

pub fn commands() -> [Command; 9] {
    [
        tts(),
        uptime(),
        botstats(),
        channel(),
        premium(),
        ping(),
        invite(),
        tts_speak(),
        tts_speak_as(),
    ]
}
