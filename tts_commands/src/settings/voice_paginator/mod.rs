use std::borrow::Cow;

use poise::serenity_prelude as serenity;
use serenity::{CollectComponentInteractions, builder::*, small_fixed_array::FixedString};

use tts_core::structs::{Context, TTSMode};

use cursor::PageCursor;

mod cursor;

pub struct MenuPaginator<'a> {
    mode: TTSMode,
    ctx: Context<'a>,
    pages: PageCursor,
    footer: Cow<'a, str>,
    current_voice: String,
}

impl<'a> MenuPaginator<'a> {
    pub fn new(
        ctx: Context<'a>,
        pages: Box<[String]>,
        current_voice: String,
        mode: TTSMode,
        footer: Cow<'a, str>,
    ) -> Self {
        Self {
            ctx,
            current_voice,
            mode,
            footer,
            pages: PageCursor::new(pages),
        }
    }

    fn create_page(&self, page: &str) -> CreateEmbed<'_> {
        let author = self.ctx.author();
        let bot_user = &self.ctx.cache().current_user().name;

        CreateEmbed::default()
            .title(format!("{bot_user} Voices | Mode: `{}`", self.mode))
            .description(format!("**Currently Supported Voice**\n{page}"))
            .field("Current voice used", &self.current_voice, false)
            .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
            .footer(CreateEmbedFooter::new(self.footer.as_ref()))
    }

    fn create_action_row(&self, disabled: bool) -> serenity::CreateComponent<'_> {
        let buttons = ["⏮️", "◀", "⏹️", "▶️", "⏭️"]
            .into_iter()
            .map(|emoji| {
                CreateButton::new(emoji)
                    .style(serenity::ButtonStyle::Primary)
                    .emoji(serenity::ReactionType::Unicode(
                        FixedString::from_static_trunc(emoji),
                    ))
                    .disabled(
                        disabled
                            || (["⏮️", "◀"].contains(&emoji) && !self.pages.can_rewind())
                            || (["▶️", "⏭️"].contains(&emoji) && !self.pages.can_advance()),
                    )
            })
            .collect();

        CreateComponent::ActionRow(CreateActionRow::Buttons(buttons))
    }

    async fn create_message(&self) -> serenity::Result<serenity::MessageId> {
        let components = [self.create_action_row(false)];
        let builder = poise::CreateReply::default()
            .embed(self.create_page(self.pages.current()))
            .components(&components);

        self.ctx.send(builder).await?.message().await.map(|m| m.id)
    }

    async fn edit_message(
        &self,
        interaction: serenity::ComponentInteraction,
        disable: bool,
    ) -> serenity::Result<()> {
        let components = [self.create_action_row(disable)];
        let builder = CreateInteractionResponseMessage::default()
            .embed(self.create_page(self.pages.current()))
            .components(&components);

        let response = CreateInteractionResponse::UpdateMessage(builder);
        interaction.create_response(self.ctx.http(), response).await
    }

    pub async fn start(mut self) -> serenity::Result<()> {
        let message_id = self.create_message().await?;
        let serenity_context = self.ctx.serenity_context();

        loop {
            let builder = message_id
                .collect_component_interactions(serenity_context)
                .timeout(std::time::Duration::from_mins(5))
                .author_id(self.ctx.author().id);

            let Some(interaction) = builder.await else {
                break Ok(());
            };

            match interaction.data.custom_id.as_str() {
                "⏮️" => {
                    self.pages.jump_start();
                    self.edit_message(interaction, false).await?;
                }
                "◀" => {
                    self.pages.rewind();
                    self.edit_message(interaction, false).await?;
                }
                "⏹️" => {
                    return self.edit_message(interaction, true).await;
                }
                "▶️" => {
                    self.pages.advance();
                    self.edit_message(interaction, false).await?;
                }
                "⏭️" => {
                    self.pages.jump_end();
                    self.edit_message(interaction, false).await?;
                }
                _ => unreachable!(),
            }
        }
    }
}
