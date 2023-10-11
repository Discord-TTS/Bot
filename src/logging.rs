use std::{collections::HashMap, fmt::Write, sync::Arc};

use anyhow::Result;
use itertools::Itertools as _;
use parking_lot::Mutex;

use poise::serenity_prelude::{async_trait, ExecuteWebhook, Http, Webhook};

type LogMessage = (&'static str, String);

pub struct WebhookLogger {
    http: Arc<Http>,
    log_prefix: &'static str,
    webhook_name: &'static str,
    max_verbosity: tracing::Level,
    level_lookup: HashMap<tracing::Level, String>,

    pending_logs: Mutex<HashMap<tracing::Level, Vec<LogMessage>>>,

    normal_logs: Webhook,
    error_logs: Webhook,
}

impl WebhookLogger {
    #[must_use]
    pub fn new(
        http: Arc<Http>,
        log_prefix: &'static str,
        webhook_name: &'static str,
        max_verbosity: tracing::Level,
        normal_logs: Webhook,
        error_logs: Webhook,
    ) -> ArcWrapper<Self> {
        let level_lookup = HashMap::from_iter(
            [
                (tracing::Level::TRACE, 1),
                (tracing::Level::DEBUG, 1),
                (tracing::Level::INFO, 0),
                (tracing::Level::WARN, 3),
                (tracing::Level::ERROR, 4),
            ]
            .map(|(level, value)| {
                (
                    level,
                    format!("https://cdn.discordapp.com/embed/avatars/{value}.png"),
                )
            }),
        );

        ArcWrapper(Arc::new(Self {
            http,
            max_verbosity,
            level_lookup,
            normal_logs,
            error_logs,
            webhook_name,
            log_prefix,
            pending_logs: Mutex::default(),
        }))
    }
}

#[async_trait]
impl crate::looper::Looper for WebhookLogger {
    const NAME: &'static str = "Logging";
    const MILLIS: u64 = 1100;

    async fn loop_func(&self) -> Result<()> {
        let pending_logs = self.pending_logs.lock().drain().collect::<HashMap<_, _>>();

        for (severity, messages) in pending_logs {
            let mut chunks: Vec<String> = Vec::with_capacity(messages.len());
            let mut pre_chunked = String::new();

            for (target, log_message) in messages {
                for line in log_message.trim().split('\n') {
                    writeln!(pre_chunked, "`[{target}]`: {line}")?;
                }
            }

            for line in pre_chunked.split_inclusive('\n') {
                for chunk in line
                    .chars()
                    .chunks(2000)
                    .into_iter()
                    .map(Iterator::collect::<String>)
                {
                    if let Some(current_chunk) = chunks.last_mut() {
                        if current_chunk.len() + chunk.len() > 2000 {
                            chunks.push(chunk);
                        } else {
                            current_chunk.push_str(&chunk);
                        }
                    } else {
                        chunks.push(chunk);
                    }
                }
            }

            let webhook = if tracing::Level::ERROR >= severity {
                &self.error_logs
            } else {
                &self.normal_logs
            };

            let severity_str = severity.as_str();
            let mut webhook_name =
                String::with_capacity(self.webhook_name.len() + 3 + severity_str.len());
            webhook_name.push_str(self.webhook_name);
            webhook_name.push_str(" [");
            webhook_name.push_str(severity_str);
            webhook_name.push(']');

            for chunk in chunks {
                webhook
                    .execute(
                        &self.http,
                        false,
                        ExecuteWebhook::default()
                            .content(chunk)
                            .username(webhook_name.clone())
                            .avatar_url(self.level_lookup.get(&severity).cloned().unwrap_or_else(
                                || String::from("https://cdn.discordapp.com/embed/avatars/5.png"),
                            )),
                    )
                    .await?;
            }
        }

        Ok(())
    }
}

impl tracing::Subscriber for ArcWrapper<WebhookLogger> {
    // Hopefully this works
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        pub struct StringVisitor<'a> {
            string: &'a mut String,
        }

        impl<'a> tracing::field::Visit for StringVisitor<'a> {
            fn record_debug(
                &mut self,
                _field: &tracing::field::Field,
                value: &dyn std::fmt::Debug,
            ) {
                write!(self.string, "{value:?}").unwrap();
            }

            fn record_str(&mut self, _field: &tracing::field::Field, value: &str) {
                self.string.push_str(value);
            }
        }

        let mut message = String::new();
        event.record(&mut StringVisitor {
            string: &mut message,
        });

        let metadata = event.metadata();
        self.pending_logs
            .lock()
            .entry(*metadata.level())
            .or_default()
            .push((metadata.target(), message));
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        // Ordered by verbosity
        if ["gnomeutils", self.log_prefix]
            .into_iter()
            .any(|t| metadata.target().starts_with(t))
        {
            self.max_verbosity >= *metadata.level()
        } else {
            tracing::Level::WARN >= *metadata.level()
        }
    }
}

// So we can impl tracing::Subscriber for Arc<WebhookLogger>
pub struct ArcWrapper<T>(pub Arc<T>);
impl<T> Clone for ArcWrapper<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::ops::Deref for ArcWrapper<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
