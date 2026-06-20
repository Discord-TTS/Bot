use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};

use poise::serenity_prelude as serenity;
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
use tokio::{net::TcpStream, sync::Mutex as TMutex};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message as RawWSMessage};

use crate::{
    structs::Data,
    voice::models::{WSConnectionInfo, WSMessageFrame},
};
pub use models::{GetTTS, WSMessage};

mod models;

#[derive(Clone, Copy)]
pub struct LastXsaidInfo {
    last_announced: Option<serenity::UserId>,
    announce_time: std::time::Instant,
}

impl Default for LastXsaidInfo {
    fn default() -> Self {
        Self {
            last_announced: None,
            announce_time: std::time::Instant::now(),
        }
    }
}

pub type RawWSStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type LockedWSStream = TMutex<RawWSStream>;

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
    /// Only occurs if channel id passed is None.
    CannotJoin,
}

#[derive(Clone)]
pub struct VCContext {
    pub serenity: serenity::Context,
    pub bot_id: serenity::UserId,
    pub guild_id: serenity::GuildId,
    pub channel_id: Option<Arc<AtomicU64>>,
}

impl VCContext {
    fn raw_channel_id(&self) -> &Arc<AtomicU64> {
        self.channel_id.as_ref().expect(
            "should be always set as StartConnectionResult::CannotJoin would be thrown otherwise",
        )
    }

    fn load_channel_id(&self) -> serenity::ChannelId {
        serenity::ChannelId::new(self.raw_channel_id().load(SeqCst))
    }

    fn store_channel_id(&self, new_id: serenity::ChannelId) {
        self.raw_channel_id().store(new_id.get(), SeqCst);
    }
}

pub async fn start_connection(data: &Data, ctx: VCContext) -> StartConnectionResult {
    let (tx, mut rx) = match data.voice_connections.lock().entry(ctx.guild_id) {
        std::collections::hash_map::Entry::Occupied(entry) => {
            return StartConnectionResult::AlreadyIn(entry.get().clone());
        }
        std::collections::hash_map::Entry::Vacant(vacant_entry) => {
            let Some(channel_id) = &ctx.channel_id else {
                return StartConnectionResult::CannotJoin;
            };

            let (tx, rx) = futures::channel::mpsc::unbounded();
            vacant_entry.insert((tx.clone(), Arc::clone(channel_id), LastXsaidInfo::default()));

            (tx, rx)
        }
    };

    let (connect_tx, connect_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let data = ctx.serenity.data::<Data>();
        let guild_id = ctx.guild_id;

        // If the leave is user-requested, a oneshot channel may be returned to notify calling code.
        //
        // It is important that `rx` is dropped AFTER the leave notifier is triggered, the `rx` drop
        // will any pending leave notifiers and therefore trigger them.
        let ws_tx = &data.ws_connections[data.select_tts_index(guild_id)];
        let leave_notifier = ws_task(ctx, ws_tx, &mut rx, connect_tx).await;

        data.voice_connections.lock().remove(&guild_id);
        if let Some(leave_notifier) = leave_notifier {
            leave_notifier.send(()).ok();
        }
    });

    match connect_rx.await {
        Ok(()) => StartConnectionResult::Started(tx),
        Err(futures::channel::oneshot::Canceled) => StartConnectionResult::TimedOut,
    }
}

#[derive(Clone, Copy)]
pub enum LeaveVCResult {
    Left,
    Mismatch,
    Missing,
}

pub async fn leave_vc(
    data: &Data,
    guild_id: serenity::GuildId,
    requested_channel_id: Option<serenity::ChannelId>,
) -> LeaveVCResult {
    let interconnect = match data.voice_connections.lock().get(&guild_id) {
        Some((tx, channel_id, _))
            if requested_channel_id
                .is_none_or(|requested| requested.get() == channel_id.load(SeqCst)) =>
        {
            tx.clone()
        }
        Some(_) => return LeaveVCResult::Mismatch,
        None => return LeaveVCResult::Missing,
    };

    end_connection(interconnect).await;
    LeaveVCResult::Left
}

pub async fn end_connection(interconnect: UnboundedSender<InterconnectMessage>) {
    let (tx, rx) = oneshot::channel();
    if interconnect
        .unbounded_send(InterconnectMessage::Leave(tx))
        .is_ok()
    {
        rx.await.ok();
    }
}

#[derive(Clone, Copy)]
pub struct MissingInterconnectError;

pub fn clear_queue(
    data: &Data,
    guild_id: serenity::GuildId,
) -> Result<(), MissingInterconnectError> {
    if let Some((tx, _, _)) = data.voice_connections.lock().get(&guild_id)
        && tx.unbounded_send(InterconnectMessage::ClearQueue).is_ok()
    {
        Ok(())
    } else {
        Err(MissingInterconnectError)
    }
}

pub fn should_announce_name(
    data: &Data,
    guild_id: serenity::GuildId,
    author_id: serenity::UserId,
) -> bool {
    let mut voice_connections = data.voice_connections.lock();
    let Some((_, _, xsaid)) = voice_connections.get_mut(&guild_id) else {
        return true;
    };

    if xsaid.last_announced == Some(author_id) && xsaid.announce_time.elapsed().as_secs() < 60 {
        false
    } else {
        xsaid.announce_time = std::time::Instant::now();
        xsaid.last_announced = Some(author_id);
        true
    }
}

#[derive(Clone, Copy, Debug)]
#[expect(dead_code, reason = "Only used for debug printing")]
pub struct VoiceDebug {
    is_open: bool,
    channel_id: serenity::ChannelId,
}

pub fn debug_info(data: &Data, guild_id: serenity::GuildId) -> Option<VoiceDebug> {
    data.voice_connections
        .lock()
        .get(&guild_id)
        .map(|(tx, id, _)| VoiceDebug {
            is_open: !tx.is_closed(),
            channel_id: serenity::ChannelId::new(id.load(SeqCst)),
        })
}

#[derive(Debug)]
pub enum InterconnectMessage {
    QueueTTS(models::GetTTS),
    Leave(oneshot::Sender<()>),
    ClearQueue,
}

async fn ws_task(
    ctx: VCContext,
    ws_tx: &LockedWSStream,
    interconnect: &mut UnboundedReceiver<InterconnectMessage>,
    connect_tx: oneshot::Sender<()>,
) -> Option<oneshot::Sender<()>> {
    let guild_id = ctx.guild_id;
    let end_vc_connection = || {
        send_gateway_message(&ctx, || serenity::ShardRunnerMessage::UpdateVoiceState {
            guild_id,
            channel_id: None,
            self_mute: false,
            self_deaf: false,
        })
    };

    let send_ws_msg = async |inner: WSMessage<'_>| {
        let msg_framed = WSMessageFrame { guild_id, inner };
        let serialized = serde_json::to_string(&msg_framed).unwrap();

        let msg = RawWSMessage::Text(serialized.into());
        ws_tx.lock().await.send(msg).await
    };

    let ctx_clone = ctx.clone();
    let mut collector = create_vc_collector(&ctx_clone);
    let mut connection_info = join_voice_channel(&ctx, &mut collector).await?;
    if send_ws_msg(WSMessage::MoveVC(&connection_info))
        .await
        .is_err()
    {
        end_vc_connection().await;

        tracing::error!("Failed to send initial MoveVC message to tts-service");
        return None;
    }

    // We don't care if the /join has hung up.
    _ = connect_tx.send(());

    let mut leave_notifier = None::<oneshot::Sender<()>>;
    loop {
        tokio::select!(
            vc_event = collector.next() => {
                if let Some(vc_event) = vc_event {
                    match apply_event_to_info(&mut connection_info, vc_event) {
                        ApplyEventResult::LeaveVC => break,
                        ApplyEventResult::Applied => {
                            ctx.store_channel_id(connection_info.channel_id);
                            if send_ws_msg(WSMessage::MoveVC(&connection_info)).await.is_err() {
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
                        if send_ws_msg(WSMessage::QueueTTS(request)).await.is_err() {
                            tracing::error!("Failed to send queue message to tts-service");
                            break;
                        }
                    },
                    Some(InterconnectMessage::ClearQueue) => {
                        if send_ws_msg(WSMessage::ClearQueue).await.is_err() {
                            tracing::error!("Failed to send clear queue message to tts-service");
                            break;
                        }
                    },
                    Some(InterconnectMessage::Leave(notifier)) => {
                        leave_notifier = Some(notifier);
                        break;
                    },
                    None => {
                        break;
                    },
                }
            }
        );
    }

    end_vc_connection().await;

    send_ws_msg(WSMessage::Leave).await.ok();
    leave_notifier
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
    Lonely,
    ChannelDeleted,
    State(StateEvent),
    Server(ServerEvent),
}

fn create_vc_collector(ctx: &VCContext) -> impl futures::Stream<Item = VCEvent> {
    let target_channel = Arc::clone(ctx.raw_channel_id());
    let cache = ctx.serenity.cache.clone();
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
        }) if event.guild_id == Some(guild_id) => {
            if event.user_id == bot_id {
                Some(VCEvent::State(StateEvent {
                    session_id: event.session_id.clone(),
                    channel_id: event.channel_id,
                }))
            } else if let Some(guild) = cache.guild(guild_id)
                && let Some(old_state) = guild.voice_states.get(&event.user_id)
                && old_state.channel_id.is_some() & event.channel_id.is_none()
            {
                let target_channel = target_channel.load(SeqCst);
                let mut channel_voice_states = guild.voice_states.iter().filter(|vs| {
                    vs.channel_id
                        .is_some_and(|vs_channel| vs_channel == target_channel)
                });

                let any_non_leaving_non_bot_member = channel_voice_states.any(|voice_state| {
                    if voice_state.user_id == event.user_id {
                        return false;
                    }

                    if let Some(member) = guild.members.get(&voice_state.user_id) {
                        !member.user.bot()
                    } else {
                        false
                    }
                });

                if any_non_leaving_non_bot_member {
                    None
                } else {
                    Some(VCEvent::Lonely)
                }
            } else {
                None
            }
        }
        serenity::Event::ChannelDelete(event)
            if event.channel.base.guild_id == guild_id
                && event.channel.id == target_channel.load(SeqCst) =>
        {
            Some(VCEvent::ChannelDeleted)
        }
        _ => None,
    })
}

async fn send_gateway_message(ctx: &VCContext, msg: impl Fn() -> serenity::ShardRunnerMessage) {
    let shard_id = ctx.serenity.shard_id;
    let runners = ctx.serenity.data_ref::<Data>().runners.get().unwrap();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        if let Some(shard_runner) = runners.get_mut(&shard_id) {
            if shard_runner.tx.unbounded_send(msg()).is_ok() {
                break;
            }

            tracing::warn!("Failed to send message to shard ${shard_id}, retrying");
        } else {
            tracing::warn!("Failed to get shard tx for shard ${shard_id}, retrying");
        }
    }
}

async fn join_voice_channel(
    ctx: &VCContext,
    collector: &mut (impl futures::Stream<Item = VCEvent> + Unpin),
) -> Option<WSConnectionInfo> {
    // Trigger the voice state update, which should trigger the events we are listening for
    send_gateway_message(ctx, || serenity::ShardRunnerMessage::UpdateVoiceState {
        guild_id: ctx.guild_id,
        channel_id: Some(ctx.load_channel_id()),
        self_mute: false,
        self_deaf: false,
    })
    .await;

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
            VCEvent::Lonely => {
                tracing::warn!("Bot became lonely while joining vc");
                return None;
            }
            VCEvent::ChannelDeleted => {
                tracing::warn!("Voice channel was deleted while joining");
                return None;
            }
            VCEvent::State(event) => {
                if let Some(new_channel_id) = event.channel_id {
                    ctx.store_channel_id(new_channel_id);
                    state = Some(event);
                }
            }
        }

        if let Some(state) = &mut state
            && let Some(server) = &mut server
        {
            return Some(WSConnectionInfo {
                bot_id: ctx.bot_id,
                guild_id: ctx.guild_id,
                channel_id: ctx.load_channel_id(),
                session_id: std::mem::take(&mut state.session_id),
                endpoint: std::mem::take(&mut server.endpoint),
                token: std::mem::take(&mut server.token),
            });
        }
    }

    tracing::warn!("Failed to recieve connection info for {}", ctx.guild_id);
    None
}

#[derive(Clone, Copy)]
enum ApplyEventResult {
    Applied,
    LeaveVC,
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
        VCEvent::Lonely | VCEvent::ChannelDeleted => ApplyEventResult::LeaveVC,
    }
}
