use aformat::{aformat, astr};
use anyhow::Error;
use num_format::{Locale, ToFormattedString};

use poise::{
    serenity_prelude::{
        self as serenity, builder::*, small_fixed_array::FixedString, Mentionable as _,
    },
    CreateReply,
};

use aformat::ToArrayString;
use tts_core::{
    common::{fetch_audio, prepare_url},
    constants::OPTION_SEPERATORS,
    opt_ext::OptionTryUnwrap,
    require_guild,
    structs::{ApplicationContext, Command, CommandResult, Context, IsPremium, TTSMode},
    traits::PoiseContextExt as _,
};

/// Shows how long TTS Bot has been online
#[poise::command(
    category = "Extra Commands",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn uptime(ctx: Context<'_>) -> CommandResult {
    let start_time = ctx.data().start_time;
    let time_since_start = start_time.duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let msg = {
        let current_user = ctx.cache().current_user().mention();
        aformat!("{current_user} has been up since: <t:{time_since_start}:R>")
    };

    ctx.say(&*msg).await?;
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
        ctx.say("You don't need to include the `/tts` for messages to be said!")
            .await?;
        Ok(())
    } else {
        _tts(ctx, ctx.author(), &message).await
    }
}

async fn _tts(ctx: Context<'_>, author: &serenity::User, message: &str) -> CommandResult {
    let attachment = {
        let data = ctx.data();
        let http = ctx.http();
        let guild_info = if let Some(guild_id) = ctx.guild_id() {
            Some((guild_id, data.is_premium_simple(http, guild_id).await?))
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

        let mut file_name = author_name;
        file_name.push_str(&aformat!(
            "-{}.{}",
            ctx.id(),
            match mode {
                TTSMode::gTTS | TTSMode::gCloud | TTSMode::Polly => astr!("mp3"),
                TTSMode::eSpeak => astr!("wav"),
            }
        ));

        serenity::CreateAttachment::bytes(audio.to_vec(), file_name)
    };

    ctx.send(
        CreateReply::default()
            .content("Generated some TTS!")
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
    let [sep1, sep2, ..] = OPTION_SEPERATORS;

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

        let title = format!("{}: Freshly rewritten in Rust!", current_user.name);
        let thumbnail = current_user.face();

        (title, thumbnail)
    };

    let time_to_fetch = start_time.elapsed()?.as_secs_f64() * 1000.0;
    let embed = CreateEmbed::default()
        .title(embed_title)
        .thumbnail(embed_thumbnail)
        .url(data.config.main_server_invite.clone())
        .colour(neutral_colour)
        .footer(CreateEmbedFooter::new(format!(
            "Time to fetch: {time_to_fetch:.2}ms
Support Server: {}
Repository: https://github.com/Discord-TTS/Bot",
            data.config.main_server_invite
        )))
        .description(format!(
            "Currently in:
{sep2} {total_voice_clients} voice channels
{sep2} {total_guild_count} servers
Currently using:
{sep1} {shard_count} shards
{sep1} {ram_usage:.1}MB of RAM
and can be used by {total_members} people!",
        ));

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
            "You are in the setup channel already!"
        } else {
            &aformat!("The current setup channel is: <#{channel}>")
        }
    } else {
        "The channel hasn't been setup, do `/setup #textchannel`"
    };

    ctx.say(msg).await?;
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

    let msg = aformat!("Current Latency: {}ms", ping_before.elapsed()?.as_millis());

    ping_msg
        .edit(ctx, CreateReply::default().content(msg.as_str()))
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

    let bot_mention = cache.current_user().id.mention();
    let msg = if ctx.guild_id() == Some(config.main_server) {
        &*aformat!(
            "Check out {} to invite {bot_mention}!",
            config.invite_channel.mention(),
        )
    } else {
        let guild = cache.guild(config.main_server).try_unwrap()?;
        let channel = guild.channels.get(&config.invite_channel).try_unwrap()?;

        &format!(
            "Join {} and look in #{} to invite {bot_mention}",
            config.main_server_invite, channel.name,
        )
    };

    ctx.say(msg).await?;
    Ok(())
}

pub fn commands() -> [Command; 8] {
    [
        tts(),
        uptime(),
        botstats(),
        channel(),
        ping(),
        invite(),
        tts_speak(),
        tts_speak_as(),
    ]
}
