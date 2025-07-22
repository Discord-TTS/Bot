use std::{collections::HashMap, fmt::Write, sync::Arc};

use aformat::{CapStr, aformat};
use anyhow::Result;
use itertools::Itertools as _;
use parking_lot::Mutex;
use tracing_subscriber::{Registry, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use serenity::all::{ExecuteWebhook, Http, Webhook};

use crate::Looper;

type LogMessage = (&'static str, String);
pub trait Layer = tracing_subscriber::Layer<Registry> + Send + Sync + 'static;

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
    pub fn init(
        console_layer: impl Layer,
        http: Arc<Http>,
        normal_logs: Webhook,
        error_logs: Webhook,
    ) {
        let logger = ArcWrapper(Arc::new(Self {
            http,
            normal_logs,
            error_logs,

            pending_logs: Mutex::default(),
        }));

        tracing_subscriber::registry()
            .with(console_layer)
            .with(logger.clone())
            .init();

        tokio::spawn(logger.0.start());
    }
}

impl Looper for Arc<WebhookLogger> {
    const NAME: &'static str = "Logging";
    const MILLIS: u64 = 1100;

    type Error = !;
    async fn loop_func(&self) -> Result<(), Self::Error> {
        let pending_logs = self.pending_logs.lock().drain().collect::<HashMap<_, _>>();

        for (severity, messages) in pending_logs {
            let mut chunks: Vec<String> = Vec::with_capacity(messages.len());
            let mut pre_chunked = String::new();

            for (target, log_message) in messages {
                for line in log_message.lines() {
                    writeln!(pre_chunked, "`[{target}]`: {line}")
                        .expect("String::write_fmt should never not fail");
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
                    .content(&chunk)
                    .username(webhook_name.as_str())
                    .avatar_url(get_avatar(severity));

                if let Err(err) = webhook.execute(&self.http, false, builder).await {
                    eprintln!("Failed to send log message: {err:?}\n{chunk}");
                }
            }
        }

        Ok(())
    }
}

pub struct StringVisitor<'a> {
    string: &'a mut String,
}

impl tracing::field::Visit for StringVisitor<'_> {
    fn record_debug(&mut self, _field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        write!(self.string, "{value:?}").unwrap();
    }

    fn record_str(&mut self, _field: &tracing::field::Field, value: &str) {
        self.string.push_str(value);
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ArcWrapper<WebhookLogger> {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        let enabled = if metadata.target().starts_with("tts_") {
            tracing::Level::INFO >= *metadata.level()
        } else {
            tracing::Level::WARN >= *metadata.level()
        };

        if !enabled {
            return;
        }

        let mut message = String::new();
        event.record(&mut StringVisitor {
            string: &mut message,
        });

        self.pending_logs
            .lock()
            .entry(*metadata.level())
            .or_default()
            .push((metadata.target(), message));
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
