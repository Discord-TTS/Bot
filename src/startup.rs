use std::collections::BTreeMap;

use poise::serenity_prelude as serenity;
use small_fixed_array::{FixedString, TruncatingInto as _};

use crate::{
    opt_ext::OptionTryUnwrap as _,
    structs::{GoogleGender, GoogleVoice, Result, WebhookConfig, WebhookConfigRaw},
};

pub async fn get_webhooks(
    http: &serenity::Http,
    webhooks_raw: WebhookConfigRaw,
) -> Result<WebhookConfig> {
    let get_webhook = |url: reqwest::Url| async move {
        let (webhook_id, token) = serenity::parse_webhook(&url).try_unwrap()?;
        anyhow::Ok(http.get_webhook_with_token(webhook_id, token).await?)
    };

    let (logs, errors, dm_logs) = tokio::try_join!(
        get_webhook(webhooks_raw.logs),
        get_webhook(webhooks_raw.errors),
        get_webhook(webhooks_raw.dm_logs),
    )?;

    println!("Fetched webhooks");
    Ok(WebhookConfig {
        logs,
        errors,
        dm_logs,
    })
}

pub async fn get_translation_langs(
    reqwest: &reqwest::Client,
    url: Option<&reqwest::Url>,
    token: Option<&str>,
) -> Result<BTreeMap<FixedString, FixedString>> {
    #[derive(serde::Deserialize)]
    pub struct DeeplVoice {
        pub name: FixedString,
        pub language: String,
    }

    #[derive(serde::Serialize)]
    struct DeeplVoiceRequest {
        #[serde(rename = "type")]
        kind: &'static str,
    }

    let (Some(url), Some(token)) = (url, token) else {
        return Ok(BTreeMap::new());
    };

    let languages: Vec<DeeplVoice> = reqwest
        .get(format!("{url}/languages"))
        .query(&DeeplVoiceRequest { kind: "target" })
        .header("Authorization", format!("DeepL-Auth-Key {token}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let language_map = languages
        .into_iter()
        .map(|v| (v.language.to_lowercase().trunc_into(), v.name))
        .collect();

    println!("Loaded DeepL translation languages");
    Ok(language_map)
}

pub fn prepare_gcloud_voices(
    raw_map: Vec<GoogleVoice>,
) -> BTreeMap<FixedString, BTreeMap<FixedString, GoogleGender>> {
    // {lang_accent: {variant: gender}}
    let mut cleaned_map = BTreeMap::new();
    for gvoice in raw_map {
        let variant = gvoice
            .name
            .splitn(3, '-')
            .nth(2)
            .and_then(|mode_variant| mode_variant.split_once('-'))
            .filter(|(mode, _)| *mode == "Standard")
            .map(|(_, variant)| variant);

        if let Some(variant) = variant {
            let [language] = gvoice.language_codes;
            cleaned_map
                .entry(language)
                .or_insert_with(BTreeMap::new)
                .insert(FixedString::from_str_trunc(variant), gvoice.ssml_gender);
        }
    }

    cleaned_map
}

pub async fn send_startup_message(
    http: &serenity::Http,
    log_webhook: &serenity::Webhook,
) -> Result<serenity::MessageId> {
    let startup_builder = serenity::ExecuteWebhook::default().content("**TTS Bot is starting up**");
    let startup_message = log_webhook.execute(http, true, startup_builder).await?;

    Ok(startup_message.unwrap().id)
}
