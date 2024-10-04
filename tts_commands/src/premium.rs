use std::{borrow::Cow, fmt::Write as _};

use aformat::aformat;

use poise::{
    futures_util::{stream::BoxStream, StreamExt as _},
    serenity_prelude::{self as serenity, builder::*},
    CreateReply,
};

use tts_core::{
    common::remove_premium,
    constants::PREMIUM_NEUTRAL_COLOUR,
    opt_ext::OptionTryUnwrap as _,
    structs::{Command, CommandResult, Context, Result, TTSMode},
    traits::PoiseContextExt,
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

/// Shows how you can help support TTS Bot's development and hosting!
#[poise::command(
    category = "Premium",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES",
    aliases("purchase", "donate")
)]
pub async fn premium(ctx: Context<'_>) -> CommandResult {
    let msg = if let Some(premium_config) = &ctx.data().premium_config {
        let patreon_url = premium_config.patreon_page_url;
        let application_id = ctx.http().application_id().try_unwrap()?;
        &aformat!(concat!(
            "To support the development and hosting of TTS Bot and get access to TTS Bot Premium, ",
            "including more modes (`/set mode`), many more voices (`/set voice`), ",
            "and extra options such as TTS translation, follow one of these links:\n",
            "Patreon: <{patreon_url}>\nDiscord: https://discord.com/application-directory/{application_id}/store"
        ))
    } else {
        "This version of TTS Bot does not have premium features enabled."
    };

    ctx.say(msg).await?;
    Ok(())
}

/// Activates a server for TTS Bot Premium!
#[poise::command(
    category = "Premium",
    guild_only,
    prefix_command,
    slash_command,
    aliases("activate"),
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn premium_activate(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let data = ctx.data();

    if data.is_premium_simple(ctx.http(), guild_id).await? {
        ctx.say("Hey, this server is already premium!").await?;
        return Ok(());
    }

    let author = ctx.author();
    let linked_guilds = get_premium_guild_count(&data.pool, author.id).await?;
    let error_msg = match data.fetch_premium_info(ctx.http(), author.id).await? {
        Some(tier) => {
            if linked_guilds >= tier.entitled_servers.get().into() {
                Some(Cow::Owned(format!("Hey, you already have {linked_guilds} servers linked, you are only subscribed to the {} tier!", tier.entitled_servers)))
            } else {
                None
            }
        }
        None => Some(Cow::Borrowed(
            "Hey, I don't think you are subscribed to TTS Bot Premium!",
        )),
    };

    if let Some(error_msg) = error_msg {
        let embed = CreateEmbed::default()
            .title("TTS Bot Premium")
            .description(error_msg)
            .thumbnail(data.premium_avatar_url.as_str())
            .colour(PREMIUM_NEUTRAL_COLOUR)
            .footer(CreateEmbedFooter::new(concat!(
                "If you have just subscribed to TTS Bot Premium, please wait up to an hour and try again!\n",
                "For support, please join the support server via `/invite`."
            )));

        ctx.send(CreateReply::default().embed(embed)).await?;
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

    ctx.say("Done! This server is now premium!").await?;

    let guild = ctx.cache().guild(guild_id);
    let guild_name = match guild.as_ref() {
        Some(g) => g.name.as_str(),
        None => "<Unknown>",
    };

    tracing::info!(
        "{} | {} linked premium to {} | {}, they had {} linked servers",
        author.tag(),
        author.id,
        guild_name,
        guild_id,
        linked_guilds
    );
    Ok(())
}

/// Lists all servers you activated for TTS Bot Premium
#[poise::command(
    category = "Premium",
    prefix_command,
    slash_command,
    required_bot_permissions = "SEND_MESSAGES | EMBED_LINKS"
)]
pub async fn list_premium(ctx: Context<'_>) -> CommandResult {
    let data = ctx.data();
    let Some(premium_info) = data.fetch_premium_info(ctx.http(), ctx.author().id).await? else {
        ctx.say("I cannot confirm you are subscribed to premium, so you don't have any premium servers!").await?;
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
    let remaining_guilds = premium_info.entitled_servers.get() - premium_guilds;
    if embed_desc.is_empty() {
        embed_desc = Cow::Borrowed("None... set some servers with `/premium_activate`!");
    }

    let footer = aformat!("You have {remaining_guilds} server(s) remaining for premium activation");
    let embed = CreateEmbed::new()
        .title("The premium servers you have activated:")
        .description(embed_desc)
        .colour(PREMIUM_NEUTRAL_COLOUR)
        .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
        .footer(CreateEmbedFooter::new(footer.as_str()));

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Deactivates a server from TTS Bot Premium.
#[poise::command(
    category = "Premium",
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
        let msg = "This server isn't activated for premium, so I can't deactivate it!";
        ctx.send_ephemeral(msg).await?;
        return Ok(());
    };

    if premium_user != author.id {
        let msg = "You are not setup as the premium user for this server, so cannot deactivate it!";
        ctx.send_ephemeral(msg).await?;
        return Ok(());
    }

    remove_premium(&data, guild_id).await?;

    let msg = "Deactivated premium from this server.";
    ctx.say(msg).await?;
    Ok(())
}

pub fn commands() -> [Command; 4] {
    [
        premium(),
        premium_activate(),
        list_premium(),
        premium_deactivate(),
    ]
}
