use std::{borrow::Cow, fmt::Write as _};

use poise::{
    futures_util::{stream::BoxStream, StreamExt as _},
    serenity_prelude::{self as serenity, builder::*},
    CreateReply,
};

use to_arraystring::ToArrayString;
use tts_core::{
    common::remove_premium,
    constants::PREMIUM_NEUTRAL_COLOUR,
    structs::{Command, CommandResult, Context, Result, TTSMode},
    traits::PoiseContextExt,
    translations::GetTextContextExt,
};

#[derive(sqlx::FromRow)]
struct GuildIdRow {
    guild_id: i64,
}

fn get_premium_guilds<'a>(
    conn: impl sqlx::PgExecutor<'a> + 'a,
    premium_user: serenity::UserId,
) -> BoxStream<'a, Result<GuildIdRow, sqlx::Error>> {
    sqlx::query_as("SELECT guild_id FROM guilds WHERE premium_user = $1")
        .bind(premium_user.get() as i64)
        .fetch(conn)
}

async fn get_premium_guild_count<'a>(
    conn: impl sqlx::PgExecutor<'a> + 'a,
    premium_user: serenity::UserId,
) -> Result<i64> {
    let guilds = get_premium_guilds(conn, premium_user);
    Ok(guilds.count().await as i64)
}

/// Activates a server for TTS Bot Premium!
#[poise::command(
    category = "Premium Management",
    guild_only,
    prefix_command,
    slash_command,
    aliases("activate"),
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn premium_activate(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let data = ctx.data();

    if data.is_premium_simple(guild_id).await? {
        ctx.say(ctx.gettext("Hey, this server is already premium!"))
            .await?;
        return Ok(());
    }

    let author = ctx.author();
    let linked_guilds = get_premium_guild_count(&data.pool, author.id).await?;
    let error_msg = match data.fetch_patreon_info(author.id).await? {
        Some(tier) => {
            if linked_guilds as u8 >= tier.entitled_servers {
                Some(Cow::Owned(ctx
                    .gettext("Hey, you already have {server_count} servers linked, you are only subscribed to the {entitled_servers} tier!")
                    .replace("{entitled_servers}", &tier.entitled_servers.to_arraystring())
                    .replace("{server_count}", &linked_guilds.to_arraystring())
                ))
            } else {
                None
            }
        }
        None => Some(Cow::Borrowed(
            ctx.gettext("Hey, I don't think you are subscribed on Patreon!"),
        )),
    };

    if let Some(error_msg) = error_msg {
        ctx.send(CreateReply::default().embed(CreateEmbed::default()
            .title("TTS Bot Premium")
            .description(error_msg)
            .thumbnail(data.premium_avatar_url.as_str())
            .colour(PREMIUM_NEUTRAL_COLOUR)
            .footer(CreateEmbedFooter::new({
                let line1 = ctx.gettext("If you have just subscribed, please wait for up to an hour for the member list to update!\n");
                let line2 = ctx.gettext("If this is incorrect, and you have waited an hour, please contact GnomedDev.");

                let mut concat = String::with_capacity(line1.len() + line2.len());
                concat.push_str(line1);
                concat.push_str(line2);
                concat
            }))
        )).await?;

        return Ok(());
    }

    let author_id = author.id.get() as i64;
    data.userinfo_db.create_row(author_id).await?;
    data.guilds_db
        .set_one(guild_id.into(), "premium_user", &author_id)
        .await?;
    data.guilds_db
        .set_one(guild_id.into(), "voice_mode", &TTSMode::gCloud)
        .await?;

    ctx.say(ctx.gettext("Done! This server is now premium!"))
        .await?;

    let guild = ctx.cache().guild(guild_id);
    let guild_name = guild.as_ref().map_or("<Unknown>", |g| g.name.as_str());

    tracing::info!(
        "{}#{} | {} linked premium to {} | {}, they had {} linked servers",
        author.name,
        author.discriminator.map_or(0, std::num::NonZeroU16::get),
        author.id,
        guild_name,
        guild_id,
        linked_guilds
    );
    Ok(())
}

/// Lists all servers you activated for TTS Bot Premium
#[poise::command(
    category = "Premium Management",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn list_premium(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let Some(premium_info) = ctx.data().fetch_patreon_info(ctx.author().id).await? else {
        ctx.say(ctx.gettext("I cannot confirm you are subscribed on patreon, so you don't have any premium servers!")).await?;
        return Ok(());
    };

    let mut premium_guilds = 0;
    let mut embed_desc = Cow::Borrowed("");
    let mut guilds = get_premium_guilds(&data.pool, ctx.author().id);
    while let Some(guild_row) = guilds.next().await {
        premium_guilds += 1;
        let guild_id = serenity::GuildId::new(guild_row?.guild_id as u64);
        if let Some(guild_ref) = ctx.cache().guild(guild_id) {
            writeln!(embed_desc.to_mut(), "- (`{guild_id}`) {}", guild_ref.name)?;
        } else {
            writeln!(embed_desc.to_mut(), "- (`{guild_id}`) **<Unknown>**")?;
        }
    }

    let author = ctx.author();
    let remaining_guilds = premium_info.entitled_servers - premium_guilds;
    if embed_desc.is_empty() {
        embed_desc = Cow::Borrowed("None... set some servers with `/premium_activate`!");
    }

    let embed = CreateEmbed::new()
        .title("The premium servers you have activated:")
        .description(embed_desc)
        .colour(PREMIUM_NEUTRAL_COLOUR)
        .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
        .footer(CreateEmbedFooter::new(format!(
            "You have {remaining_guilds} server(s) remaining for premium activation"
        )));

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Deactivates a server from TTS Bot Premium.
#[poise::command(
    category = "Premium Management",
    prefix_command,
    slash_command,
    guild_only,
    aliases("premium_remove", "premium_delete"),
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn premium_deactivate(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();
    let guild_id = ctx.guild_id().unwrap();
    let guild_row = data.guilds_db.get(guild_id.get() as i64).await?;

    let Some(premium_user) = guild_row.premium_user else {
        let msg = ctx.gettext("This server isn't activated for premium, so I can't deactivate it!");
        ctx.send_ephemeral(msg).await?;
        return Ok(());
    };

    if premium_user != author.id {
        let msg = ctx.gettext(
            "You are not setup as the premium user for this server, so cannot deactivate it!",
        );
        ctx.send_ephemeral(msg).await?;
        return Ok(());
    }

    remove_premium(&data, guild_id).await?;

    let msg = ctx.gettext("Deactivated premium from this server.");
    ctx.say(msg).await?;
    Ok(())
}

pub fn commands() -> [Command; 3] {
    [premium_activate(), list_premium(), premium_deactivate()]
}
