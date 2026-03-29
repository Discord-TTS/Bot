use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};

use poise::serenity_prelude as serenity;
use reqwest::Url;
use serenity::{
    futures::{
        self, SinkExt, StreamExt,
        channel::{
            mpsc::{UnboundedReceiver, UnboundedSender},
            oneshot,
        },
    },
    small_fixed_array::FixedString,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message as RawWSMessage};

use crate::{
    structs::{Data, LastXsaidInfo},
    voice::models::WSConnectionInfo,
};
pub use models::{GetTTS, WSMessage};

mod models;

// Do not write to AtomicU64 outside of voice task.
pub type ConnectionEntry = (
    UnboundedSender<InterconnectMessage>,
    Arc<AtomicU64>,
    LastXsaidInfo,
);

pub enum StartConnectionResult {
    Started(UnboundedSender<InterconnectMessage>),
    TimedOut,
    AlreadyIn(ConnectionEntry),
}

#[derive(Clone)]
pub struct VCContext {
    pub tts_service: Url,
    pub serenity: serenity::Context,
    pub bot_id: serenity::UserId,
    pub guild_id: serenity::GuildId,
    pub channel_id: Arc<AtomicU64>,
}

pub async fn start_connection(data: &Data, ctx: VCContext) -> StartConnectionResult {
    let (tx, rx) = match data.voice_connections.lock().entry(ctx.guild_id) {
        std::collections::hash_map::Entry::Occupied(entry) => {
            return StartConnectionResult::AlreadyIn(entry.get().clone());
        }
        std::collections::hash_map::Entry::Vacant(vacant_entry) => {
            let (tx, rx) = futures::channel::mpsc::unbounded();
            vacant_entry.insert((
                tx.clone(),
                Arc::clone(&ctx.channel_id),
                LastXsaidInfo::default(),
            ));

            (tx, rx)
        }
    };

    let (connect_tx, connect_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let data = ctx.serenity.data::<Data>();
        let guild_id = ctx.guild_id;
        ws_task(ctx, rx, connect_tx).await;

        data.voice_connections.lock().remove(&guild_id);
    });

    match connect_rx.await {
        Ok(()) => StartConnectionResult::Started(tx),
        Err(futures::channel::oneshot::Canceled) => StartConnectionResult::TimedOut,
    }
}

#[derive(Debug)]
pub enum InterconnectMessage {
    QueueTTS(models::GetTTS),
    ClearQueue,
}

async fn ws_task(
    mut ctx: VCContext,
    mut interconnect: UnboundedReceiver<InterconnectMessage>,
    connect_tx: oneshot::Sender<()>,
) {
    let ctx_clone = ctx.clone();
    let mut collector = create_vc_collector(&ctx_clone);
    let Some(mut connection_info) = join_voice_channel(&mut ctx, &mut collector).await else {
        return;
    };

    let mut stream_url = ctx.tts_service.clone();
    stream_url.set_scheme("ws").unwrap();
    stream_url.set_path("stream");

    let mut stream = match tokio_tungstenite::connect_async(stream_url).await {
        Ok((stream, _)) => stream,
        Err(err) => {
            tracing::error!("Failed to connect to tts-service: {err}");
            return;
        }
    };

    if send_ws_msg(&mut stream, &connection_info).await.is_err() {
        tracing::error!("Failed to send initial message to tts-service");
        return;
    }

    // We don't care if the /join has hung up.
    _ = connect_tx.send(());

    loop {
        tokio::select!(
            vc_event = collector.next() => {
                if let Some(vc_event) = vc_event {
                    match apply_event_to_info(&mut connection_info, vc_event) {
                        ApplyEventResult::NoDifference => {},
                        ApplyEventResult::LeaveVC => break,
                        ApplyEventResult::Applied => {
                            ctx.channel_id.store(connection_info.channel_id.get(), SeqCst);
                            if send_ws_msg(&mut stream, &WSMessage::MoveVC(&connection_info)).await.is_err() {
                                tracing::error!("Failed to send rejoin message to tts-service");
                                break;
                            }
                        },
                    }
                } else {
                    // Replace the collector since it seems to have been dropped?
                    collector = create_vc_collector(&ctx);
                }
            },
            inter_msg = interconnect.next() => {
                match inter_msg {
                    Some(InterconnectMessage::QueueTTS(request)) => {
                        if send_ws_msg(&mut stream, &WSMessage::QueueTTS(request)).await.is_err() {
                            tracing::error!("Failed to send queue message to tts-service");
                            break;
                        }
                    },
                    Some(InterconnectMessage::ClearQueue) => {
                        if send_ws_msg(&mut stream, &WSMessage::ClearQueue).await.is_err() {
                            tracing::error!("Failed to send clear queue message to tts-service");
                            break;
                        }
                    },
                    None => break,
                }
            }
        );
    }

    // Leave VC
    ctx.serenity
        .update_voice_state(ctx.guild_id, None, false, false);
}

struct StateEvent {
    session_id: FixedString,
    channel_id: Option<serenity::ChannelId>,
}

struct ServerEvent {
    token: FixedString,
    endpoint: FixedString,
}

enum VCEvent {
    State(StateEvent),
    Server(ServerEvent),
    ChannelDelete(serenity::ChannelId),
}

fn create_vc_collector(ctx: &VCContext) -> impl futures::Stream<Item = VCEvent> {
    let guild_id = ctx.guild_id;
    let bot_id = ctx.bot_id;
    serenity::collector::collect(&ctx.serenity, move |event| match event {
        serenity::Event::VoiceServerUpdate(event)
            if let Some(endpoint) = &event.endpoint
                && event.guild_id == guild_id =>
        {
            Some(VCEvent::Server(ServerEvent {
                token: event.token.clone(),
                endpoint: endpoint.clone(),
            }))
        }
        serenity::Event::VoiceStateUpdate(serenity::VoiceStateUpdateEvent {
            voice_state: event,
            ..
        }) if event.guild_id == Some(guild_id) && event.user_id == bot_id => {
            Some(VCEvent::State(StateEvent {
                session_id: event.session_id.clone(),
                channel_id: event.channel_id,
            }))
        }
        serenity::Event::ChannelDelete(event) if event.channel.base.guild_id == guild_id => {
            Some(VCEvent::ChannelDelete(event.channel.id))
        }
        _ => None,
    })
}

async fn join_voice_channel(
    ctx: &mut VCContext,
    collector: &mut (impl futures::Stream<Item = VCEvent> + Unpin),
) -> Option<WSConnectionInfo> {
    let mut target_channel = serenity::ChannelId::new(ctx.channel_id.load(SeqCst));

    // Trigger the voice state update, which should trigger the events we are listening for
    ctx.serenity
        .update_voice_state(ctx.guild_id, Some(target_channel), false, false);

    // Setup a timer that will Poll::ready in 30 seconds.
    let mut deadline = std::pin::pin!(tokio::time::sleep(Duration::from_secs(30)));

    // Loop events until both fields are filled in.
    let mut state: Option<StateEvent> = None;
    let mut server: Option<ServerEvent> = None;
    while let Some(event) = tokio::select!(
        () = deadline.as_mut() => return None,
        event = collector.next() => event
    ) {
        match event {
            VCEvent::Server(event) => server = Some(event),
            VCEvent::ChannelDelete(channel_id) => {
                if target_channel == channel_id {
                    // Channel we are joining just got deleted, bail out as timeout will occur.
                    tracing::warn!("Voice channel was deleted while joining");
                    return None;
                }
            }
            VCEvent::State(event) => {
                if let Some(new_channel_id) = event.channel_id {
                    target_channel = new_channel_id;
                    state = Some(event);
                }
            }
        }

        if let Some(state) = &mut state
            && let Some(server) = &mut server
        {
            ctx.channel_id.store(target_channel.get(), SeqCst);
            return Some(WSConnectionInfo {
                bot_id: ctx.bot_id,
                guild_id: ctx.guild_id,
                channel_id: target_channel,
                session_id: std::mem::take(&mut state.session_id),
                endpoint: std::mem::take(&mut server.endpoint),
                token: std::mem::take(&mut server.token),
            });
        }
    }

    tracing::warn!("Failed to recieve connection info for {}", ctx.guild_id);
    None
}

async fn send_ws_msg<T: serde::Serialize>(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    msg: &T,
) -> Result<(), tokio_tungstenite::tungstenite::Error> {
    let serialized = serde_json::to_string(msg).unwrap();
    let msg = RawWSMessage::Text(serialized.into());
    ws.send(msg).await
}

#[derive(Clone, Copy)]
enum ApplyEventResult {
    Applied,
    LeaveVC,
    NoDifference,
}

fn apply_event_to_info(connection_info: &mut WSConnectionInfo, event: VCEvent) -> ApplyEventResult {
    match event {
        VCEvent::State(StateEvent {
            session_id,
            channel_id,
        }) => {
            if let Some(channel_id) = channel_id {
                connection_info.session_id = session_id;
                connection_info.channel_id = channel_id;
                ApplyEventResult::Applied
            } else {
                ApplyEventResult::LeaveVC
            }
        }
        VCEvent::Server(ServerEvent { token, endpoint }) => {
            connection_info.token = token;
            connection_info.endpoint = endpoint;
            ApplyEventResult::Applied
        }
        VCEvent::ChannelDelete(channel_id) => {
            if channel_id == connection_info.channel_id {
                ApplyEventResult::LeaveVC
            } else {
                ApplyEventResult::NoDifference
            }
        }
    }
}
