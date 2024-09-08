use std::{num::NonZeroU16, sync::Arc};

use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, to_vec};

use self::serenity::UserId;
use serenity::all as serenity;

use tts_core::structs::{BotListTokens, Result};

pub struct BotListUpdater {
    cache: Arc<serenity::cache::Cache>,
    reqwest: reqwest::Client,
    tokens: BotListTokens,
}

struct BotListReq {
    url: String,
    body: Vec<u8>,
    token: HeaderValue,
}

impl BotListUpdater {
    #[must_use]
    pub fn new(
        reqwest: reqwest::Client,
        cache: Arc<serenity::cache::Cache>,
        tokens: BotListTokens,
    ) -> Self {
        Self {
            cache,
            reqwest,
            tokens,
        }
    }

    fn top_gg_data(
        &self,
        bot_id: UserId,
        guild_count: usize,
        shard_count: NonZeroU16,
    ) -> BotListReq {
        BotListReq {
            url: format!("https://top.gg/api/bots/{bot_id}/stats"),
            token: HeaderValue::from_str(self.tokens.top_gg.as_str()).unwrap(),
            body: to_vec(&json!({
                "server_count": guild_count,
                "shard_count": shard_count,
            }))
            .unwrap(),
        }
    }

    fn discord_bots_gg_data(
        &self,
        bot_id: UserId,
        guild_count: usize,
        shard_count: NonZeroU16,
    ) -> BotListReq {
        BotListReq {
            url: format!("https://discord.bots.gg/api/v1/bots/{bot_id}/stats"),
            token: HeaderValue::from_str(self.tokens.discord_bots_gg.as_str()).unwrap(),
            body: to_vec(&json!({
                "guildCount": guild_count,
                "shardCount": shard_count,
            }))
            .unwrap(),
        }
    }

    fn bots_on_discord_data(&self, bot_id: UserId, guild_count: usize) -> BotListReq {
        BotListReq {
            url: format!("https://bots.ondiscord.xyz/bot-api/bots/{bot_id}/guilds"),
            token: HeaderValue::from_str(self.tokens.bots_on_discord.as_str()).unwrap(),
            body: to_vec(&json!({"guildCount": guild_count})).unwrap(),
        }
    }
}

impl crate::Looper for BotListUpdater {
    const NAME: &'static str = "Bot List Updater";
    const MILLIS: u64 = 1000 * 60 * 60;

    async fn loop_func(&self) -> Result<()> {
        let perform = |BotListReq { url, body, token }| async move {
            let headers = reqwest::header::HeaderMap::from_iter([
                (AUTHORIZATION, token),
                (CONTENT_TYPE, HeaderValue::from_static("application/json")),
            ]);

            let request = self.reqwest.post(url).body(body).headers(headers);

            let resp_res = request.send().await;
            if let Err(err) = resp_res.and_then(reqwest::Response::error_for_status) {
                tracing::error!("{} Error: {:?}", Self::NAME, err);
            };
        };

        let shard_count = self.cache.shard_count();
        let bot_id = self.cache.current_user().id;
        let guild_count = self.cache.guild_count();

        tokio::join!(
            perform(self.bots_on_discord_data(bot_id, guild_count)),
            perform(self.top_gg_data(bot_id, guild_count, shard_count)),
            perform(self.discord_bots_gg_data(bot_id, guild_count, shard_count)),
        );

        Ok(())
    }
}
