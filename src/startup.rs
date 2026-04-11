use std::{collections::BTreeMap, sync::Arc, time::Duration};

use rand::Rng;
use small_fixed_array::{FixedArray, FixedString, TruncatingInto as _};

use poise::serenity_prelude::{
    self as serenity,
    futures::{self, SinkExt, StreamExt, stream::FusedStream},
};

use tokio_tungstenite::tungstenite::Message;
use tts_core::{
    opt_ext::OptionTryUnwrap as _,
    structs::{
        Data, GoogleGender, GoogleVoice, MainConfig, Result, TTSMode, WebhookConfig,
        WebhookConfigRaw,
    },
    voice,
};

pub async fn get_webhooks(
    http: &serenity::Http,
    webhooks_raw: WebhookConfigRaw,
) -> Result<WebhookConfig> {
    let get_webhook = |url: reqwest::Url| async move {
        let (webhook_id, _) = serenity::parse_webhook(&url).try_unwrap()?;
        anyhow::Ok(webhook_id.to_webhook(http).await?)
    };

    let (logs, errors) = tokio::try_join!(
        get_webhook(webhooks_raw.logs),
        get_webhook(webhooks_raw.errors),
    )?;

    println!("Fetched webhooks");
    Ok(WebhookConfig { logs, errors })
}

async fn fetch_json<T>(reqwest: &reqwest::Client, url: reqwest::Url) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let resp = reqwest.get(url).send().await?;
    Ok(resp.error_for_status()?.json().await?)
}

pub async fn fetch_voices<T: serde::de::DeserializeOwned>(
    reqwest: &reqwest::Client,
    mut tts_service: reqwest::Url,
    mode: TTSMode,
) -> Result<T> {
    tts_service.set_path("voices");
    tts_service
        .query_pairs_mut()
        .append_pair("mode", mode.into())
        .append_pair("raw", "true")
        .finish();

    let res = fetch_json(reqwest, tts_service).await?;

    println!("Loaded voices for TTS Mode: {mode}");
    Ok(res)
}

pub async fn fetch_translation_languages(
    reqwest: &reqwest::Client,
    mut tts_service: reqwest::Url,
) -> Result<BTreeMap<FixedString<u8>, FixedString<u8>>> {
    tts_service.set_path("translation_languages");

    let raw_langs: Vec<(String, FixedString<u8>)> = fetch_json(reqwest, tts_service).await?;
    let lang_map = raw_langs.into_iter().map(|(mut lang, name)| {
        lang.make_ascii_lowercase();
        (lang.trunc_into(), name)
    });

    println!("Loaded DeepL translation languages");
    Ok(lang_map.collect())
}

pub fn prepare_gcloud_voices(
    raw_map: Vec<GoogleVoice>,
) -> BTreeMap<FixedString<u8>, BTreeMap<FixedString<u8>, GoogleGender>> {
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

async fn connect_ws_stream(mut url: reqwest::Url) -> Result<voice::RawWSStream> {
    url.set_path("stream");
    url.set_scheme("ws").unwrap();

    Ok(tokio_tungstenite::connect_async(url).await?.0)
}

pub async fn setup_ws_stream(config: &MainConfig) -> Result<FixedArray<voice::LockedWSStream, u8>> {
    let tasks = config.tts_services.iter().map(async |url| {
        let stream = connect_ws_stream(url.clone()).await?;
        anyhow::Ok(voice::LockedWSStream::new(stream))
    });

    let streams = futures::future::try_join_all(tasks).await?;
    println!("Connected to {} tts-services", config.tts_services.len());
    Ok(streams.trunc_into())
}

async fn reconnect_ws_stream(url: &reqwest::Url, index: u8) -> voice::RawWSStream {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        if let Ok(stream) = connect_ws_stream(url.clone()).await {
            tracing::warn!("Reconnected to tts-service-{index}");
            break stream;
        }

        tracing::error!("Failed to reconnect tts-service-{index}");
        interval.tick().await;
    }
}

async fn check_ws_healthy(rng: &mut rand::rngs::SmallRng, ws_tx: &mut voice::RawWSStream) -> bool {
    let mut expected_pong = [0_u8; 64];
    rng.fill_bytes(&mut expected_pong);
    let ping = bytes::Bytes::copy_from_slice(&expected_pong);

    if ws_tx.is_terminated() {
        return false;
    }

    if ws_tx.send(Message::Ping(ping.clone())).await.is_err() {
        return false;
    }

    match tokio::time::timeout(Duration::from_secs(1), ws_tx.next()).await {
        Ok(Some(Ok(Message::Pong(pong)))) => ping == pong,
        _ => false,
    }
}

pub fn start_ws_health_checks(data: &Arc<Data>) {
    let health_check = async |data: Arc<Data>, index| {
        let ws_tx_locked = &data.ws_connections[index];
        let mut rng: rand::rngs::SmallRng = rand::make_rng();
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;

            let mut ws_tx = ws_tx_locked.lock().await;
            if check_ws_healthy(&mut rng, &mut ws_tx).await {
                tracing::debug!("Health check passed for tts-service-{index}");
            } else {
                let url = data.config.tts_services[index].clone();
                *ws_tx = reconnect_ws_stream(&url, index).await;
            }
        }
    };

    for index in 0..data.ws_connections.len() {
        tokio::spawn(health_check(data.clone(), index));
    }
}
