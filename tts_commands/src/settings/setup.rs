use anyhow::bail;
use to_arraystring::ToArrayString;

use poise::serenity_prelude as serenity;
use serenity::{builder::*, small_fixed_array::FixedString, ComponentInteractionDataKind};

use tts_core::{
    common::{confirm_dialog, random_footer},
    require, require_guild,
    structs::{CommandResult, Context, Result},
    translations::GetTextContextExt as _,
};

fn can_send(
    guild: &serenity::Guild,
    channel: &serenity::GuildChannel,
    member: &serenity::Member,
) -> bool {
    const REQUIRED_PERMISSIONS: serenity::Permissions = serenity::Permissions::from_bits_truncate(
        serenity::Permissions::SEND_MESSAGES.bits() | serenity::Permissions::VIEW_CHANNEL.bits(),
    );

    (REQUIRED_PERMISSIONS - guild.user_permissions_in(channel, member)).is_empty()
}

type U64ArrayString = <u64 as ToArrayString>::ArrayString;
type EligibleSetupChannel = (
    serenity::ChannelId,
    U64ArrayString,
    FixedString<u16>,
    u16,
    bool,
);

async fn get_eligible_channels(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    bot_member: serenity::Member,
) -> Result<Option<Vec<EligibleSetupChannel>>> {
    let author = ctx.author();
    let author_member = guild_id.member(ctx, author.id).await?;

    let guild = require_guild!(ctx, Ok(None));
    let channels = guild
        .channels
        .values()
        .filter(|c| {
            c.kind == serenity::ChannelType::Text
                && can_send(&guild, c, &author_member)
                && can_send(&guild, c, &bot_member)
        })
        .map(|c| {
            let has_webhook_perms = guild.user_permissions_in(c, &bot_member).manage_webhooks();
            let id_str = c.id.get().to_arraystring();
            (c.id, id_str, c.name.clone(), c.position, has_webhook_perms)
        })
        .collect();

    Ok(Some(channels))
}

async fn show_channel_select_menu(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    bot_member: serenity::Member,
) -> Result<Option<(serenity::ChannelId, bool)>> {
    let mut text_channels = require!(
        get_eligible_channels(ctx, guild_id, bot_member).await?,
        Ok(None)
    );

    if text_channels.is_empty() {
        ctx.say(ctx.gettext("**Error**: This server doesn't have any text channels that we both have Read/Send Messages in!")).await?;
        return Ok(None);
    } else if text_channels.len() >= (25 * 5) {
        ctx.say(ctx.gettext("**Error**: This server has too many text channels to show in a menu! Please run `/setup #channel`")).await?;
        return Ok(None);
    };

    text_channels.sort_by(|(_, _, _, f, _), (_, _, _, s, _)| Ord::cmp(&f, &s));

    let builder = poise::CreateReply::default()
        .content(ctx.gettext("Select a channel!"))
        .components(generate_channel_select(&text_channels));

    let reply = ctx.send(builder).await?;
    let interaction = reply
        .message()
        .await?
        .await_component_interaction(ctx.serenity_context().shard.clone())
        .timeout(std::time::Duration::from_secs(60 * 5))
        .author_id(ctx.author().id)
        .await;

    let Some(interaction) = interaction else {
        // The timeout was hit
        return Ok(None);
    };

    interaction.defer(ctx.http()).await?;

    let ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind else {
        bail!("Expected a string value")
    };

    let selected_id: serenity::ChannelId = values[0].parse()?;
    let (_, _, _, _, has_webhook_perms) = text_channels
        .into_iter()
        .find(|(c_id, _, _, _, _)| *c_id == selected_id)
        .unwrap();

    Ok(Some((selected_id, has_webhook_perms)))
}

fn generate_channel_select(text_channels: &[EligibleSetupChannel]) -> Vec<CreateActionRow<'_>> {
    text_channels
        .chunks(25)
        .enumerate()
        .map(|(i, chunked_channels)| {
            CreateActionRow::SelectMenu(CreateSelectMenu::new(
                format!("select::channels::{i}"),
                CreateSelectMenuKind::String {
                    options: chunked_channels
                        .iter()
                        .map(|(_, id_str, name, _, _)| {
                            CreateSelectMenuOption::new(&**name, &**id_str)
                        })
                        .collect(),
                },
            ))
        })
        .collect::<Vec<_>>()
}

/// Setup the bot to read messages from the given channel
#[poise::command(
    guild_only,
    category = "Settings",
    prefix_command,
    slash_command,
    required_permissions = "ADMINISTRATOR",
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]

pub async fn setup(
    ctx: Context<'_>,
    #[description = "The channel for the bot to read messages from"]
    #[channel_types("Text")]
    channel: Option<serenity::GuildChannel>,
) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();
    let guild_id = ctx.guild_id().unwrap();

    let (bot_user_id, bot_user_name, bot_user_face) = {
        let current_user = ctx.cache().current_user();
        (
            current_user.id,
            current_user.name.clone(),
            current_user.face(),
        )
    };

    let (channel_id, has_webhook_perms) = {
        let bot_member = guild_id.member(ctx, bot_user_id).await?;
        let (channel, has_webhook_perms) = if let Some(channel) = channel {
            let chan_perms = require_guild!(ctx).user_permissions_in(&channel, &bot_member);
            (channel.id, chan_perms.manage_webhooks())
        } else {
            require!(
                show_channel_select_menu(ctx, guild_id, bot_member).await?,
                Ok(())
            )
        };

        (channel, has_webhook_perms)
    };

    data.guilds_db
        .set_one(guild_id.into(), "channel", &(channel_id.get() as i64))
        .await?;
    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title(
                    ctx.gettext("{bot_name} has been setup!")
                        .replace("{bot_name}", &bot_user_name),
                )
                .thumbnail(&bot_user_face)
                .description(
                    ctx.gettext(
                        "
TTS Bot will now accept commands and read from <#{channel}>.
Just do `/join` and start talking!",
                    )
                    .replace("{channel}", &channel_id.get().to_arraystring()),
                )
                .footer(serenity::CreateEmbedFooter::new(random_footer(
                    &data.config.main_server_invite,
                    bot_user_id,
                    ctx.current_catalog(),
                )))
                .author(serenity::CreateEmbedAuthor::new(&*author.name).icon_url(author.face())),
        ),
    )
    .await?;

    let poise::Context::Application(_) = ctx else {
        return Ok(());
    };

    if !has_webhook_perms {
        return Ok(());
    }

    let Some(confirmed) = confirm_dialog(
        ctx,
        ctx.gettext("Would you like to set up TTS Bot update announcements for the setup channel?"),
        ctx.gettext("Yes"),
        ctx.gettext("No"),
    )
    .await?
    else {
        return Ok(());
    };

    let reply = if confirmed {
        let announcements = data.config.announcements_channel;
        announcements.follow(ctx.http(), channel_id).await?;

        ctx.gettext("Set up update announcements in this channel!")
    } else {
        ctx.gettext("Okay, didn't set up update announcements.")
    };

    ctx.send(poise::CreateReply::default().content(reply).ephemeral(true))
        .await?;

    Ok(())
}
