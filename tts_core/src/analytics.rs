use std::{borrow::Cow, future::Future, pin::Pin};

use dashmap::DashMap;

use crate::{bool_enum, structs::Context};

bool_enum!(EventType(Normal | Command));

pub struct Handler {
    pub log_buffer: DashMap<(Cow<'static, str>, EventType), i32>,
    pub pool: sqlx::PgPool,
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
