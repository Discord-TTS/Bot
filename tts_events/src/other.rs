use std::borrow::Cow;

use poise::serenity_prelude as serenity;

use tts_core::{
    errors,
    structs::{Data, FrameworkContext, Result},
};

pub fn resume(data: &Data) {
    data.analytics.log(Cow::Borrowed("resumed"), false);
}

pub async fn interaction_create(
    framework_ctx: FrameworkContext<'_>,
    interaction: &serenity::Interaction,
) -> Result<()> {
    errors::interaction_create(framework_ctx, interaction).await
}
