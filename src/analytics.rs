use std::{borrow::Cow, future::Future, pin::Pin, sync::Arc};

use dashmap::DashMap;
use sqlx::Connection;

use crate::structs::Context;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EventType {
    Normal,
    Command,
}

impl From<bool> for EventType {
    fn from(is_command: bool) -> Self {
        if is_command {
            EventType::Command
        } else {
            EventType::Normal
        }
    }
}

pub struct Handler {
    log_buffer: DashMap<(Cow<'static, str>, EventType), i32>,
    pool: sqlx::PgPool,
}

impl Handler {
    #[must_use]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            pool,
            log_buffer: DashMap::new(),
        }
    }

    pub fn log(&self, event: Cow<'static, str>, kind: impl Into<EventType>) {
        let key = (event, kind.into());

        let count = (*self.log_buffer.entry(key.clone()).or_insert(0)) + 1;
        self.log_buffer.insert(key, count);
    }
}

impl crate::Looper for Arc<Handler> {
    const NAME: &'static str = "Analytics";
    const MILLIS: u64 = 5000;

    async fn loop_func(&self) -> anyhow::Result<()> {
        let log_buffer = self.log_buffer.clone();
        self.log_buffer.clear();

        let mut conn = self.pool.acquire().await?;
        conn.transaction(move |transaction| {
            Box::pin(async {
                for ((event, kind), count) in log_buffer {
                    let query = sqlx::query(
                        "
                    INSERT INTO analytics(event, is_command, count)
                    VALUES($1, $2, $3)
                    ON CONFLICT ON CONSTRAINT analytics_pkey
                    DO UPDATE SET count = analytics.count + EXCLUDED.count
                ;",
                    );

                    query
                        .bind(event)
                        .bind(kind == EventType::Command)
                        .bind(count)
                        .execute(&mut **transaction)
                        .await?;
                }

                Ok(())
            })
        })
        .await
    }
}

pub fn pre_command(ctx: Context<'_>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    let analytics_handler = &ctx.data().analytics;

    analytics_handler.log(Cow::Owned(ctx.command().qualified_name.clone()), true);
    analytics_handler.log(
        Cow::Borrowed(match ctx {
            poise::Context::Prefix(_) => "command",
            poise::Context::Application(ctx) => match ctx.interaction_type {
                poise::CommandInteractionType::Autocomplete => "autocomplete",
                poise::CommandInteractionType::Command => "slash_command",
            },
        }),
        false,
    );

    // TODO: Replace with futures::future::always_ready
    // once https://github.com/rust-lang/futures-rs/pull/2825 is merged.
    Box::pin(async {})
}
