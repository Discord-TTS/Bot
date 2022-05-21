use std::borrow::Cow;
use std::sync::Arc;

use lazy_static::lazy_static;
use regex::Regex;

use poise::serenity_prelude as serenity;

use crate::structs::{Result, JoinVCToken, Context, TTSMode};
use crate::constants::{RED, FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};


pub trait OptionTryUnwrap<T> {
    fn try_unwrap(self) -> Result<T>;
}
pub trait OptionGettext<'a> {
    fn gettext(self, translate: &'a str) -> &'a str;
}

#[serenity::async_trait]
pub trait PoiseContextExt {
    async fn neutral_colour(&self) -> u32;
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str;
    fn current_catalog(&self) -> Option<&gettext::Catalog>;
    async fn author_permissions(&self) -> Result<serenity::Permissions>;
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>>;
}
#[serenity::async_trait]
pub trait SerenityContextExt {
    async fn user_from_dm(&self, dm_name: &str) -> Option<serenity::User>;
    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>>;
}


#[serenity::async_trait]
impl PoiseContextExt for Context<'_> {
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str {
        self.current_catalog().gettext(translate)
    }

    fn current_catalog(&self) -> Option<&gettext::Catalog> {
        let catalog = if
            let poise::Context::Application(ctx) = self &&
            let poise::ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction) = ctx.interaction
        {
            ctx.data.translations.get(match interaction.locale.as_str() {
                "ko" => "ko-KR",
                "pt-BR" => "pt",
                l => l
            })
        } else {
            None
        };

        catalog.or_else(|| self.data().default_catalog())
    }

    async fn neutral_colour(&self) -> u32 {
        if let Some(guild_id) = self.guild_id() {
            let row = self.data().guilds_db.get(guild_id.0 as i64).await;
            if row.map(|row| row.voice_mode).map_or(false, TTSMode::is_premium) {
                return PREMIUM_NEUTRAL_COLOUR
            }
        }

        FREE_NEUTRAL_COLOUR
    }

    async fn author_permissions(&self) -> Result<serenity::Permissions> {
        let ctx_discord = self.discord();

        match ctx_discord.cache.channel(self.channel_id()).try_unwrap()? {
            serenity::Channel::Guild(channel) => {
                let guild = channel.guild(&ctx_discord.cache).try_unwrap()?;
                let member = guild.member(ctx_discord, self.author()).await?;

                Ok(guild.user_permissions_in(&channel, &member)?)
            }
            _ => {
                Ok(((serenity::Permissions::from_bits_truncate(0b111110010000000000001111111100001000000)
                    | serenity::Permissions::SEND_MESSAGES)
                    - serenity::Permissions::SEND_TTS_MESSAGES)
                    - serenity::Permissions::MANAGE_MESSAGES)
            }
        }
    }

    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>> {
        let author = self.author();
        let ctx_discord = self.discord();

        let m;
        let (name, avatar_url) = match self.channel_id().to_channel(ctx_discord).await? {
            serenity::Channel::Guild(channel) => {
                let permissions = channel.permissions_for_user(ctx_discord, ctx_discord.cache.current_user_id())?;

                if !permissions.send_messages() {
                    return Ok(None);
                };

                if !permissions.embed_links() {
                    return self.send(|b| {b
                        .ephemeral(true)
                        .content("An Error Occurred! Please give me embed links permissions so I can tell you more!")
                    }).await.map(Some).map_err(Into::into)
                };

                match channel.guild_id.member(ctx_discord, author.id).await {
                    Ok(member) => {
                        m = member;
                        (m.display_name(), m.face())
                    },
                    Err(_) => (Cow::Borrowed(&author.name), author.face()),
                }
            }
            serenity::Channel::Private(_) => (Cow::Borrowed(&author.name), author.face()),
            _ => unreachable!(),
        };


        match self.send(|b| b
            .ephemeral(true)
            .embed(|e| e
                .colour(RED)
                .title("An Error Occurred!")
                .description(format!(
                    "Sorry but {}, to fix this, please {}!", error,
                    fix.unwrap_or("get in contact with us via the support server"),
                ))
                .author(|a| a
                    .name(name.into_owned())
                    .icon_url(avatar_url)
                )
                .footer(|f| f.text(format!(
                    "Support Server: {}", self.data().config.main_server_invite
                )))
            )
        ).await {
            Ok(handle) => Ok(Some(handle)),
            Err(_) => Ok(None)
        }
    }
}

#[serenity::async_trait]
impl SerenityContextExt for serenity::Context {
    async fn user_from_dm(&self, dm_name: &str) -> Option<serenity::User> {
        lazy_static! {
            static ref ID_IN_BRACKETS_REGEX: Regex = Regex::new(r"\((\d+)\)").unwrap();
        }

        let re_match = ID_IN_BRACKETS_REGEX.captures(dm_name)?;
        let user_id: u64 = re_match.get(1)?.as_str().parse().ok()?;
        self.http.get_user(user_id).await.ok()
    }

    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>> {
        let manager = songbird::get(self).await.unwrap();
        let (call, r) = manager.join(guild_id.0, channel_id).await;
        r?;
        Ok(call)
    }
}

impl<T> OptionTryUnwrap<T> for Option<T> {
    #[track_caller]
    fn try_unwrap(self) -> Result<T> {
        match self {
            Some(v) => Ok(v),
            None => Err({
                let location = std::panic::Location::caller();
                anyhow::anyhow!("Unexpected None value on line {} in {}", location.line(), location.file())
            })
        }
    }
}

impl<'a> OptionGettext<'a> for Option<&'a gettext::Catalog> {
    fn gettext(self, translate: &'a str) -> &'a str {
        self.map_or(translate, |c| c.gettext(translate))
    }
}
