use std::{sync::Arc, collections::HashMap};

use hmac::{Mac as _, digest::FixedOutput as _};
use reqwest::header::HeaderValue;
use parking_lot::RwLock;
use subtle::ConstantTimeEq as _;
use tracing::error;

use poise::serenity_prelude as serenity;

use crate::{structs::{Result, PatreonConfig}, require};


type Md5Hmac = hmac::Hmac<md5::Md5>;
const BASE_URL: &str = "https://www.patreon.com/api/oauth2/v2";


#[derive(Copy, Clone, Debug)]
pub enum PatreonTier {
    Basic,
    Extra,
}

impl PatreonTier {
    pub const fn entitled_servers(self) -> u8 {
        match self {
            Self::Basic => 2,
            Self::Extra => 5,
        }
    }
}


fn check_md5(key: &[u8], untrusted_signature: &[u8], untrusted_data: &[u8]) -> Result<bool> {
    let mut mac = Md5Hmac::new_from_slice(key)?;
    mac.update(untrusted_data);

    let correct_sig = mac.finalize_fixed();
    Ok(correct_sig.ct_eq(untrusted_signature).into())
}

pub struct PatreonChecker {
    config: PatreonConfig,
    reqwest: reqwest::Client,
    members: RwLock<HashMap<serenity::UserId, PatreonTier>>,
}

impl PatreonChecker {
    pub fn new(reqwest: reqwest::Client, config: PatreonConfig) -> Self {
        Self {
            reqwest, config,
            members: RwLock::new(HashMap::new()),
        }
    }


    pub fn check(&self, patreon_member: serenity::UserId) -> Option<PatreonTier> {
        self.members.read().get(&patreon_member).copied()
    }

    pub async fn background_task(self: Arc<Self>) {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(60*60));

        loop {
            timer.tick().await;
            if let Err(error) = self.fill_members().await {
                error!("Patreon checker: {error:?}");
            }
        }
    }

    pub async fn webhook_recv(&self, 
        headers: axum::http::HeaderMap,
        payload: String,
    ) -> Result<()> {
        if check_md5(
            self.config.webhook_secret.as_bytes(),
            require!(headers.get("X-Patreon-Signature"), Ok(())).as_bytes(),
            payload.as_bytes()
        )? {
            anyhow::bail!("Signature mismatch!");
        };

        let event = require!(headers.get("X-Patreon-Event"), Ok(())).to_str()?;
        if matches!(event, "members:pledge:create" | "members:pledge:delete" | "members:pledge:update" | "members:create") {
            self.fill_members().await // Just refresh all the members
        } else {
            anyhow::bail!("Unknown event: {event}");
        }
    }



    fn get_member_tier(&self, member: &RawPatreonMember, user: &RawPatreonUser) -> Option<(serenity::UserId, Option<PatreonTier>)> {
        user.attributes.social_connections.as_ref().and_then(|socials| socials.discord.as_ref()).map(|discord_info| {
            let check_tier = |tier_id| member.relationships.currently_entitled_tiers.data.iter().any(|tier| tier_id == &tier.id);

            (
                serenity::UserId(discord_info.user_id.parse().unwrap()),
                if check_tier(&self.config.extra_tier_id) {
                    Some(PatreonTier::Extra)
                } else if check_tier(&self.config.basic_tier_id) {
                    Some(PatreonTier::Basic)
                } else {
                    None
                }
            )
        })
    }

    pub async fn fill_members(&self) -> Result<()> {
        let mut url = reqwest::Url::parse(&format!("{BASE_URL}/campaigns/{}/members", self.config.campaign_id))?;
        url.query_pairs_mut()
            .append_pair("fields[user]", "social_connections")
            .append_pair("include", "user,currently_entitled_tiers")
            .finish();

        let mut cursor = String::from("");
        let headers = reqwest::header::HeaderMap::from_iter([
            (reqwest::header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", self.config.creator_access_token))?)
        ]);

        let mut members = HashMap::with_capacity(self.members.read().len());

        loop {
            let mut url = url.clone();
            url.query_pairs_mut().append_pair("page[cursor]", &cursor);

            let resp = self.reqwest
                .get(url).headers(headers.clone())
                .send().await?
                .json::<RawPatreonResponse>().await?;

            members.extend(resp.data.into_iter().filter_map(|member| {
                let user_id = &member.relationships.user.data.id;
                let user = resp.included.iter().find(|u| &u.id == user_id).unwrap();
    
                self.get_member_tier(&member, user).and_then(|(discord_id, tier)| {
                    tier.map(|tier| (discord_id, tier))
                })
            }));

            if let Some(cursors) = resp.meta.pagination.cursors {
                cursor = cursors.next;
            } else {
                members.shrink_to_fit();
                *self.members.write() = members;
                break Ok(())
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct RawPatreonResponse {
    data: Vec<RawPatreonMember>,
    included: Vec<RawPatreonUser>,
    meta: RawPatreonMeta,
}

#[derive(serde::Deserialize)]
struct RawPatreonMember {
    relationships: RawPatreonRelationships,
}

#[derive(serde::Deserialize)]
struct RawPatreonRelationships {
    user: RawPatreonIdData,
    currently_entitled_tiers: RawPatreonTierRelationship,
}

#[derive(serde::Deserialize)]
struct RawPatreonIdData {
    data: RawPatreonId
}

#[derive(serde::Deserialize)]
struct RawPatreonId {
    id: String
}

#[derive(serde::Deserialize)]
struct RawPatreonTierRelationship {
    data: Vec<RawPatreonId>,
}

#[derive(serde::Deserialize)]
struct RawPatreonUser {
    id: String,
    attributes: RawPatreonUserAttributes
}

#[derive(serde::Deserialize)]
struct RawPatreonUserAttributes {
    social_connections: Option<RawPatreonSocialConnections>
}

#[derive(serde::Deserialize)]
struct RawPatreonSocialConnections {
    discord: Option<RawPatreonDiscordConnection>
}

#[derive(serde::Deserialize)]
struct RawPatreonDiscordConnection {
    user_id: String
}

#[derive(serde::Deserialize)]
struct RawPatreonMeta {
    pagination: RawPatreonPagination
}

#[derive(serde::Deserialize)]
struct RawPatreonPagination {
    cursors: Option<RawPatreonCursors>,
}

#[derive(serde::Deserialize)]
struct RawPatreonCursors {
    next: String
}
