#![feature(let_chains)]

use std::borrow::Cow;

use aformat::aformat;

use serenity::all::{self as serenity, Mentionable as _};

use tts_core::{
    constants::PREMIUM_NEUTRAL_COLOUR,
    opt_ext::OptionTryUnwrap as _,
    structs::{Command, Context, FailurePoint, PartialContext, Result},
    traits::PoiseContextExt,
};

mod help;
mod main_;
mod other;
mod owner;
mod premium;
mod settings;

pub fn commands() -> Vec<Command> {
    main_::commands()
        .into_iter()
        .chain(other::commands())
        .chain(settings::commands())
        .chain(premium::commands())
        .chain(owner::commands())
        .chain(help::commands())
        .collect()
}

pub async fn premium_command_check(ctx: Context<'_>) -> Result<bool> {
    if let Context::Application(ctx) = ctx {
        if ctx.interaction_type == poise::CommandInteractionType::Autocomplete {
            // Ignore the premium check during autocomplete.
            return Ok(true);
        }
    }

    let data = ctx.data();
    let guild_id = ctx.guild_id();
    let serenity_ctx = ctx.serenity_context();

    let mut main_msg = match data.premium_check(ctx.http(), guild_id).await? {
        None => return Ok(true),
        Some(FailurePoint::Guild) => {
            Cow::Borrowed("Hey, this is a premium command so it must be run in a server!")
        }
        Some(FailurePoint::PremiumUser) => Cow::Borrowed(
            "Hey, this server isn't premium, please purchase TTS Bot Premium! (`/premium`)",
        ),
        Some(FailurePoint::NotSubscribed(premium_user_id)) => {
            let premium_user = premium_user_id.to_user(serenity_ctx).await?;
            Cow::Owned(format!(concat!(
                "Hey, this server has a premium user setup, however they no longer have a subscription! ",
                "Please ask {} to renew their membership."
            ), premium_user.tag()))
        }
    };

    let author = ctx.author();
    let guild_info = match guild_id.and_then(|g_id| serenity_ctx.cache.guild(g_id)) {
        Some(g) => &format!("{} | {}", g.name, g.id),
        None => "DMs",
    };

    tracing::warn!(
        "{} | {} failed the premium check in {}",
        author.tag(),
        author.id,
        guild_info
    );

    let permissions = ctx.author_permissions()?;
    if permissions.send_messages() {
        let builder = poise::CreateReply::default();
        ctx.send({
            const FOOTER_MSG: &str = "If this is an error, please contact GnomedDev.";
            if permissions.embed_links() {
                let embed = serenity::CreateEmbed::default()
                    .title("TTS Bot Premium - Premium Only Command!")
                    .description(main_msg)
                    .colour(PREMIUM_NEUTRAL_COLOUR)
                    .thumbnail(data.premium_avatar_url.as_str())
                    .footer(serenity::CreateEmbedFooter::new(FOOTER_MSG));

                builder.embed(embed)
            } else {
                let main_msg = main_msg.to_mut();
                main_msg.push('\n');
                main_msg.push_str(FOOTER_MSG);
                builder.content(main_msg.as_str())
            }
        })
        .await?;
    }

    Ok(false)
}

pub async fn get_prefix(ctx: PartialContext<'_>) -> Result<Option<Cow<'static, str>>> {
    let Some(guild_id) = ctx.guild_id else {
        return Ok(Some(Cow::Borrowed("-")));
    };

    let data = ctx.framework.user_data();
    let row = data.guilds_db.get(guild_id.into()).await?;

    let prefix = row.prefix.as_str();
    let prefix = if prefix == "-" {
        Cow::Borrowed("-")
    } else {
        Cow::Owned(String::from(prefix))
    };

    Ok(Some(prefix))
}

#[cold]
async fn notify_banned(ctx: Context<'_>) -> Result<()> {
    const BAN_MESSAGE: &str = "
You have been banned from the bot. This is not reversable and is only given out in exceptional circumstances.
You may have:
- Committed a hate crime against the developers of the bot.
- Exploited an issue in the bot to bring it down or receive premium without paying.
- Broken the TTS Bot terms of service.";

    let author = ctx.author();
    let bot_face = ctx.cache().current_user().face();

    let embed = serenity::CreateEmbed::new()
        .author(serenity::CreateEmbedAuthor::new(author.name.as_str()).icon_url(author.face()))
        .thumbnail(bot_face)
        .colour(tts_core::constants::RED)
        .description(BAN_MESSAGE)
        .footer(serenity::CreateEmbedFooter::new(
            "Do not join the support server to appeal this. You are not wanted.",
        ));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

pub async fn command_check(ctx: Context<'_>) -> Result<bool> {
    if ctx.author().bot() {
        return Ok(false);
    };

    let data = ctx.data();
    let user_row = data.userinfo_db.get(ctx.author().id.into()).await?;
    if user_row.bot_banned() {
        notify_banned(ctx).await?;
        return Ok(false);
    }

    let Some(guild_id) = ctx.guild_id() else {
        return Ok(true);
    };

    let guild_row = data.guilds_db.get(guild_id.into()).await?;
    let Some(required_role) = guild_row.required_role else {
        return Ok(true);
    };

    let member_roles = match ctx {
        Context::Application(poise::ApplicationContext { interaction, .. }) => {
            &interaction.member.as_deref().try_unwrap()?.roles
        }
        Context::Prefix(poise::PrefixContext { msg, .. }) => {
            &msg.member.as_deref().try_unwrap()?.roles
        }
    };

    if member_roles.contains(&required_role) || ctx.author_permissions()?.administrator() {
        return Ok(true);
    };

    let msg = aformat!(
        "You do not have the required role to use this bot, ask a server administrator for {}.",
        required_role.mention()
    );

    ctx.send_error(msg.as_str()).await?;
    Ok(false)
}
