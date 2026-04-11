use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use aformat::{ArrayString, aformat};

use poise::serenity_prelude::{
    self as serenity, builder::*, colours::branding::YELLOW, futures::channel::mpsc,
};

use tts_core::{
    common::{push_permission_names, random_footer},
    constants::RED,
    database_models::GuildRow,
    opt_ext::OptionTryUnwrap as _,
    require_guild,
    structs::{Command, CommandResult, Context, Result},
    traits::PoiseContextExt,
    voice,
};

use crate::REQUIRED_VC_PERMISSIONS;

/// Returns Some(GuildRow) on correct channel, otherwise None.
async fn channel_check(
    ctx: &Context<'_>,
    author_vc: Option<serenity::ChannelId>,
) -> Result<Option<Arc<GuildRow>>> {
    let guild_id = ctx.guild_id().unwrap();
    let guild_row = ctx.data().guilds_db.get(guild_id.into()).await?;

    let channel_id = Some(ctx.channel_id().expect_channel());
    if guild_row.channel == channel_id || author_vc == channel_id {
        return Ok(Some(guild_row));
    }

    let msg = if let Some(setup_id) = guild_row.channel {
        let guild = require_guild!(ctx, Ok(None));
        if guild.channels.contains_key(&setup_id) {
            &aformat!("You ran this command in the wrong channel, please move to <#{setup_id}>.")
        } else {
            "Your setup channel has been deleted, please run /setup!"
        }
    } else {
        "You haven't setup the bot, please run /setup!"
    };

    ctx.send_error(msg).await?;
    Ok(None)
}

async fn handle_vc_mismatch(
    ctx: Context<'_>,
    author_vc: serenity::ChannelId,
    interconnect: mpsc::UnboundedSender<voice::InterconnectMessage>,
    bot_id: serenity::UserId,
    bot_channel_id: serenity::ChannelId,
) -> Result<()> {
    let (guild_id, channel_exists, voice_state_matches) = {
        let Some(guild) = ctx.guild() else {
            return Ok(());
        };

        let voice_state = guild.voice_states.get(&bot_id);

        (
            guild.id,
            guild.channels.contains_key(&bot_channel_id),
            voice_state.is_some_and(|vs| vs.channel_id == Some(bot_channel_id)),
        )
    };

    let msg = match (channel_exists, voice_state_matches) {
        (true, true) => {
            if author_vc == bot_channel_id {
                "I am already in your voice channel!"
            } else {
                &*aformat!("I am already in <#{bot_channel_id}>!")
            }
        }
        (false, _) => {
            tracing::warn!("Channel {bot_channel_id} didn't exist in {guild_id} in `/join`");
            voice::end_connection(interconnect).await;
            "I was in a deleted voice channel, but I have left now. Please try again."
        }
        (_, false) => {
            tracing::warn!("Voice task was in the wrong channel in {guild_id} in `/join`");
            voice::end_connection(interconnect).await;
            "I was confused about what voice channel I was in, but I have left now. Please try again."
        }
    };

    ctx.send_error(msg).await?;
    Ok(())
}

fn create_warning_embed<'a>(title: &'a str, footer: &'a str) -> serenity::CreateEmbed<'a> {
    serenity::CreateEmbed::default()
        .title(title)
        .colour(YELLOW)
        .footer(serenity::CreateEmbedFooter::new(footer))
}

#[cold]
fn gtts_disabled_embed<'a>(
    msg: poise::CreateReply<'a>,
    support_server: &'a str,
) -> poise::CreateReply<'a> {
    msg.embed(
        serenity::CreateEmbed::default()
            .title("The `gTTS` voice mode is globally disabled due to maintainance")
            .description("Any usage of this mode will instead use the lower quality `eSpeak` mode, while premium modes are unaffected.")
            .footer(serenity::CreateEmbedFooter::new(format!("Support server: {support_server}")))
            .colour(RED)
    )
}

#[cold]
fn required_prefix_embed<'a>(
    title_place: &'a mut ArrayString<46>,
    msg: poise::CreateReply<'a>,
    required_prefix: ArrayString<8>,
) -> poise::CreateReply<'a> {
    *title_place = aformat!("Your TTS required prefix is set to: `{required_prefix}`");
    let footer = "To disable the required prefix, use /set required_prefix with no options.";

    msg.embed(create_warning_embed(title_place.as_str(), footer))
}

#[cold]
fn required_role_embed<'a>(
    title_place: &'a mut ArrayString<133>,

    ctx: Context<'a>,
    msg: poise::CreateReply<'a>,
    required_role: serenity::RoleId,
) -> poise::CreateReply<'a> {
    let guild = ctx.guild();
    let role_name = guild
        .as_deref()
        .and_then(|g| g.roles.get(&required_role).map(|r| r.name.as_str()))
        .unwrap_or("Unknown");

    let role_name = aformat::CapStr::<100>(role_name);
    *title_place = aformat!("The required role for TTS is: `@{role_name}`");
    let footer = "To disable the required role, use /set required_role with no options.";

    msg.embed(create_warning_embed(title_place.as_str(), footer))
}

/// Joins the voice channel you're in!
#[poise::command(
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn join(ctx: Context<'_>) -> CommandResult {
    let Some(author_vc) = ctx.author_vc() else {
        let err = "I cannot join your voice channel unless you are in one!";
        ctx.send_error(err).await?;
        return Ok(());
    };

    let Some(guild_row) = channel_check(&ctx, Some(author_vc)).await? else {
        return Ok(());
    };

    let guild_id = ctx.guild_id().unwrap();
    let (bot_id, bot_face) = {
        let current_user = ctx.cache().current_user();
        (current_user.id, current_user.face())
    };

    let (author_vc_bot_perms, communication_disabled_until) = {
        let guild = require_guild!(ctx);
        let bot_member = guild.members.get(&bot_id).try_unwrap()?;
        let author_vc = guild.channels.get(&author_vc).try_unwrap()?;

        let bot_vc_perms = guild.user_permissions_in(author_vc, bot_member);
        (bot_vc_perms, bot_member.communication_disabled_until)
    };

    if let Some(communication_disabled_until) = communication_disabled_until
        && communication_disabled_until > serenity::Timestamp::now()
    {
        let msg = "I am timed out, please ask a moderator to remove the timeout.";
        ctx.send_error(msg).await?;
        return Ok(());
    }

    let missing_permissions = REQUIRED_VC_PERMISSIONS - author_vc_bot_perms;
    if !missing_permissions.is_empty() {
        let mut msg = String::from(
            "I do not have permission to TTS in your voice channel, please ask a server administrator to give me: ",
        );
        push_permission_names(&mut msg, missing_permissions);

        ctx.send_error(msg).await?;
        return Ok(());
    }

    let data = ctx.data();
    let display_name = {
        let voice_context = voice::VCContext {
            serenity: ctx.serenity_context().clone(),
            channel_id: Arc::new(AtomicU64::new(author_vc.get())),
            guild_id,
            bot_id,
        };

        match voice::start_connection(&data, voice_context).await {
            voice::StartConnectionResult::Started(_) => {}
            voice::StartConnectionResult::AlreadyIn((interconnect, bot_channel_id, _)) => {
                let bot_channel_id =
                    serenity::ChannelId::new(bot_channel_id.load(Ordering::SeqCst));
                handle_vc_mismatch(ctx, author_vc, interconnect, bot_id, bot_channel_id).await?;
                return Ok(());
            }
            voice::StartConnectionResult::TimedOut => {
                let msg = "I failed to join your voice channel, please check I have the right permissions and try again!";
                ctx.send_error(msg).await?;
                return Ok(());
            }
        }

        match ctx {
            Context::Application(poise::ApplicationContext { interaction, .. }) => {
                interaction.member.as_deref().try_unwrap()?.display_name()
            }
            Context::Prefix(poise::PrefixContext { msg, .. }) => {
                let member = msg.member.as_deref().try_unwrap()?;
                member.nick.as_deref().unwrap_or(msg.author.display_name())
            }
        }
    };

    let embed = serenity::CreateEmbed::default()
        .title("Joined your voice channel!")
        .description("Just type normally and TTS Bot will say your messages!")
        .thumbnail(bot_face)
        .author(CreateEmbedAuthor::new(display_name).icon_url(ctx.author().face()))
        .footer(CreateEmbedFooter::new(random_footer(
            &data.config.main_server_invite,
            bot_id,
        )));

    let mut msg = poise::CreateReply::default().embed(embed);

    // In-perfect premium check, but we don't need to be perfect
    if data.config.gtts_disabled.load(Ordering::Relaxed) && guild_row.premium_user.is_none() {
        msg = gtts_disabled_embed(msg, &data.config.main_server_invite);
    }

    let mut title_place = ArrayString::new();
    if let Some(required_prefix) = guild_row.required_prefix {
        msg = required_prefix_embed(&mut title_place, msg, required_prefix);
    }

    let mut title_place = ArrayString::new();
    if let Some(required_role) = guild_row.required_role {
        msg = required_role_embed(&mut title_place, ctx, msg, required_role);
    }

    ctx.send(msg).await?;
    Ok(())
}

/// Leaves voice channel TTS Bot is in!
#[poise::command(
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn leave(ctx: Context<'_>) -> CommandResult {
    let (guild_id, author_vc) = {
        let guild = require_guild!(ctx);
        let channel_id = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|vs| vs.channel_id);

        (guild.id, channel_id)
    };

    if channel_check(&ctx, author_vc).await?.is_none() {
        return Ok(());
    }

    let result = if let Some(author_vc) = author_vc {
        voice::leave_vc(&ctx.data(), guild_id, Some(author_vc)).await
    } else {
        voice::LeaveVCResult::Mismatch
    };

    match result {
        voice::LeaveVCResult::Left => {
            ctx.say("Left voice channel!").await?;
        }
        voice::LeaveVCResult::Mismatch => {
            ctx.send_error("You need to be in the same voice channel as me to make me leave!")
                .await?;
        }
        voice::LeaveVCResult::Missing => {
            ctx.send_error("I'm not in a voice channel, so can't leave one!")
                .await?;
        }
    }

    Ok(())
}

/// Clears the message queue!
#[poise::command(
    aliases("skip"),
    category = "Main Commands",
    guild_only,
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | ADD_REACTIONS"
)]
pub async fn clear(ctx: Context<'_>) -> CommandResult {
    if channel_check(&ctx, ctx.author_vc()).await?.is_none() {
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();
    if voice::clear_queue(&ctx.data(), guild_id).is_ok() {
        match ctx {
            poise::Context::Prefix(ctx) => {
                // Prefixed command, just add a thumbsup reaction
                ctx.msg.react(ctx.http(), '👍').await?;
            }
            poise::Context::Application(_) => {
                // Slash command, no message to react to, just say thumbsup
                ctx.say("👍").await?;
            }
        }
    } else {
        ctx.say("**Error**: I am not in a voice channel!").await?;
    }

    Ok(())
}

pub fn commands() -> [Command; 3] {
    [join(), leave(), clear()]
}
