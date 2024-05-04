use std::{collections::HashMap, fmt::Write, num::NonZeroU16, sync::atomic::Ordering};

use self::serenity::builder::*;
use poise::serenity_prelude as serenity;

use tts_core::{
    constants::FREE_NEUTRAL_COLOUR,
    structs::{Data, FrameworkContext, Result},
};
use tts_tasks::Looper;

#[cfg(unix)]
fn clear_allocator_cache() {
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(unix))]
fn clear_allocator_cache() {}

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

async fn update_startup_message(
    ctx: &serenity::Context,
    data: &Data,
    user_name: &str,
    status: String,
    shard_count: Option<NonZeroU16>,
) -> Result<()> {
    let builder = serenity::EditWebhookMessage::default().content("").embed(
        CreateEmbed::default()
            .description(status)
            .colour(FREE_NEUTRAL_COLOUR)
            .title(if let Some(shard_count) = shard_count {
                format!("{user_name} is starting up {shard_count} shards!")
            } else {
                format!(
                    "{user_name} started in {} seconds",
                    data.start_time.elapsed().unwrap().as_secs()
                )
            }),
    );

    data.webhooks
        .logs
        .edit_message(&ctx.http, data.startup_message, builder)
        .await?;

    Ok(())
}

#[cold]
fn finalize_startup(ctx: &serenity::Context, data: &Data) {
    if let Some(bot_list_tokens) = data.bot_list_tokens.lock().take() {
        let stats_updater = tts_tasks::bot_list_updater::BotListUpdater::new(
            data.reqwest.clone(),
            ctx.cache.clone(),
            bot_list_tokens,
        );

        tokio::spawn(stats_updater.start());
    }

    if let Some(website_info) = data.website_info.lock().take() {
        let web_updater = tts_tasks::web_updater::Updater {
            patreon_service: data.config.patreon_service.clone(),
            reqwest: data.reqwest.clone(),
            cache: ctx.cache.clone(),
            pool: data.pool.clone(),
            config: website_info,
        };

        tokio::spawn(web_updater.start());
    }

    // Tell glibc to let go of the memory it's holding onto.
    // We are very unlikely to reach the peak of memory allocation that was just hit.
    clear_allocator_cache();
}

pub async fn ready(
    framework_ctx: FrameworkContext<'_>,
    data_about_bot: &serenity::Ready,
) -> Result<()> {
    let data = framework_ctx.user_data();
    let ctx = framework_ctx.serenity_context;

    let shard_count = ctx.cache.shard_count();
    let is_last_shard = (ctx.shard_id.0 + 1) == shard_count.get();

    // Don't update the welcome message for concurrent shard startups.
    if let Ok(_guard) = data.update_startup_lock.try_lock() {
        let status = generate_status(&*framework_ctx.shard_manager.runners.lock().await);
        let shard_count = (!is_last_shard).then_some(shard_count);

        update_startup_message(ctx, &data, &data_about_bot.user.name, status, shard_count).await?;
    }

    data.regex_cache
        .bot_mention
        .get_or_init(|| regex::Regex::new(&format!("^<@!?{}>$", data_about_bot.user.id)).unwrap());

    if is_last_shard && !data.fully_started.swap(true, Ordering::SeqCst) {
        finalize_startup(ctx, &data);
    }

    Ok(())
}
