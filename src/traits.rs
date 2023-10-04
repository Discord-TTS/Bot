use std::sync::Arc;

use poise::serenity_prelude as serenity;

use crate::opt_ext::{OptionTryUnwrap, OptionGettext};
use crate::{require_guild, constants};
use crate::structs::{Result, JoinVCToken, Context, TTSMode};
use crate::constants::{FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR};


#[serenity::async_trait]
pub trait PoiseContextExt {
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str;

    fn current_catalog(&self) -> Option<&gettext::Catalog>;
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>>;
    
    async fn neutral_colour(&self) -> u32;
    fn author_vc(&self) -> Option<serenity::ChannelId>;
    async fn author_permissions(&self) -> Result<serenity::Permissions>;
}

#[serenity::async_trait]
impl PoiseContextExt for Context<'_> {
    fn author_vc(&self) -> Option<serenity::ChannelId> {
        require_guild!(self, None)
            .voice_states
            .get(&self.author().id)
            .and_then(|vc| vc.channel_id)
    }

    async fn neutral_colour(&self) -> u32 {
        if let Some(guild_id) = self.guild_id() {
            let row = self.data().guilds_db.get(guild_id.get() as i64).await;
            if row.map(|row| row.voice_mode).map_or(false, TTSMode::is_premium) {
                return PREMIUM_NEUTRAL_COLOUR
            }
        }

        FREE_NEUTRAL_COLOUR
    }

    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str {
        self.current_catalog().gettext(translate)
    }

    fn current_catalog(&self) -> Option<&gettext::Catalog> {
        if let poise::Context::Application(ctx) = self {
            if let poise::CommandOrAutocompleteInteraction::Command(interaction) = ctx.interaction {
                return ctx.data.translations.get(match interaction.locale.as_str() {
                    "ko" => "ko-KR",
                    "pt-BR" => "pt",
                    l => l
                })
            }
        };

        None
    }

    async fn author_permissions(&self) -> Result<serenity::Permissions> {
        let ctx_discord = self.serenity_context();
        match ctx_discord.cache.channel(self.channel_id()).try_unwrap()? {
            serenity::Channel::Guild(channel) => {
                let member = channel.guild_id.member(ctx_discord, self.author()).await?;
                let guild = channel.guild(&ctx_discord.cache).try_unwrap()?;

                Ok(guild.user_permissions_in(&channel, &member))
            }
            _ => {
                Ok(((serenity::Permissions::from_bits_truncate(0b111_1100_1000_0000_0000_0111_1111_1000_0100_0000)
                    | serenity::Permissions::SEND_MESSAGES)
                    - serenity::Permissions::SEND_TTS_MESSAGES)
                    - serenity::Permissions::MANAGE_MESSAGES)
            }
        }
    }

    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>> {
        let author = self.author();
        let serenity_ctx = self.serenity_context();

        let m;
        let (name, avatar_url) = match self.channel_id().to_channel(serenity_ctx).await? {
            serenity::Channel::Guild(channel) => {
                let permissions = channel.permissions_for_user(serenity_ctx, serenity_ctx.cache.current_user().id)?;

                if !permissions.send_messages() {
                    return Ok(None);
                };

                if !permissions.embed_links() {
                    return self.send(poise::CreateReply::default()
                        .ephemeral(true)
                        .content("An Error Occurred! Please give me embed links permissions so I can tell you more!")
                    ).await.map(Some).map_err(Into::into)
                };

                match channel.guild_id.member(serenity_ctx, author.id).await {
                    Ok(member) => {
                        m = member;
                        (m.display_name(), m.face())
                    },
                    Err(_) => (author.name.as_str(), author.face()),
                }
            }
            serenity::Channel::Private(_) => (author.name.as_str(), author.face()),
            _ => unreachable!(),
        };

        match self.send(poise::CreateReply::default()
            .ephemeral(true)
            .embed(serenity::CreateEmbed::default()
                .colour(constants::RED)
                .title("An Error Occurred!")
                .author(serenity::CreateEmbedAuthor::new(name).icon_url(avatar_url))
                .description(format!(
                    "Sorry but {}, to fix this, please {error}!",
                    fix.unwrap_or("get in contact with us via the support server"),
                ))
                .footer(serenity::CreateEmbedFooter::new(format!(
                    "Support Server: {}", self.data().main_server_invite
                )))
            )
        ).await {
            Ok(handle) => Ok(Some(handle)),
            Err(_) => Ok(None)
        }
    }
}

#[serenity::async_trait]
pub trait SongbirdManagerExt {
    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>, songbird::error::JoinError>;
}

#[serenity::async_trait]
impl SongbirdManagerExt for songbird::Songbird {
    async fn join_vc(
        &self,
        guild_id: tokio::sync::MutexGuard<'_, JoinVCToken>,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>, songbird::error::JoinError> {
        match self.join(guild_id.0, channel_id).await {
            Ok(call) => Ok(call),
            Err(err) => {
                // On error, the Call is left in a semi-connected state.
                // We need to correct this by removing the call from the manager.
                drop(self.leave(guild_id.0).await);
                Err(err)
            }
        }
    }
}
