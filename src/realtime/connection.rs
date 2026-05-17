//! WebSocket connection actor.
//!
//! Owns a single Phoenix-protocol WebSocket and multiplexes many channels
//! over it. Spawned tasks handle inbound routing, outbound writes, periodic
//! heartbeats, and automatic reconnection with exponential backoff.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, info, warn};

use crate::error::{AuthError, Result, SupabaseError};

use super::events::RealtimeEvent;
use super::protocol::{events as ev, IncomingMessage, OutgoingMessage, ReplyPayload, HEARTBEAT_TOPIC};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const CHANNEL_BUFFER: usize = 64;
const OUTBOUND_BUFFER: usize = 256;

/// Tunable reconnect policy for the realtime WebSocket.
#[derive(Debug, Clone, Copy)]
pub struct ReconnectPolicy {
    pub enabled: bool,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_attempts: Option<u32>,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            max_attempts: None,
        }
    }
}

/// Handle to the connection actor. Cheaply cloneable.
#[derive(Clone)]
pub(crate) struct Connection {
    pub(crate) inner: Arc<ConnectionInner>,
}

pub(crate) struct ConnectionInner {
    url: String,
    outbound: mpsc::Sender<String>,
    channels: Mutex<HashMap<String, ChannelEntry>>,
    pending: Mutex<HashMap<String, oneshot::Sender<ReplyResult>>>,
    ref_counter: AtomicU64,
    access_token: RwLock<Option<String>>,
    reconnect: ReconnectPolicy,
}

/// Per-topic state. Keeps the user-facing event sender plus the original
/// join payload so reconnects can re-subscribe transparently.
pub(crate) struct ChannelEntry {
    pub tx: mpsc::Sender<RealtimeEvent>,
    pub join_payload: Value,
}

pub(crate) type ReplyResult = std::result::Result<ReplyPayload, String>;

impl Connection {
    pub async fn connect(url: &str, reconnect: ReconnectPolicy) -> Result<Self> {
        let (outbound_tx, outbound_rx) = mpsc::channel::<String>(OUTBOUND_BUFFER);
        let inner = Arc::new(ConnectionInner {
            url: url.to_string(),
            outbound: outbound_tx.clone(),
            channels: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            ref_counter: AtomicU64::new(1),
            access_token: RwLock::new(None),
            reconnect,
        });

        // First connection attempt must succeed so we can surface a sensible
        // error to the caller. Subsequent reconnects happen in the supervisor.
        let ws = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| {
                SupabaseError::Auth(AuthError::from_message(format!(
                    "Realtime connection failed: {e}"
                )))
            })?
            .0;

        spawn_supervisor(Arc::clone(&inner), ws, outbound_rx);
        spawn_heartbeat(Arc::clone(&inner));

        Ok(Self { inner })
    }

    /// Send a message and await the phx_reply payload.
    pub async fn request(
        &self,
        topic: &str,
        event: &str,
        payload: Value,
    ) -> Result<ReplyPayload> {
        let msg_ref = next_ref(&self.inner);
        let (tx, rx) = oneshot::channel::<ReplyResult>();
        self.inner.pending.lock().await.insert(msg_ref.clone(), tx);

        let frame = OutgoingMessage {
            topic,
            event,
            payload,
            message_ref: msg_ref,
        };
        let text = serde_json::to_string(&frame)?;
        self.inner
            .outbound
            .send(text)
            .await
            .map_err(|_| SupabaseError::Unexpected("Realtime writer task is gone".into()))?;

        match rx.await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(msg)) => Err(SupabaseError::Auth(AuthError::from_message(msg))),
            Err(_) => Err(SupabaseError::Unexpected(
                "Realtime reply channel was dropped".into(),
            )),
        }
    }

    /// Send a message without expecting a reply.
    pub async fn send(&self, topic: &str, event: &str, payload: Value) -> Result<()> {
        let msg_ref = next_ref(&self.inner);
        let frame = OutgoingMessage {
            topic,
            event,
            payload,
            message_ref: msg_ref,
        };
        let text = serde_json::to_string(&frame)?;
        self.inner
            .outbound
            .send(text)
            .await
            .map_err(|_| SupabaseError::Unexpected("Realtime writer task is gone".into()))?;
        Ok(())
    }

    /// Register a channel handle so inbound messages for `topic` route to its mpsc.
    /// `join_payload` is retained so the channel can be re-joined on reconnect.
    pub async fn register_channel(
        &self,
        topic: String,
        join_payload: Value,
    ) -> mpsc::Receiver<RealtimeEvent> {
        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);
        self.inner
            .channels
            .lock()
            .await
            .insert(topic, ChannelEntry { tx, join_payload });
        rx
    }

    pub async fn unregister_channel(&self, topic: &str) {
        self.inner.channels.lock().await.remove(topic);
    }

    /// Update the cached access token and push an `access_token` event on
    /// every joined channel. Mirrors `supabase-js`'s `realtime.setAuth`.
    pub async fn set_auth(&self, token: Option<String>) -> Result<()> {
        *self.inner.access_token.write().await = token.clone();
        if let Some(ref tk) = token {
            let topics: Vec<String> = {
                let chans = self.inner.channels.lock().await;
                chans.keys().cloned().collect()
            };
            let payload = json!({ "access_token": tk });
            for topic in topics {
                if let Err(e) = self.send(&topic, ev::ACCESS_TOKEN, payload.clone()).await {
                    debug!(target: "supabase::realtime", topic = %topic, error = %e, "set_auth send failed");
                }
            }
        }
        Ok(())
    }

    /// Current cached access token, if any.
    pub async fn access_token(&self) -> Option<String> {
        self.inner.access_token.read().await.clone()
    }
}

fn next_ref(inner: &ConnectionInner) -> String {
    inner.ref_counter.fetch_add(1, Ordering::Relaxed).to_string()
}

/// Spawn the supervisor that owns the live WebSocket. On disconnect it
/// reconnects (with exponential backoff) and replays all known channel
/// joins so subscribers keep receiving events transparently.
fn spawn_supervisor(
    inner: Arc<ConnectionInner>,
    initial: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut outbound_rx: mpsc::Receiver<String>,
) {
    tokio::spawn(async move {
        let mut ws = Some(initial);
        let mut attempt: u32 = 0;

        loop {
            let stream = match ws.take() {
                Some(s) => s,
                None => match connect_with_backoff(&inner, &mut attempt).await {
                    Some(s) => s,
                    None => {
                        warn!(
                            target: "supabase::realtime",
                            "giving up on reconnect — closing channels"
                        );
                        drain_channels(&inner).await;
                        return;
                    }
                },
            };
            // Connected (or just-reconnected). Reset backoff and replay joins.
            attempt = 0;
            if let Err(e) = replay_joins(&inner).await {
                warn!(target: "supabase::realtime", error = %e, "failed to queue replay joins");
            }

            let (mut sink, mut read_stream) = stream.split();

            // Run a single read+write loop until either side breaks. Use
            // tokio::select! to multiplex outbound and inbound traffic.
            loop {
                tokio::select! {
                    biased;

                    // Outbound: forward queued frames to the sink.
                    maybe_text = outbound_rx.recv() => {
                        match maybe_text {
                            Some(text) => {
                                if let Err(e) = sink.send(WsMessage::Text(text.into())).await {
                                    warn!(target: "supabase::realtime", error = %e, "send failed; reconnecting");
                                    break;
                                }
                            }
                            None => {
                                // Outbound channel dropped — shutdown.
                                let _ = sink.close().await;
                                drain_channels(&inner).await;
                                return;
                            }
                        }
                    }

                    // Inbound: parse and route incoming frames.
                    frame = read_stream.next() => {
                        match frame {
                            Some(Ok(WsMessage::Text(text))) => {
                                handle_inbound_text(&inner, &text).await;
                            }
                            Some(Ok(WsMessage::Close(_))) => {
                                debug!(target: "supabase::realtime", "server closed connection");
                                break;
                            }
                            Some(Ok(_)) => { /* ignore ping/pong/binary */ }
                            Some(Err(e)) => {
                                warn!(target: "supabase::realtime", error = %e, "recv failed; reconnecting");
                                break;
                            }
                            None => {
                                debug!(target: "supabase::realtime", "stream ended");
                                break;
                            }
                        }
                    }
                }
            }

            if let Ok(mut reunited) = sink.reunite(read_stream) {
                let _ = reunited.close(None).await;
            }

            if !inner.reconnect.enabled {
                drain_channels(&inner).await;
                return;
            }
            // Loop falls back to connect_with_backoff on the next iteration.
        }
    });
}

async fn connect_with_backoff(
    inner: &Arc<ConnectionInner>,
    attempt: &mut u32,
) -> Option<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let policy = inner.reconnect;
    loop {
        if let Some(max) = policy.max_attempts {
            if *attempt >= max {
                return None;
            }
        }
        let delay = backoff(policy, *attempt);
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }
        *attempt = attempt.saturating_add(1);

        match tokio_tungstenite::connect_async(&inner.url).await {
            Ok((s, _)) => {
                info!(target: "supabase::realtime", attempt = *attempt, "reconnected");
                return Some(s);
            }
            Err(e) => {
                warn!(
                    target: "supabase::realtime",
                    attempt = *attempt,
                    error = %e,
                    "reconnect failed"
                );
                continue;
            }
        }
    }
}

fn backoff(policy: ReconnectPolicy, attempt: u32) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let exp = attempt.min(16);
    let nanos = policy
        .initial_backoff
        .as_nanos()
        .saturating_mul(1u128 << (exp - 1).min(15));
    let capped = nanos.min(policy.max_backoff.as_nanos());
    Duration::from_nanos(capped.min(u64::MAX as u128) as u64)
}

async fn replay_joins(inner: &Arc<ConnectionInner>) -> Result<()> {
    // Snapshot to avoid holding the lock while awaiting sends.
    let snapshot: Vec<(String, Value)> = {
        let chans = inner.channels.lock().await;
        chans
            .iter()
            .map(|(t, e)| (t.clone(), e.join_payload.clone()))
            .collect()
    };
    for (topic, payload) in snapshot {
        let msg_ref = inner.ref_counter.fetch_add(1, Ordering::Relaxed).to_string();
        let frame = OutgoingMessage {
            topic: &topic,
            event: ev::JOIN,
            payload,
            message_ref: msg_ref,
        };
        let text = serde_json::to_string(&frame)?;
        // Best-effort: if outbound is full or closed, the supervisor will retry on next reconnect.
        let _ = inner.outbound.send(text).await;
    }
    Ok(())
}

async fn drain_channels(inner: &Arc<ConnectionInner>) {
    let mut chans = inner.channels.lock().await;
    for (_, entry) in chans.drain() {
        let _ = entry.tx.send(RealtimeEvent::Closed).await;
    }
}

async fn handle_inbound_text(inner: &Arc<ConnectionInner>, text: &str) {
    let parsed: Result<IncomingMessage> =
        serde_json::from_str(text).map_err(|e| SupabaseError::Decode {
            message: e.to_string(),
            body: text.to_string(),
        });
    match parsed {
        Ok(msg) => dispatch_inbound(inner, msg).await,
        Err(e) => debug!(target: "supabase::realtime", error = %e, "decode failed"),
    }
}

fn spawn_heartbeat(inner: Arc<ConnectionInner>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        interval.tick().await; // first tick is immediate; skip
        loop {
            interval.tick().await;
            if inner.outbound.is_closed() {
                break;
            }
            let msg_ref = next_ref(&inner);
            let frame = OutgoingMessage {
                topic: HEARTBEAT_TOPIC,
                event: ev::HEARTBEAT,
                payload: serde_json::json!({}),
                message_ref: msg_ref,
            };
            if let Ok(text) = serde_json::to_string(&frame) {
                let _ = inner.outbound.send(text).await;
            }
        }
    });
}

async fn dispatch_inbound(state: &Arc<ConnectionInner>, msg: IncomingMessage) {
    // Replies to phx_join / phx_leave / heartbeat / access_token: deliver to the oneshot waiter.
    if msg.event == ev::REPLY {
        if let Some(ref_id) = msg.message_ref.clone() {
            let mut pending = state.pending.lock().await;
            if let Some(tx) = pending.remove(&ref_id) {
                let parsed: Result<ReplyPayload> = serde_json::from_value(msg.payload.clone())
                    .map_err(|e| SupabaseError::Decode {
                        message: e.to_string(),
                        body: msg.payload.to_string(),
                    });
                let _ = match parsed {
                    Ok(reply) => tx.send(Ok(reply)),
                    Err(e) => tx.send(Err(e.to_string())),
                };
                return;
            }
        }
    }

    // Channel-scoped events go to the channel's mpsc.
    let topic = msg.topic.clone();
    let chans = state.channels.lock().await;
    let Some(entry) = chans.get(&topic) else {
        return;
    };
    let sender = entry.tx.clone();
    drop(chans);

    let event = match msg.event.as_str() {
        ev::POSTGRES_CHANGES => decode_postgres(msg.payload),
        ev::BROADCAST => decode_broadcast(msg.payload),
        ev::PRESENCE_STATE => Some(RealtimeEvent::PresenceSync(msg.payload)),
        ev::PRESENCE_DIFF => Some(RealtimeEvent::PresenceDiff(msg.payload)),
        ev::SYSTEM => Some(RealtimeEvent::System(msg.payload)),
        ev::ERROR => Some(RealtimeEvent::Error(
            msg.payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("realtime error")
                .to_string(),
        )),
        ev::CLOSE => Some(RealtimeEvent::Closed),
        _ => None,
    };

    if let Some(evt) = event {
        let _ = sender.send(evt).await;
    }
}

fn decode_postgres(payload: Value) -> Option<RealtimeEvent> {
    // Realtime v2 wraps data in a `data` field.
    let inner = payload.get("data").cloned().unwrap_or(payload);
    match serde_json::from_value::<super::events::PostgresChangePayload>(inner) {
        Ok(pc) => Some(RealtimeEvent::PostgresChange(pc)),
        Err(_) => None,
    }
}

fn decode_broadcast(payload: Value) -> Option<RealtimeEvent> {
    match serde_json::from_value::<super::events::BroadcastPayload>(payload) {
        Ok(b) => Some(RealtimeEvent::Broadcast(b)),
        Err(_) => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn backoff_zero_for_first_attempt() {
        let p = ReconnectPolicy::default();
        assert_eq!(backoff(p, 0), Duration::ZERO);
    }

    #[test]
    fn backoff_grows_then_caps() {
        let p = ReconnectPolicy {
            enabled: true,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            max_attempts: None,
        };
        assert_eq!(backoff(p, 1), Duration::from_millis(100));
        assert_eq!(backoff(p, 2), Duration::from_millis(200));
        assert_eq!(backoff(p, 3), Duration::from_millis(400));
        // Eventually clamps to max_backoff
        assert!(backoff(p, 10) <= Duration::from_secs(5));
        assert_eq!(backoff(p, 20), Duration::from_secs(5));
    }
}
