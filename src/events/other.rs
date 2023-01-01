use std::borrow::Cow;

use poise::serenity_prelude as serenity;

use crate::structs::{Data, FrameworkContext, Result};

pub fn resume(data: &Data) {
    data.analytics.log(Cow::Borrowed("resumed"), false);
}

pub async fn interaction_create(framework_ctx: FrameworkContext<'_>, ctx: &serenity::Context, interaction: &serenity::Interaction) -> Result<()> {
    gnomeutils::errors::interaction_create(ctx, interaction, framework_ctx).await
}
