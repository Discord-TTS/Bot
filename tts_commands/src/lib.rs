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
#![feature(let_chains)]

use std::{borrow::Cow, num::NonZeroU16};

use serenity::all::{self as serenity, Mentionable as _};

use tts_core::{
    constants::PREMIUM_NEUTRAL_COLOUR,
    opt_ext::OptionTryUnwrap as _,
    require_guild,
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

    let main_msg =
        match data.premium_check(guild_id).await? {
            None => return Ok(true),
            Some(FailurePoint::Guild) => Cow::Borrowed("Hey, this is a premium command so it must be run in a server!"),
            Some(FailurePoint::PremiumUser) => Cow::Borrowed("Hey, this server isn't premium, please purchase TTS Bot Premium via Patreon! (`/donate`)"),
            Some(FailurePoint::NotSubscribed(premium_user_id)) => {
                let premium_user = premium_user_id.to_user(serenity_ctx).await?;
                Cow::Owned(format!(concat!(
                    "Hey, this server has a premium user setup, however they are not longer a patreon! ",
                    "Please ask {}#{} to renew their membership."
                ), premium_user.name, premium_user.discriminator.map_or(0, NonZeroU16::get)))
            }
        };

    let author = ctx.author();
    tracing::warn!(
        "{}#{} | {} failed the premium check in {}",
        author.name,
        author.discriminator.map_or(0, NonZeroU16::get),
        author.id,
        guild_id
            .and_then(|g_id| serenity_ctx.cache.guild(g_id))
            .map_or(Cow::Borrowed("DMs"), |g| (Cow::Owned(format!(
                "{} | {}",
                g.name, g.id
            ))))
    );

    let permissions = ctx.author_permissions().await?;
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
                builder.content(format!("{main_msg}\n{FOOTER_MSG}"))
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

    let member = ctx.author_member().await.try_unwrap()?;

    let is_admin = || {
        let guild = require_guild!(ctx, anyhow::Ok(false));
        let channel = guild.channels.get(&ctx.channel_id()).try_unwrap()?;

        let permissions = guild.user_permissions_in(channel, &member);
        Ok(permissions.administrator())
    };

    if member.roles.contains(&required_role) || is_admin()? {
        return Ok(true);
    };

    let msg = format!(
        "You do not have the required role to use this bot, ask a server administrator for {}.",
        required_role.mention()
    );

    ctx.send_error(msg).await?;
    Ok(false)
}
