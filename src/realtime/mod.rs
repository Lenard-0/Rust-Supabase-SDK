//! Supabase Realtime — WebSocket-based subscriptions for Postgres changes,
//! broadcast, and presence.
//!
//! Behind the `realtime` feature flag.
//!
//! Two complementary APIs share the same connection:
//!
//! - **Stream style** — `let mut ch = client.realtime().connect().await?.channel("t").on_postgres_changes(...).subscribe().await?;`
//!   then `while let Some(event) = ch.recv().await { ... }`.
//! - **Callback style** — register callbacks via `on_postgres_changes_callback` / `on_broadcast_callback` / `on_presence_callback`
//!   and call `Channel::run().await` (or `tokio::spawn(async move { ch.run().await })`).
//!
//! ```no_run
//! # use rust_supabase_sdk::SupabaseClient;
//! # use rust_supabase_sdk::realtime::{PostgresChangeKind, PostgresChangesFilter, RealtimeEvent};
//! # async fn demo(client: SupabaseClient) -> rust_supabase_sdk::Result<()> {
//! let rt = client.realtime().connect().await?;
//! let mut channel = rt
//!     .channel("realtime:public:messages")
//!     .on_postgres_changes(
//!         PostgresChangesFilter::new(PostgresChangeKind::All)
//!             .schema("public")
//!             .table("messages"),
//!     )
//!     .subscribe()
//!     .await?;
//!
//! while let Some(event) = channel.recv().await {
//!     match event {
//!         RealtimeEvent::PostgresChange(c) => println!("change: {} on {}", c.change_type, c.table),
//!         RealtimeEvent::Closed => break,
//!         _ => {}
//!     }
//! }
//! # Ok(()) }
//! ```

mod channel;
mod connection;
pub mod events;
mod protocol;

pub use channel::{
    BroadcastCallback, Channel, ChannelBuilder, PostgresChangesCallback, PresenceCallback,
};
pub use connection::ReconnectPolicy;
pub use events::{
    BroadcastPayload, PostgresChangeKind, PostgresChangePayload, PostgresChangesFilter,
    PresenceEvent, PresencePayload, RealtimeEvent, SubscriptionStatus,
};

use crate::error::Result;
use crate::SupabaseClient;

impl SupabaseClient {
    /// Open the realtime namespace. Call [`Realtime::connect`] to establish
    /// the WebSocket.
    pub fn realtime(&self) -> Realtime {
        Realtime {
            client: self.clone(),
            reconnect: ReconnectPolicy::default(),
        }
    }
}

/// Realtime namespace — call [`connect`](Self::connect) to spin up a
/// shared WebSocket connection.
#[derive(Debug, Clone)]
pub struct Realtime {
    pub(crate) client: SupabaseClient,
    pub(crate) reconnect: ReconnectPolicy,
}

impl Realtime {
    /// Override the default reconnect policy (exponential backoff capped at
    /// 30s, unlimited attempts).
    pub fn reconnect(mut self, policy: ReconnectPolicy) -> Self {
        self.reconnect = policy;
        self
    }

    /// Disable automatic reconnection. Channels close on the first disconnect.
    pub fn no_reconnect(mut self) -> Self {
        self.reconnect.enabled = false;
        self
    }

    /// Establish the WebSocket connection. Returns a handle that can spawn
    /// many [`Channel`]s.
    pub async fn connect(&self) -> Result<RealtimeClient> {
        let url = build_url(&self.client);
        let connection = connection::Connection::connect(&url, self.reconnect).await?;
        let token = live_access_token(&self.client);
        connection.set_auth(token.clone()).await?;
        Ok(RealtimeClient { connection, access_token: token })
    }
}

fn build_url(client: &SupabaseClient) -> String {
    let base = client
        .url
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    let bearer = client.effective_bearer();
    format!("{base}/realtime/v1/websocket?apikey={bearer}&vsn=1.0.0")
}

fn live_access_token(client: &SupabaseClient) -> Option<String> {
    client.session_store.get().map(|s| s.access_token)
}

/// A live realtime connection. Cheap to clone — channels share the underlying
/// WebSocket.
#[derive(Clone)]
pub struct RealtimeClient {
    pub(crate) connection: connection::Connection,
    pub(crate) access_token: Option<String>,
}

impl RealtimeClient {
    /// Create a new channel for `topic` (e.g. `"realtime:public:messages"` or
    /// `"room-1"` for broadcast-only channels).
    pub fn channel(&self, topic: impl Into<String>) -> ChannelBuilder {
        ChannelBuilder::new(
            self.connection.clone(),
            topic.into(),
            self.access_token.clone(),
        )
    }

    /// Update the bearer token used by every joined channel. Mirrors
    /// `supabase-js`'s `realtime.setAuth(token)`. Pass `None` to clear.
    pub async fn set_auth(&self, access_token: Option<String>) -> Result<()> {
        self.connection.set_auth(access_token).await
    }

    /// The token currently associated with this connection, if any.
    pub async fn access_token(&self) -> Option<String> {
        self.connection.access_token().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn client() -> SupabaseClient {
        SupabaseClient::new("https://example.supabase.co", "anon-key", None)
    }

    #[test]
    fn ws_url_uses_wss_and_apikey() {
        let url = build_url(&client());
        assert_eq!(
            url,
            "wss://example.supabase.co/realtime/v1/websocket?apikey=anon-key&vsn=1.0.0"
        );
    }

    #[test]
    fn ws_url_uses_ws_for_http() {
        let client = SupabaseClient::new("http://localhost:54321", "key", None);
        let url = build_url(&client);
        assert!(url.starts_with("ws://localhost:54321/realtime/v1/websocket"));
    }

    #[test]
    fn postgres_changes_filter_serializes() {
        use serde_json::json;
        let filter = PostgresChangesFilter::new(PostgresChangeKind::Insert)
            .schema("public")
            .table("messages")
            .filter("room_id=eq.7");
        assert_eq!(
            filter.to_json(),
            json!({
                "event": "INSERT",
                "schema": "public",
                "table": "messages",
                "filter": "room_id=eq.7"
            })
        );
    }

    #[test]
    fn postgres_change_kind_strings() {
        assert_eq!(PostgresChangeKind::All.as_str(), "*");
        assert_eq!(PostgresChangeKind::Insert.as_str(), "INSERT");
        assert_eq!(PostgresChangeKind::Update.as_str(), "UPDATE");
        assert_eq!(PostgresChangeKind::Delete.as_str(), "DELETE");
    }

    #[test]
    fn presence_event_strings() {
        assert_eq!(PresenceEvent::Sync.as_str(), "sync");
        assert_eq!(PresenceEvent::Join.as_str(), "join");
        assert_eq!(PresenceEvent::Leave.as_str(), "leave");
    }

    #[test]
    fn postgres_kind_matches() {
        assert!(PostgresChangeKind::All.matches("INSERT"));
        assert!(PostgresChangeKind::Insert.matches("INSERT"));
        assert!(!PostgresChangeKind::Insert.matches("UPDATE"));
        assert!(PostgresChangeKind::Update.matches("update"));
    }
}
