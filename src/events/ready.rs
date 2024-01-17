use std::{
    collections::HashMap,
    fmt::Write,
    sync::{atomic::Ordering, Arc},
};

use self::serenity::builder::*;
use poise::serenity_prelude as serenity;

use crate::{
    bot_list_updater::BotListUpdater,
    constants::FREE_NEUTRAL_COLOUR,
    looper::Looper,
    structs::{FrameworkContext, Result},
    web_updater,
};

fn generate_status(shards: &HashMap<serenity::ShardId, serenity::ShardRunnerInfo>) -> String {
    let mut shards: Vec<_> = shards.iter().collect();
    shards.sort_by_key(|(id, _)| *id);

    let mut run_start = 0;
    let mut last_stage = None;
    let mut status = String::with_capacity(shards.len());

    for (i, (id, info)) in shards.iter().enumerate() {
        if Some(info.stage) == last_stage && i != (shards.len() - 1) {
            continue;
        }

        if let Some(last_stage) = last_stage {
            writeln!(status, "Shards {run_start}-{id}: {last_stage}").unwrap();
        }

        last_stage = Some(info.stage);
        run_start = id.0;
    }

    status
}

pub async fn ready(
    framework_ctx: FrameworkContext<'_>,
    data_about_bot: &serenity::Ready,
) -> Result<()> {
    let data = framework_ctx.user_data();
    let ctx = framework_ctx.serenity_context;

    let shard_count = ctx.cache.shard_count();
    let user_name = &data_about_bot.user.name;
    let last_shard = (ctx.shard_id.0 + 1) == shard_count.get();
    let status = generate_status(&*framework_ctx.shard_manager.runners.lock().await);

    data.webhooks
        .logs
        .edit_message(
            &ctx.http,
            data.startup_message,
            serenity::EditWebhookMessage::default().content("").embed(
                CreateEmbed::default()
                    .description(status)
                    .colour(FREE_NEUTRAL_COLOUR)
                    .title(if last_shard {
                        format!(
                            "{user_name} started in {} seconds",
                            data.start_time.elapsed().unwrap().as_secs()
                        )
                    } else {
                        format!("{user_name} is starting up {shard_count} shards!")
                    }),
            ),
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
