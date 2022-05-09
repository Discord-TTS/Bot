// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{collections::HashMap, fmt::Write, sync::Arc, borrow::Cow};

use parking_lot::Mutex;

use poise::serenity_prelude as serenity;

use crate::structs::Result;


type LogMessage = (&'static str, String);

pub struct WebhookLogger {
    http: Arc<serenity::Http>,
    max_verbosity: tracing::Level,
    level_lookup: HashMap<tracing::Level, String>,

    pending_logs: Arc<Mutex<HashMap<tracing::Level, Vec<LogMessage>>>>,

    normal_logs: serenity::Webhook,
    error_logs: serenity::Webhook,
}

impl WebhookLogger {
    pub fn new(
        http: Arc<serenity::Http>,
        max_verbosity: tracing::Level,
        normal_logs: serenity::Webhook,
        error_logs: serenity::Webhook,
    ) -> ArcWrapper<Self> {
        let level_lookup = HashMap::from_iter([
            (tracing::Level::TRACE, 1),
            (tracing::Level::DEBUG, 1),
            (tracing::Level::INFO, 0),
            (tracing::Level::WARN, 3),
            (tracing::Level::ERROR, 4),
        ].map(|(level, value)| (level, format!("https://cdn.discordapp.com/embed/avatars/{value}.png"))));

        ArcWrapper(Arc::new(Self {
            http, max_verbosity, level_lookup, normal_logs, error_logs,
            pending_logs: Arc::default(),
        }))
    }

    pub async fn listener(&self) {
        let mut periodic = tokio::time::interval(std::time::Duration::from_millis(1100));

        loop {
            periodic.tick().await;
            if let Err(error) = self.send_buffer().await {
                eprintln!("Logging Error: {:?}", error);
            }
        }
    }

    async fn send_buffer(&self) -> Result<()> {
        let pending_logs = self.pending_logs.lock().drain().collect::<HashMap<_, _>>();

        for (severity, messages) in pending_logs {
            let mut chunks: Vec<Cow<'_, str>> = Vec::with_capacity(messages.len());
            let pre_chunked: String = messages
                .into_iter()
                .map(|(target, log_message)| {
                    log_message.trim().split('\n').map(move |line| {
                        format!("`[{}]`: {}\n", target, line)
                    }).collect::<String>()
                })
                .collect();

            for line in pre_chunked.split_inclusive('\n') {
                if let Some(chunk) = chunks.last_mut() {
                    if chunk.len() + line.len() > 2000 {
                        chunks.push(Cow::Borrowed(line));
                    } else {
                        chunk.to_mut().push_str(line);
                    }
                } else {
                    chunks.push(Cow::Borrowed(line));
                }
            }

            let webhook = if tracing::Level::ERROR >= severity {
                &self.error_logs
            } else {
                &self.normal_logs
            };

            for chunk in chunks {
                webhook.execute(&self.http, false, |b| b
                    .content(chunk)
                    .username(format!("TTS-Webhook [{}]", severity.as_str()))
                    .avatar_url(self.level_lookup.get(&severity).cloned().unwrap_or_else(|| String::from(
                        "https://cdn.discordapp.com/embed/avatars/5.png",
                    )))
                ).await?;
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
            fn record_debug(&mut self, _field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                write!(self.string, "{:?}", value).unwrap();
            }

            fn record_str(&mut self, _field: &tracing::field::Field, value: &str) {
                self.string.push_str(value);
            }
        }

        let mut message = String::new();
        event.record(&mut StringVisitor {string: &mut message});

        let metadata = event.metadata();
        self.pending_logs
            .lock()
            .entry(*metadata.level())
            .or_insert_with(Vec::new)
            .push((metadata.target(), message));
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        // Ordered by verbosity
        if metadata.target().starts_with("discord_tts_bot") {
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
