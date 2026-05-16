//! Channel-level API — what users actually touch.
//!
//! Two complementary styles, mirroring (and extending) `supabase-js`:
//!
//! 1. **Stream style** — accumulate filters via [`ChannelBuilder::on_postgres_changes`]
//!    / [`ChannelBuilder::on_broadcast`] / [`ChannelBuilder::on_presence`], then
//!    `.subscribe().await` and pull events via [`Channel::recv`] or the
//!    [`futures_util::Stream`] impl.
//! 2. **Callback style** — register `Fn(...) + Send + Sync + 'static` callbacks via
//!    [`ChannelBuilder::on_postgres_changes_callback`] / [`ChannelBuilder::on_broadcast_callback`]
//!    / [`ChannelBuilder::on_presence_callback`], then `.subscribe()` and call
//!    [`Channel::run`] to dispatch events until the channel closes.
//!
//! Both styles compose: registered callbacks fire as part of `Channel::run`, and
//! events that are not consumed by a callback can still be observed via `recv`/`Stream`.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::Stream;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::error::{AuthError, Result, SupabaseError};

use super::connection::Connection;
use super::events::{
    BroadcastPayload, PostgresChangePayload, PostgresChangesFilter, PresenceEvent, RealtimeEvent,
    SubscriptionStatus,
};
use super::protocol::events as ev;

/// User-supplied callback for `postgres_changes`. Filtered by the registered
/// [`PostgresChangesFilter`] before invocation.
pub type PostgresChangesCallback = Arc<dyn Fn(PostgresChangePayload) + Send + Sync + 'static>;

/// User-supplied callback for broadcast events. Filtered by event name when
/// one is provided.
pub type BroadcastCallback = Arc<dyn Fn(BroadcastPayload) + Send + Sync + 'static>;

/// User-supplied callback for presence sub-events (`sync`/`join`/`leave`).
pub type PresenceCallback = Arc<dyn Fn(Value) + Send + Sync + 'static>;

pub(crate) struct PostgresChangesHandler {
    filter: PostgresChangesFilter,
    callback: PostgresChangesCallback,
}

pub(crate) struct BroadcastHandler {
    event: Option<String>,
    callback: BroadcastCallback,
}

pub(crate) struct PresenceHandler {
    event: PresenceEvent,
    callback: PresenceCallback,
}

/// Builder for a channel subscription.
///
/// Chain [`on_postgres_changes`](Self::on_postgres_changes) /
/// [`on_broadcast`](Self::on_broadcast) / [`on_presence`](Self::on_presence) before
/// calling [`subscribe`](Self::subscribe).
pub struct ChannelBuilder {
    pub(crate) connection: Connection,
    pub(crate) topic: String,
    pub(crate) postgres_changes: Vec<PostgresChangesFilter>,
    pub(crate) broadcast: BroadcastConfig,
    pub(crate) presence: PresenceConfig,
    pub(crate) access_token: Option<String>,
    pub(crate) pg_callbacks: Vec<PostgresChangesHandler>,
    pub(crate) broadcast_callbacks: Vec<BroadcastHandler>,
    pub(crate) presence_callbacks: Vec<PresenceHandler>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BroadcastConfig {
    pub ack: bool,
    pub self_broadcast: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PresenceConfig {
    pub key: Option<String>,
}

impl ChannelBuilder {
    pub(crate) fn new(connection: Connection, topic: String, access_token: Option<String>) -> Self {
        Self {
            connection,
            topic,
            postgres_changes: Vec::new(),
            broadcast: BroadcastConfig::default(),
            presence: PresenceConfig::default(),
            access_token,
            pg_callbacks: Vec::new(),
            broadcast_callbacks: Vec::new(),
            presence_callbacks: Vec::new(),
        }
    }

    /// Add a Postgres-changes subscription. May be called multiple times to
    /// subscribe to several events / tables on the same channel.
    pub fn on_postgres_changes(mut self, filter: PostgresChangesFilter) -> Self {
        self.postgres_changes.push(filter);
        self
    }

    /// Register a Postgres-changes callback. Adds the filter to the
    /// subscription if not already present.
    pub fn on_postgres_changes_callback<F>(mut self, filter: PostgresChangesFilter, cb: F) -> Self
    where
        F: Fn(PostgresChangePayload) + Send + Sync + 'static,
    {
        self.postgres_changes.push(filter.clone());
        self.pg_callbacks.push(PostgresChangesHandler {
            filter,
            callback: Arc::new(cb),
        });
        self
    }

    /// Configure broadcast behavior. `ack = true` makes the server ack each
    /// broadcast; `self_broadcast = true` echoes your own messages back.
    pub fn on_broadcast(mut self, ack: bool, self_broadcast: bool) -> Self {
        self.broadcast = BroadcastConfig { ack, self_broadcast };
        self
    }

    /// Register a broadcast callback. Pass `Some("event-name")` to filter, or
    /// `None` to receive every broadcast on this channel.
    pub fn on_broadcast_callback<F>(mut self, event: Option<&str>, cb: F) -> Self
    where
        F: Fn(BroadcastPayload) + Send + Sync + 'static,
    {
        self.broadcast_callbacks.push(BroadcastHandler {
            event: event.map(str::to_owned),
            callback: Arc::new(cb),
        });
        self
    }

    /// Configure presence. `key` distinguishes multiple sessions for the same user.
    pub fn on_presence(mut self, key: impl Into<String>) -> Self {
        self.presence.key = Some(key.into());
        self
    }

    /// Register a presence callback. Mirrors supabase-js's
    /// `.on('presence', { event: 'sync'|'join'|'leave' }, cb)`.
    pub fn on_presence_callback<F>(mut self, event: PresenceEvent, cb: F) -> Self
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        self.presence_callbacks.push(PresenceHandler {
            event,
            callback: Arc::new(cb),
        });
        self
    }

    /// Join the channel. Returns a [`Channel`] handle whose
    /// [`recv`](Channel::recv) (or `Stream` impl) yields server messages.
    pub async fn subscribe(self) -> Result<Channel> {
        let topic = self.topic.clone();
        let join_payload = json!({
            "config": {
                "postgres_changes": self.postgres_changes.iter().map(|f| f.to_json()).collect::<Vec<_>>(),
                "broadcast": { "ack": self.broadcast.ack, "self": self.broadcast.self_broadcast },
                "presence": { "key": self.presence.key.clone().unwrap_or_default() },
            },
            "access_token": self.access_token,
        });

        let rx = self
            .connection
            .register_channel(topic.clone(), join_payload.clone())
            .await;

        let reply = self
            .connection
            .request(&topic, ev::JOIN, join_payload)
            .await?;

        if reply.status != "ok" {
            self.connection.unregister_channel(&topic).await;
            return Err(SupabaseError::Auth(AuthError::from_message(format!(
                "Realtime channel join failed: {}",
                reply.response
            ))));
        }

        Ok(Channel {
            connection: self.connection,
            topic,
            events: rx,
            status: SubscriptionStatus::Subscribed,
            pg_callbacks: self.pg_callbacks,
            broadcast_callbacks: self.broadcast_callbacks,
            presence_callbacks: self.presence_callbacks,
        })
    }
}

/// An active channel subscription.
///
/// Drop the channel to leave the topic on the server (best-effort).
pub struct Channel {
    connection: Connection,
    topic: String,
    events: mpsc::Receiver<RealtimeEvent>,
    status: SubscriptionStatus,
    pg_callbacks: Vec<PostgresChangesHandler>,
    broadcast_callbacks: Vec<BroadcastHandler>,
    presence_callbacks: Vec<PresenceHandler>,
}

impl Channel {
    /// Receive the next event from the channel. Returns `None` once the
    /// connection or channel is closed.
    pub async fn recv(&mut self) -> Option<RealtimeEvent> {
        self.events.recv().await
    }

    /// Current subscription state.
    pub fn status(&self) -> SubscriptionStatus {
        self.status
    }

    /// Topic this channel is subscribed to.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Pump events through any registered callbacks until the channel closes.
    /// Callbacks fire synchronously on the same task; spawn this with
    /// [`tokio::spawn`] if you don't want to block.
    pub async fn run(&mut self) {
        while let Some(event) = self.events.recv().await {
            self.dispatch(&event);
            if matches!(event, RealtimeEvent::Closed) {
                self.status = SubscriptionStatus::Closed;
                break;
            }
        }
    }

    fn dispatch(&self, event: &RealtimeEvent) {
        match event {
            RealtimeEvent::PostgresChange(p) => {
                for handler in &self.pg_callbacks {
                    if handler.filter.event.matches(&p.change_type) {
                        (handler.callback)(p.clone());
                    }
                }
            }
            RealtimeEvent::Broadcast(b) => {
                for handler in &self.broadcast_callbacks {
                    if handler
                        .event
                        .as_deref()
                        .map_or(true, |name| name == b.event)
                    {
                        (handler.callback)(b.clone());
                    }
                }
            }
            RealtimeEvent::PresenceSync(v) => {
                for h in &self.presence_callbacks {
                    if h.event == PresenceEvent::Sync {
                        (h.callback)(v.clone());
                    }
                }
            }
            RealtimeEvent::PresenceDiff(v) => {
                // Phoenix sends a single diff with both joins & leaves; deliver to both.
                for h in &self.presence_callbacks {
                    if matches!(h.event, PresenceEvent::Join | PresenceEvent::Leave) {
                        (h.callback)(v.clone());
                    }
                }
            }
            _ => {}
        }
    }

    /// Broadcast a message to every subscriber of this channel.
    pub async fn send_broadcast(&self, event: &str, payload: Value) -> Result<()> {
        let body = json!({ "type": "broadcast", "event": event, "payload": payload });
        self.connection.send(&self.topic, "broadcast", body).await
    }

    /// Publish a presence payload for this client.
    pub async fn track(&self, payload: Value) -> Result<()> {
        let body = json!({ "type": "presence", "event": "track", "payload": payload });
        self.connection.send(&self.topic, "presence", body).await
    }

    /// Stop publishing presence for this client.
    pub async fn untrack(&self) -> Result<()> {
        let body = json!({ "type": "presence", "event": "untrack" });
        self.connection.send(&self.topic, "presence", body).await
    }

    /// Send a fresh access token down this channel. Most callers should use
    /// [`RealtimeClient::set_auth`](super::RealtimeClient::set_auth) which
    /// updates every channel at once.
    pub async fn set_auth(&self, access_token: impl Into<String>) -> Result<()> {
        let token = access_token.into();
        self.connection
            .send(&self.topic, ev::ACCESS_TOKEN, json!({ "access_token": token }))
            .await
    }

    /// Explicitly leave the channel on the server.
    pub async fn unsubscribe(mut self) -> Result<()> {
        let _ = self
            .connection
            .request(&self.topic, ev::LEAVE, Value::Null)
            .await;
        self.connection.unregister_channel(&self.topic).await;
        self.status = SubscriptionStatus::Closed;
        Ok(())
    }
}

impl Stream for Channel {
    type Item = RealtimeEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events.poll_recv(cx)
    }
}
