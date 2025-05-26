use std::borrow::Cow;

use poise::serenity_prelude as serenity;

use tts_core::{
    errors,
    structs::{Data, Result},
};

pub fn resume(ctx: &serenity::Context) {
    tracing::info!("Shard {} has resumed", ctx.shard_id);

    let data = ctx.data_ref::<Data>();
    data.analytics.log(Cow::Borrowed("resumed"), false);
}

pub async fn interaction_create(
    ctx: &serenity::Context,
    interaction: &serenity::Interaction,
) -> Result<()> {
    errors::interaction_create(ctx, interaction).await
}
