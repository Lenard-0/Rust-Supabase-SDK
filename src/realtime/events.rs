//! User-facing event types yielded by a [`Channel`](super::Channel).

use serde::Deserialize;
use serde_json::Value;

/// What kind of Postgres change to subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostgresChangeKind {
    All,
    Insert,
    Update,
    Delete,
}

impl PostgresChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "*",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
        }
    }

    /// Match an incoming `record.type` field (`"INSERT"`/`"UPDATE"`/`"DELETE"`)
    /// against this subscription filter.
    pub fn matches(self, change_type: &str) -> bool {
        match self {
            Self::All => true,
            Self::Insert => change_type.eq_ignore_ascii_case("INSERT"),
            Self::Update => change_type.eq_ignore_ascii_case("UPDATE"),
            Self::Delete => change_type.eq_ignore_ascii_case("DELETE"),
        }
    }
}

/// Subscription filter for `postgres_changes`. Build via [`PostgresChangesFilter::new`] and chain.
#[derive(Debug, Clone)]
pub struct PostgresChangesFilter {
    pub(crate) event: PostgresChangeKind,
    pub(crate) schema: String,
    pub(crate) table: Option<String>,
    pub(crate) filter: Option<String>,
}

impl PostgresChangesFilter {
    pub fn new(event: PostgresChangeKind) -> Self {
        Self {
            event,
            schema: "public".to_string(),
            table: None,
            filter: None,
        }
    }

    pub fn schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = schema.into();
        self
    }

    pub fn table(mut self, table: impl Into<String>) -> Self {
        self.table = Some(table.into());
        self
    }

    /// PostgREST-style filter, e.g. `"id=eq.42"` or `"status=in.(active,trial)"`.
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "event": self.event.as_str(),
            "schema": self.schema,
        });
        if let Some(t) = &self.table {
            obj["table"] = serde_json::Value::String(t.clone());
        }
        if let Some(f) = &self.filter {
            obj["filter"] = serde_json::Value::String(f.clone());
        }
        obj
    }
}

/// Decoded `postgres_changes` event payload.
#[derive(Debug, Clone, Deserialize)]
pub struct PostgresChangePayload {
    #[serde(default, rename = "type")]
    pub change_type: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub table: String,
    #[serde(default)]
    pub commit_timestamp: Option<String>,
    #[serde(default)]
    pub record: Value,
    #[serde(default)]
    pub old_record: Value,
    #[serde(default)]
    pub errors: Option<Value>,
}

/// Broadcast event payload (free-form).
#[derive(Debug, Clone, Deserialize)]
pub struct BroadcastPayload {
    pub event: String,
    #[serde(default)]
    pub payload: Value,
}

/// Presence sync / diff payloads.
#[derive(Debug, Clone, Deserialize)]
pub struct PresencePayload(pub Value);

/// Which presence sub-event to subscribe to. Mirrors supabase-js's
/// `channel.on('presence', { event: 'sync' | 'join' | 'leave' }, cb)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceEvent {
    Sync,
    Join,
    Leave,
}

impl PresenceEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Join => "join",
            Self::Leave => "leave",
        }
    }
}

/// One event yielded by a [`Channel`](super::Channel).
#[derive(Debug, Clone)]
pub enum RealtimeEvent {
    /// A row was inserted, updated, or deleted.
    PostgresChange(PostgresChangePayload),
    /// A broadcast message was sent to the channel.
    Broadcast(BroadcastPayload),
    /// Presence state was synced (after join).
    PresenceSync(Value),
    /// Presence delta (someone joined or left).
    PresenceDiff(Value),
    /// A `system` message from the server.
    System(Value),
    /// The channel encountered a non-fatal server error.
    Error(String),
    /// The server closed this channel.
    Closed,
}

/// Subscription state, mirroring supabase-js's `RealtimeChannel.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Subscribing,
    Subscribed,
    TimedOut,
    Closed,
    ChannelError,
}

impl SubscriptionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subscribing => "SUBSCRIBING",
            Self::Subscribed => "SUBSCRIBED",
            Self::TimedOut => "TIMED_OUT",
            Self::Closed => "CLOSED",
            Self::ChannelError => "CHANNEL_ERROR",
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- PostgresChangeKind ---

    #[test]
    fn kind_as_str_all_variants() {
        assert_eq!(PostgresChangeKind::All.as_str(), "*");
        assert_eq!(PostgresChangeKind::Insert.as_str(), "INSERT");
        assert_eq!(PostgresChangeKind::Update.as_str(), "UPDATE");
        assert_eq!(PostgresChangeKind::Delete.as_str(), "DELETE");
    }

    #[test]
    fn kind_matches_case_insensitive() {
        assert!(PostgresChangeKind::Insert.matches("INSERT"));
        assert!(PostgresChangeKind::Insert.matches("insert"));
        assert!(PostgresChangeKind::Insert.matches("Insert"));
        assert!(!PostgresChangeKind::Insert.matches("UPDATE"));
        assert!(!PostgresChangeKind::Insert.matches("DELETE"));
    }

    #[test]
    fn kind_all_matches_everything() {
        assert!(PostgresChangeKind::All.matches("INSERT"));
        assert!(PostgresChangeKind::All.matches("UPDATE"));
        assert!(PostgresChangeKind::All.matches("DELETE"));
        assert!(PostgresChangeKind::All.matches("anything"));
    }

    #[test]
    fn kind_update_matches_only_update() {
        assert!(PostgresChangeKind::Update.matches("UPDATE"));
        assert!(PostgresChangeKind::Update.matches("update"));
        assert!(!PostgresChangeKind::Update.matches("INSERT"));
    }

    #[test]
    fn kind_delete_matches_only_delete() {
        assert!(PostgresChangeKind::Delete.matches("DELETE"));
        assert!(!PostgresChangeKind::Delete.matches("UPDATE"));
    }

    // --- PostgresChangesFilter ---

    #[test]
    fn filter_defaults_to_public_schema() {
        let f = PostgresChangesFilter::new(PostgresChangeKind::Insert);
        assert_eq!(f.schema, "public");
        assert!(f.table.is_none());
        assert!(f.filter.is_none());
    }

    #[test]
    fn filter_to_json_minimal() {
        let f = PostgresChangesFilter::new(PostgresChangeKind::Update).schema("auth");
        assert_eq!(
            f.to_json(),
            json!({"event": "UPDATE", "schema": "auth"})
        );
    }

    #[test]
    fn filter_to_json_with_table_and_filter() {
        let f = PostgresChangesFilter::new(PostgresChangeKind::Delete)
            .schema("public")
            .table("messages")
            .filter("user_id=eq.42");
        assert_eq!(
            f.to_json(),
            json!({"event": "DELETE", "schema": "public", "table": "messages", "filter": "user_id=eq.42"})
        );
    }

    #[test]
    fn filter_all_event_to_json() {
        let f = PostgresChangesFilter::new(PostgresChangeKind::All);
        assert_eq!(f.to_json()["event"], "*");
    }

    #[test]
    fn filter_table_not_set_omits_key() {
        let f = PostgresChangesFilter::new(PostgresChangeKind::Insert);
        let j = f.to_json();
        assert!(j.get("table").is_none(), "table should be absent when not set");
    }

    #[test]
    fn filter_filter_not_set_omits_key() {
        let f = PostgresChangesFilter::new(PostgresChangeKind::Insert).table("t");
        let j = f.to_json();
        assert!(j.get("filter").is_none(), "filter key should be absent when not set");
    }

    // --- PresenceEvent ---

    #[test]
    fn presence_event_strings() {
        assert_eq!(PresenceEvent::Sync.as_str(), "sync");
        assert_eq!(PresenceEvent::Join.as_str(), "join");
        assert_eq!(PresenceEvent::Leave.as_str(), "leave");
    }

    #[test]
    fn presence_event_eq() {
        assert_eq!(PresenceEvent::Sync, PresenceEvent::Sync);
        assert_ne!(PresenceEvent::Sync, PresenceEvent::Join);
    }

    // --- SubscriptionStatus ---

    #[test]
    fn subscription_status_strings() {
        assert_eq!(SubscriptionStatus::Subscribing.as_str(), "SUBSCRIBING");
        assert_eq!(SubscriptionStatus::Subscribed.as_str(), "SUBSCRIBED");
        assert_eq!(SubscriptionStatus::TimedOut.as_str(), "TIMED_OUT");
        assert_eq!(SubscriptionStatus::Closed.as_str(), "CLOSED");
        assert_eq!(SubscriptionStatus::ChannelError.as_str(), "CHANNEL_ERROR");
    }

    // --- PostgresChangePayload deserialization ---

    #[test]
    fn postgres_change_payload_deserializes_insert() {
        let payload = json!({
            "type": "INSERT",
            "schema": "public",
            "table": "messages",
            "commit_timestamp": "2024-01-01T00:00:00Z",
            "record": {"id": 1, "text": "hello"},
            "old_record": {},
            "errors": null
        });
        let p: PostgresChangePayload = serde_json::from_value(payload).unwrap();
        assert_eq!(p.change_type, "INSERT");
        assert_eq!(p.schema, "public");
        assert_eq!(p.table, "messages");
        assert_eq!(p.record["id"], 1);
    }

    #[test]
    fn postgres_change_payload_deserializes_missing_optional_fields() {
        let payload = json!({ "type": "DELETE", "schema": "public", "table": "t" });
        let p: PostgresChangePayload = serde_json::from_value(payload).unwrap();
        assert_eq!(p.change_type, "DELETE");
        assert!(p.commit_timestamp.is_none());
        assert!(p.errors.is_none());
    }

    #[test]
    fn broadcast_payload_deserializes() {
        let payload = json!({"event": "cursor-pos", "payload": {"x": 10, "y": 20}});
        let b: BroadcastPayload = serde_json::from_value(payload).unwrap();
        assert_eq!(b.event, "cursor-pos");
        assert_eq!(b.payload["x"], 10);
    }

    #[test]
    fn broadcast_payload_missing_payload_defaults_to_null() {
        let payload = json!({"event": "ping"});
        let b: BroadcastPayload = serde_json::from_value(payload).unwrap();
        assert_eq!(b.event, "ping");
        assert_eq!(b.payload, json!(null));
    }

    // --- RealtimeEvent variants are Send + Sync ---

    #[test]
    fn realtime_event_is_debug_and_clone() {
        let e = RealtimeEvent::Closed;
        let cloned = e.clone();
        let _fmt = format!("{cloned:?}");
    }

    #[test]
    fn realtime_event_error_holds_string() {
        let e = RealtimeEvent::Error("something bad".into());
        match e {
            RealtimeEvent::Error(msg) => assert_eq!(msg, "something bad"),
            _ => panic!("wrong variant"),
        }
    }
}
