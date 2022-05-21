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

use std::borrow::Cow;

use poise::async_trait;
use dashmap::DashMap;
use sqlx::Connection;

use crate::structs::Result;

pub struct Handler {
    log_buffer: DashMap<Cow<'static, str>, i32>,
    pool: sqlx::PgPool
}

impl Handler {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            pool, 
            log_buffer: DashMap::new()
        }
    }
}

#[async_trait]
impl crate::traits::Looper for Handler {
    const NAME: &'static str = "Analytics";
    const MILLIS: u64 = 5000;

    async fn loop_func(&self) -> Result<()> {
        let log_buffer = self.log_buffer.clone();
        self.log_buffer.clear();

        let mut conn = self.pool.acquire().await?;
        conn.transaction::<_, _, anyhow::Error>(move |transaction| Box::pin(async {
            for (raw_event, count) in log_buffer {
                let query = sqlx::query("
                    INSERT INTO analytics(event, is_command, count)
                    VALUES($1, $2, $3)
                    ON CONFLICT ON CONSTRAINT analytics_pkey
                    DO UPDATE SET count = analytics.count + EXCLUDED.count
                ;");

                let event = raw_event.strip_prefix("on_").unwrap_or(&raw_event);
                query.bind(event).bind(raw_event == event).bind(count).execute(&mut *transaction).await?;
            }

            Ok(())
        })).await
    }
}

impl Handler {
    pub fn log(&self, event: Cow<'static, str>) {
        let count = (*self.log_buffer.entry(event.clone()).or_insert(0)) + 1;
        self.log_buffer.insert(event, count);
    }
}
