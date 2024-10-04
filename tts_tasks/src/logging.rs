use std::{collections::HashMap, fmt::Write, sync::Arc};

use aformat::{aformat, CapStr};
use anyhow::Result;
use itertools::Itertools as _;
use parking_lot::Mutex;

use serenity::all::{ExecuteWebhook, Http, Webhook};

use crate::Looper;

type LogMessage = (&'static str, String);

fn get_avatar(level: tracing::Level) -> &'static str {
    match level {
        tracing::Level::TRACE | tracing::Level::DEBUG => {
            "https://cdn.discordapp.com/embed/avatars/1.png"
        }
        tracing::Level::INFO => "https://cdn.discordapp.com/embed/avatars/0.png",
        tracing::Level::WARN => "https://cdn.discordapp.com/embed/avatars/3.png",
        tracing::Level::ERROR => "https://cdn.discordapp.com/embed/avatars/4.png",
    }
}

pub struct WebhookLogger {
    http: Arc<Http>,

    pending_logs: Mutex<HashMap<tracing::Level, Vec<LogMessage>>>,

    normal_logs: Webhook,
    error_logs: Webhook,
}

impl WebhookLogger {
    pub fn init(http: Arc<Http>, normal_logs: Webhook, error_logs: Webhook) {
        let logger = ArcWrapper(Arc::new(Self {
            http,
            normal_logs,
            error_logs,

            pending_logs: Mutex::default(),
        }));

        tracing::subscriber::set_global_default(logger.clone()).unwrap();
        tokio::spawn(logger.0.start());
    }
}

impl Looper for Arc<WebhookLogger> {
    const NAME: &'static str = "Logging";
    const MILLIS: u64 = 1100;

    async fn loop_func(&self) -> Result<()> {
        let pending_logs = self.pending_logs.lock().drain().collect::<HashMap<_, _>>();

        for (severity, messages) in pending_logs {
            let mut chunks: Vec<String> = Vec::with_capacity(messages.len());
            let mut pre_chunked = String::new();

            for (target, log_message) in messages {
                for line in log_message.lines() {
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

            let webhook_name = aformat!("TTS-Webhook [{}]", CapStr::<5>(severity.as_str()));

            for chunk in chunks {
                let builder = ExecuteWebhook::default()
                    .content(chunk)
                    .username(webhook_name.as_str())
                    .avatar_url(get_avatar(severity));

                webhook.execute(&self.http, false, builder).await?;
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

        impl tracing::field::Visit for StringVisitor<'_> {
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
        let target = metadata.target();
        if target.starts_with(env!("CARGO_CRATE_NAME")) {
            tracing::Level::INFO >= *metadata.level()
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
