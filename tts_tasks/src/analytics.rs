use std::sync::Arc;

use sqlx::Connection;

use tts_core::analytics;

impl crate::Looper for Arc<analytics::Handler> {
    const NAME: &'static str = "Analytics";
    const MILLIS: u64 = 5000;

    type Error = anyhow::Error;
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
                        .bind(kind == analytics::EventType::Command)
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
