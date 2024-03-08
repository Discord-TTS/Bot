use std::borrow::Cow;

use poise::serenity_prelude as serenity;
use serenity::{builder::*, small_fixed_array::FixedString};

use tts_core::{
    require,
    structs::{Context, TTSMode},
    translations::GetTextContextExt as _,
};

pub struct MenuPaginator<'a> {
    index: usize,
    mode: TTSMode,
    ctx: Context<'a>,
    pages: Vec<String>,
    footer: Cow<'a, str>,
    current_voice: String,
}

impl<'a> MenuPaginator<'a> {
    pub fn new(
        ctx: Context<'a>,
        pages: Vec<String>,
        current_voice: String,
        mode: TTSMode,
        footer: Cow<'a, str>,
    ) -> Self {
        Self {
            ctx,
            pages,
            current_voice,
            mode,
            footer,
            index: 0,
        }
    }

    fn create_page(&self, page: &str) -> CreateEmbed<'_> {
        let author = self.ctx.author();

        CreateEmbed::default()
            .title(
                self.ctx
                    .gettext("{bot_user} Voices | Mode: `{mode}`")
                    .replace("{mode}", self.mode.into())
                    .replace("{bot_user}", &self.ctx.cache().current_user().name),
            )
            .description(
                self.ctx
                    .gettext("**Currently Supported Voice**\n{page}")
                    .replace("{page}", page),
            )
            .field(
                self.ctx.gettext("Current voice used"),
                &self.current_voice,
                false,
            )
            .author(CreateEmbedAuthor::new(&*author.name).icon_url(author.face()))
            .footer(CreateEmbedFooter::new(self.footer.as_ref()))
    }

    fn create_action_row(&self, disabled: bool) -> serenity::CreateActionRow<'_> {
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
                            || (["⏮️", "◀"].contains(&emoji) && self.index == 0)
                            || (["▶️", "⏭️"].contains(&emoji)
                                && self.index == (self.pages.len() - 1)),
                    )
            })
            .collect();

        serenity::CreateActionRow::Buttons(buttons)
    }

    async fn create_message(&self) -> serenity::Result<serenity::Message> {
        self.ctx
            .send(
                poise::CreateReply::default()
                    .embed(self.create_page(&self.pages[self.index]))
                    .components(vec![self.create_action_row(false)]),
            )
            .await?
            .into_message()
            .await
    }

    async fn edit_message(
        &self,
        message: &mut serenity::Message,
        disable: bool,
    ) -> serenity::Result<()> {
        message
            .edit(
                self.ctx,
                EditMessage::default()
                    .embed(self.create_page(&self.pages[self.index]))
                    .components(vec![self.create_action_row(disable)]),
            )
            .await
    }

    pub async fn start(mut self) -> serenity::Result<()> {
        let mut message = self.create_message().await?;
        let serenity_context = self.ctx.serenity_context();

        loop {
            let builder = message
                .await_component_interaction(serenity_context.shard.clone())
                .timeout(std::time::Duration::from_secs(60 * 5))
                .author_id(self.ctx.author().id);

            let interaction = require!(builder.await, Ok(()));
            match interaction.data.custom_id.as_str() {
                "⏮️" => {
                    self.index = 0;
                    self.edit_message(&mut message, false).await?;
                }
                "◀" => {
                    self.index -= 1;
                    self.edit_message(&mut message, false).await?;
                }
                "⏹️" => {
                    self.edit_message(&mut message, true).await?;
                    return interaction.defer(&serenity_context.http).await;
                }
                "▶️" => {
                    self.index += 1;
                    self.edit_message(&mut message, false).await?;
                }
                "⏭️" => {
                    self.index = self.pages.len() - 1;
                    self.edit_message(&mut message, false).await?;
                }
                _ => unreachable!(),
            };
            interaction.defer(&serenity_context.http).await?;
        }
    }
}
