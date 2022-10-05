use std::sync::Arc;

use gnomeutils::serenity::{self as serenity, json::prelude as json};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::{Result, structs::{TTSMode, WebsiteInfo}};


#[allow(dead_code, clippy::match_same_arms)]
fn remember_to_update_analytics_query() {
    match TTSMode::gTTS {
        TTSMode::gTTS => (),
        TTSMode::Polly => (),
        TTSMode::TikTok => (),
        TTSMode::eSpeak => (),
        TTSMode::gCloud => (),
    }
}

#[derive(serde::Serialize)]
struct Statistics {
    user_count: u64,
    guild_count: u32,
    message_count: u64,
}


pub struct Updater {
    pub config: WebsiteInfo,
    pub database: sqlx::PgPool,
    pub reqwest: reqwest::Client,
    pub cache: Arc<serenity::Cache>,
}

#[serenity::async_trait]
impl gnomeutils::Looper for Updater {
    const NAME: &'static str = "WebUpdater";
    const MILLIS: u64 = 1000 * 60;

    async fn loop_func(&self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct QueryResult {
            count: i64
        }

        let message_count = {
            let mut db_conn = self.database.acquire().await?;
            sqlx::query_as::<_, QueryResult>("
                SELECT count FROM analytics
                WHERE date_collected = (CURRENT_DATE - 1) AND (
                    event = 'gTTS_tts'   OR
                    event = 'TikTok_tts' OR
                    event = 'eSpeak_tts' OR
                    event = 'gCloud_tts' OR
                    event = 'Polly_tts'
                )
            ").fetch_all(&mut db_conn).await?.into_iter().map(|r| r.count).sum::<i64>()
        };

        let guilds = self.cache.guilds();
        let guild_count = guilds.len();

        let stats = Statistics {
            user_count: guilds.into_iter().filter_map(|g| self.cache.guild(g)).map(|g| g.member_count).sum(),
            message_count: message_count as u64,
            guild_count: guild_count as u32,
        };

        let url = {
            let mut url = self.config.url.clone();
            url.set_path("/update_stats");
            url
        };

        self.reqwest.post(url)
            .header(AUTHORIZATION, self.config.stats_key.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(json::to_string(&stats)?)
            .send().await?
            .error_for_status()
            .map(drop).map_err(Into::into)
        }
}
