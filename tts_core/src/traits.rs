use std::{borrow::Cow, sync::Arc};

use poise::serenity_prelude as serenity;

use crate::{
    constants,
    constants::{FREE_NEUTRAL_COLOUR, PREMIUM_NEUTRAL_COLOUR},
    opt_ext::OptionTryUnwrap,
    require_guild,
    structs::{Context, JoinVCToken, Result, TTSMode},
};

pub trait PoiseContextExt<'ctx> {
    async fn send_error(
        &'ctx self,
        error_message: impl Into<Cow<'ctx, str>>,
    ) -> Result<Option<poise::ReplyHandle<'ctx>>>;
    async fn send_ephemeral(
        &'ctx self,
        message: impl Into<Cow<'ctx, str>>,
    ) -> Result<poise::ReplyHandle<'ctx>>;

    async fn neutral_colour(&self) -> u32;
    fn author_vc(&self) -> Option<serenity::ChannelId>;
    fn author_permissions(&self) -> Result<serenity::Permissions>;
}

impl<'ctx> PoiseContextExt<'ctx> for Context<'ctx> {
    fn author_vc(&self) -> Option<serenity::ChannelId> {
        require_guild!(self, None)
            .voice_states
            .get(&self.author().id)
            .and_then(|vc| vc.channel_id)
    }

    async fn neutral_colour(&self) -> u32 {
        if let Some(guild_id) = self.guild_id() {
            let row = self.data().guilds_db.get(guild_id.get() as i64).await;
            if row
                .map(|row| row.voice_mode)
                .map_or(false, TTSMode::is_premium)
            {
                return PREMIUM_NEUTRAL_COLOUR;
            }
        }

        FREE_NEUTRAL_COLOUR
    }

    fn author_permissions(&self) -> Result<serenity::Permissions> {
        match self {
            poise::Context::Application(poise::ApplicationContext { interaction, .. }) => {
                let channel = interaction.channel.as_ref().try_unwrap()?;
                let Some(author_member) = interaction.member.as_deref() else {
                    return Ok(serenity::Permissions::dm_permissions());
                };

                let mut permissions = author_member.permissions.try_unwrap()?;
                if matches!(channel, serenity::GenericInteractionChannel::Thread(_)) {
                    permissions.set(
                        serenity::Permissions::SEND_MESSAGES,
                        permissions.send_messages_in_threads(),
                    );
                }

                Ok(permissions)
            }
            poise::Context::Prefix(poise::PrefixContext { msg, .. }) => {
                msg.author_permissions(self.cache()).try_unwrap()
            }
        }
    }

    async fn send_ephemeral(
        &'ctx self,
        message: impl Into<Cow<'ctx, str>>,
    ) -> Result<poise::ReplyHandle<'ctx>> {
        let reply = poise::CreateReply::default().content(message);
        let handle = self.send(reply).await?;
        Ok(handle)
    }

    #[cold]
    async fn send_error(
        &'ctx self,
        error_message: impl Into<Cow<'ctx, str>>,
    ) -> Result<Option<poise::ReplyHandle<'ctx>>> {
        let author = self.author();
        let guild_id = self.guild_id();
        let serenity_ctx = self.serenity_context();

        let (name, avatar_url) = match self.channel_id().to_channel(serenity_ctx, guild_id).await? {
            serenity::Channel::Guild(channel) => {
                if !self.author_permissions()?.embed_links() {
                    self.send(poise::CreateReply::new()
                        .content("An Error Occurred! Please give me embed links permissions so I can tell you more!")
                        .ephemeral(true)
                    ).await?;

                    return Ok(None);
                };

                let member = match self {
                    Self::Application(ctx) => ctx.interaction.member.as_deref().map(Cow::Borrowed),
                    Self::Prefix(_) => {
                        let member = channel.base.guild_id.member(serenity_ctx, author.id).await;
                        member.ok().map(Cow::Owned)
                    }
                };

                match member {
                    Some(m) => (Cow::Owned(m.display_name().to_owned()), m.face()),
                    None => (Cow::Borrowed(&*author.name), author.face()),
                }
            }
            serenity::Channel::Private(_) => (Cow::Borrowed(&*author.name), author.face()),
            _ => unreachable!(),
        };

        match self
            .send(
                poise::CreateReply::default().ephemeral(true).embed(
                    serenity::CreateEmbed::default()
                        .colour(constants::RED)
                        .title("An Error Occurred!")
                        .author(serenity::CreateEmbedAuthor::new(name).icon_url(avatar_url))
                        .description(error_message)
                        .footer(serenity::CreateEmbedFooter::new(format!(
                            "Support Server: {}",
                            self.data().config.main_server_invite
                        ))),
                ),
            )
            .await
        {
            Ok(handle) => Ok(Some(handle)),
            Err(_) => Ok(None),
        }
    }
}

pub trait SongbirdManagerExt {
    async fn join_vc(
        &self,
        guild_id: JoinVCToken,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>, songbird::error::JoinError>;
}

impl SongbirdManagerExt for songbird::Songbird {
    async fn join_vc(
        &self,
        JoinVCToken(guild_id, lock): JoinVCToken,
        channel_id: serenity::ChannelId,
    ) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>, songbird::error::JoinError> {
        let _guard = lock.lock().await;
        match self.join(guild_id, channel_id).await {
            Ok(call) => Ok(call),
            Err(err) => {
                // On error, the Call is left in a semi-connected state.
                // We need to correct this by removing the call from the manager.
                drop(self.leave(guild_id).await);
                Err(err)
            }
        }
    }
}
