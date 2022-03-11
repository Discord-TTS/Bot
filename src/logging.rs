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

use std::fmt::Write;
use std::sync::mpsc::{Receiver, Sender};
use std::{collections::HashMap, sync::Arc};

use poise::serenity_prelude as serenity;
use itertools::Itertools;
use parking_lot::Mutex;
use strfmt::strfmt;

use crate::structs::Error;

// This is split up into two structs as listener needs to be run in the background
// and the sender needs to be given to tracing as a subscriber, so mspc is used as
// an easy way to send the log messages from sync to async land.

// [target, message]
type LogMessage = [String; 2];

pub struct WebhookLogRecv {
    prefix: String,
    http: Arc<serenity::Http>,
    level_lookup: HashMap<tracing::Level, String>,

    rx: Mutex<Receiver<(tracing::Level, LogMessage)>>,

    normal_logs: serenity::Webhook,
    error_logs: serenity::Webhook,
}

impl WebhookLogRecv {
    pub fn new(
        rx: Receiver<(tracing::Level, LogMessage)>,
        http: Arc<serenity::Http>,
        prefix: String,
        normal_logs: serenity::Webhook,
        error_logs: serenity::Webhook,
    ) -> Self {
        let level_lookup_raw = [
            (tracing::Level::TRACE, 1),
            (tracing::Level::DEBUG, 1),
            (tracing::Level::INFO, 0),
            (tracing::Level::WARN, 3),
            (tracing::Level::ERROR, 4),
        ];

        let mut level_lookup = HashMap::new();
        for (level_enum, value) in level_lookup_raw {
            level_lookup.insert(
                level_enum,
                format!("https://cdn.discordapp.com/embed/avatars/{}.png", value),
            );
        }

        Self {
            http,
            prefix,
            level_lookup,
            error_logs,
            normal_logs,

            rx: Mutex::new(rx),
        }
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

    async fn send_buffer(&self) -> Result<(), Error> {
        let recv_buf: Vec<(tracing::Level, LogMessage)> = self.rx.lock().try_iter().collect();
        let mut message_buf: HashMap<tracing::Level, Vec<[String; 2]>> =
            HashMap::with_capacity(recv_buf.len());

        for (level, [target, msg]) in recv_buf {
            let messages = message_buf.get_mut(&level);
            match messages {
                Some(messages) => messages.push([target, msg]),
                None => {message_buf.insert(level, vec![[target, msg]]);}
            };
        }

        for (severity, messages) in &message_buf {
            let severity_name = severity.as_str();
            let username = format!("TTS-Webhook [{}]", severity_name);

            let format_string = format!("`{} [{{target}}]`: {}\n",
                self.prefix,
                if severity <= &tracing::Level::WARN {
                    "**{line}**"
                } else {
                    "{line}"
                }
            );

            let message_chunked: Vec<String> =
                messages.iter().flat_map(|[target, log_message]| {
                    log_message.trim().split('\n').map(|line| {
                        strfmt(
                            &format_string,
                            &HashMap::from_iter([
                                (String::from("line"), line),
                                (String::from("target"), target)
                            ]),
                        ).unwrap()
                    })
                })
                .collect::<String>()
                .chars().chunks(2000).into_iter()
                .map(std::iter::Iterator::collect)
                .collect();

            let webhook = if tracing::Level::ERROR >= *severity {
                &self.error_logs
            } else {
                &self.normal_logs
            };

            for chunk in message_chunked {
                webhook.execute(&self.http, false, |b| {b
                    .content(&chunk)
                    .username(&username)
                    .avatar_url(&self.level_lookup.get(severity).unwrap_or(&String::from(
                        "https://cdn.discordapp.com/embed/avatars/5.png",
                    )))
                }).await?;
            }
        }
        Ok(())
    }
}
pub struct StringVisitor<'a> {
    string: &'a mut String,
}
pub struct WebhookLogSend {
    sender: Mutex<Option<Sender<(tracing::Level, LogMessage)>>>,
    max_verbosity: tracing::Level,
}

impl<'a> tracing::field::Visit for StringVisitor<'a> {
    fn record_debug(&mut self, _field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        write!(self.string, "{:?}", value).unwrap();
    }

    fn record_str(&mut self, _field: &tracing::field::Field, value: &str) {
        self.string.push_str(value);
    }
}

impl WebhookLogSend {
    pub fn new(sender: Sender<(tracing::Level, LogMessage)>, max_verbosity: tracing::Level) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
            max_verbosity,
        }
    }
}

impl tracing::Subscriber for WebhookLogSend {
    // Hopefully this works
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = String::new();
        event.record(&mut StringVisitor {string: &mut message});
        
        let mut sender_lock = self.sender.lock();
        match &*sender_lock {
            Some(sender) => {
                let metadata = event.metadata();
                if sender.send((*metadata.level(), [String::from(metadata.target()), message])).is_err() {
                    eprintln!("Logging channel hung up, assuming shutdown.");
                    sender_lock.take();
                }
            }
            None => {
                eprintln!("{} log during shutdown: {}", event.metadata().level(), message);
            }
        }
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        // Ordered by verbosity
        if metadata.target().starts_with("discord_tts_bot") {
            self.max_verbosity >= *metadata.level()
        } else {
            tracing::Level::ERROR >= *metadata.level()
        }
    }
}
