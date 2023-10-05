use std::{sync::Arc, collections::{HashMap, HashSet}};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::{
    Result,
    funcs::decode_resp,
    structs::{TTSMode, WebsiteInfo},
    serenity::{self as serenity, json::prelude as json}
};


#[allow(dead_code, clippy::match_same_arms)]
fn remember_to_update_analytics_query() {
    match TTSMode::gTTS {
        TTSMode::gTTS => (),
        TTSMode::Polly => (),
        TTSMode::eSpeak => (),
        TTSMode::gCloud => (),
    }
}

fn count_members<'a>(guilds: impl Iterator<Item=serenity::cache::GuildRef<'a>>) -> u64 {
    guilds.map(|g| g.member_count).sum()
}


#[derive(serde::Serialize)]
struct Statistics {
    premium_guild_count: u32,
    premium_user_count: u64,
    message_count: u64,
    guild_count: u32,
    user_count: u64,
}


pub struct Updater {
    pub patreon_service: Option<reqwest::Url>,
    pub cache: Arc<serenity::Cache>,
    pub reqwest: reqwest::Client,
    pub config: WebsiteInfo,
    pub pool: sqlx::PgPool,
}

#[serenity::async_trait]
impl crate::Looper for Updater {
    const NAME: &'static str = "WebUpdater";
    const MILLIS: u64 = 1000 * 60 * 60;

    async fn loop_func(&self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct AnalyticsQueryResult {
            count: i32
        }

        #[derive(sqlx::FromRow)]
        struct PremiumGuildsQueryResult {
            guild_id: i64
        }

        let patreon_members = if let Some(mut patreon_service) = self.patreon_service.clone() {
            patreon_service.set_path("members");
            let resp = self.reqwest.get(patreon_service).send().await?.error_for_status()?;
            let raw_members: HashMap<i64, serde::de::IgnoredAny> = decode_resp(resp).await?;

            raw_members.into_keys().collect()
        } else {
            Vec::new()
        };

        let (message_count, premium_guild_ids) = {
            let mut db_conn = self.pool.acquire().await?;
            let message_count = sqlx::query_as::<_, AnalyticsQueryResult>("
                SELECT count FROM analytics
                WHERE date_collected = (CURRENT_DATE - 1) AND (
                    event = 'gTTS_tts'   OR
                    event = 'eSpeak_tts' OR
                    event = 'gCloud_tts' OR
                    event = 'Polly_tts'
                )
            ").fetch_all(&mut *db_conn).await?.into_iter().map(|r| r.count as i64).sum::<i64>();

            let premium_guild_ids = sqlx::query_as::<_, PremiumGuildsQueryResult>("SELECT guild_id FROM guilds WHERE premium_user = ANY($1)")
                .bind(&patreon_members).fetch_all(&mut *db_conn).await?
                .into_iter().map(|g| g.guild_id).collect::<HashSet<_>>();

            (message_count, premium_guild_ids)
        };

        let guild_ids = self.cache.guilds();

        let guild_ref_iter = guild_ids.iter().filter_map(|g| self.cache.guild(*g));
        let user_count = count_members(guild_ref_iter.clone());

        let premium_guild_ref_iter = guild_ref_iter.filter(|g| premium_guild_ids.contains(&(g.id.get() as i64)));
        let premium_user_count = count_members(premium_guild_ref_iter.clone());
        let premium_guild_count = premium_guild_ref_iter.count();

        let stats = Statistics {
            user_count, premium_user_count,
            message_count: message_count as u64,
            guild_count: guild_ids.len() as u32,
            premium_guild_count: premium_guild_count as u32,
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
            .error_for_status()?;
        
        Ok(())
    }
}
