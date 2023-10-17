use std::sync::{atomic::Ordering, Arc};

use self::serenity::builder::*;
use poise::serenity_prelude as serenity;

use crate::{
    bot_list_updater::BotListUpdater,
    constants::FREE_NEUTRAL_COLOUR,
    funcs::generate_status,
    looper::Looper,
    structs::{FrameworkContext, Result},
    web_updater,
};

pub async fn ready(
    framework_ctx: FrameworkContext<'_>,
    ctx: &serenity::Context,
    data_about_bot: &serenity::Ready,
) -> Result<()> {
    let data = framework_ctx.user_data;

    let user_name = &data_about_bot.user.name;
    let last_shard = (ctx.shard_id.0 + 1) == ctx.cache.shard_count();
    let status = generate_status(&*framework_ctx.shard_manager.runners.lock().await);

    data.webhooks
        .logs
        .edit_message(
            &ctx.http,
            data.startup_message,
            serenity::EditWebhookMessage::default()
                .content("")
                .embeds(vec![CreateEmbed::default()
                    .description(status)
                    .colour(FREE_NEUTRAL_COLOUR)
                    .title(if last_shard {
                        format!(
                            "{user_name} started in {} seconds",
                            data.start_time.elapsed().unwrap().as_secs()
                        )
                    } else {
                        format!("{user_name} is starting up!")
                    })]),
        )
        .await?;

    if last_shard && !data.fully_started.load(Ordering::SeqCst) {
        data.fully_started.store(true, Ordering::SeqCst);
        let stats_updater = Arc::new(BotListUpdater::new(
            data.reqwest.clone(),
            ctx.cache.clone(),
            data.bot_list_tokens.clone(),
        ));

        if let Some(website_info) = data.website_info.write().take() {
            let web_updater = Arc::new(web_updater::Updater {
                patreon_service: data.config.patreon_service.clone(),
                reqwest: data.reqwest.clone(),
                cache: ctx.cache.clone(),
                pool: data.pool.clone(),
                config: website_info,
            });

            tokio::spawn(web_updater.start());
        }

        tokio::spawn(stats_updater.start());
    }

    Ok(())
}
