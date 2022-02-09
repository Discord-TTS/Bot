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

use std::sync::Arc;

use dashmap::DashMap;
use tracing::error;

use crate::structs::Error;

pub struct Handler {
    log_buffer: DashMap<String, u32>,
    pool: Arc<deadpool_postgres::Pool>
}

impl Handler {
    pub fn new(pool: Arc<deadpool_postgres::Pool>) -> Self {
        Self {
            pool, 
            log_buffer: DashMap::new()
        }
    }

    pub async fn loop_task(&self) {
        let mut periodic = tokio::time::interval(std::time::Duration::from_millis(5000));

        loop {
            periodic.tick().await;
            if let Err(error) = self.send_info_to_db().await {
                error!("Analytics Handler: {:?}", error);
            }
        }
    }

    async fn send_info_to_db(&self) -> Result<(), Error> {
        let log_buffer = self.log_buffer.clone();
        self.log_buffer.clear();

        let mut conn = self.pool.get().await?;
        let transaction = conn.transaction().await?;

        let query = transaction.prepare_cached("
            INSERT INTO analytics(event, is_command, count)
            VALUES($1, $2, $3)
            ON CONFLICT ON CONSTRAINT analytics_pkey
            DO UPDATE SET count = analytics.count + EXCLUDED.count
        ;").await?;

        for (raw_event, count) in log_buffer {
            let event = raw_event.strip_prefix("on_").unwrap_or(&raw_event);
            transaction.execute(&query, &[&event, &(raw_event == event), &count]).await?;
        }

        transaction.commit().await?;
        Ok(())
    }

    pub fn log(&self, event: String) -> u32 {
        let count = (*self.log_buffer.entry(event.clone()).or_insert(0)) + 1;
        self.log_buffer.insert(event, count);
        count
    }
}
