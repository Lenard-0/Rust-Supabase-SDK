//! Phoenix Channels wire format used by the Supabase Realtime server.
//!
//! Messages take the shape:
//! ```json
//! { "topic": "realtime:public:t", "event": "...", "payload": {...}, "ref": "1" }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OutgoingMessage<'a> {
    pub topic: &'a str,
    pub event: &'a str,
    pub payload: Value,
    #[serde(rename = "ref")]
    pub message_ref: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IncomingMessage {
    pub topic: String,
    pub event: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default, rename = "ref")]
    pub message_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReplyPayload {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub response: Value,
}

/// Phoenix system events we recognize.
pub(crate) mod events {
    pub const JOIN: &str = "phx_join";
    pub const LEAVE: &str = "phx_leave";
    pub const REPLY: &str = "phx_reply";
    pub const ERROR: &str = "phx_error";
    pub const CLOSE: &str = "phx_close";
    pub const HEARTBEAT: &str = "heartbeat";
    pub const SYSTEM: &str = "system";

    pub const POSTGRES_CHANGES: &str = "postgres_changes";
    pub const BROADCAST: &str = "broadcast";
    pub const PRESENCE_STATE: &str = "presence_state";
    pub const PRESENCE_DIFF: &str = "presence_diff";

    /// Sent by the client to refresh the JWT on an already-joined channel.
    pub const ACCESS_TOKEN: &str = "access_token";
}

pub(crate) const HEARTBEAT_TOPIC: &str = "phoenix";

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- OutgoingMessage serialization ---

    #[test]
    fn outgoing_message_serializes_fields() {
        let msg = OutgoingMessage {
            topic: "realtime:public:users",
            event: events::JOIN,
            payload: json!({ "config": {} }),
            message_ref: "1".into(),
        };
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["topic"], "realtime:public:users");
        assert_eq!(v["event"], "phx_join");
        assert_eq!(v["ref"], "1");
        assert!(v.get("payload").is_some());
    }

    #[test]
    fn outgoing_message_ref_key_renamed() {
        // "ref" is the JSON key; "message_ref" is the Rust field
        let msg = OutgoingMessage {
            topic: "t",
            event: "e",
            payload: Value::Null,
            message_ref: "42".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"ref\":\"42\""), "{s}");
        assert!(!s.contains("message_ref"), "{s}");
    }

    // --- IncomingMessage deserialization ---

    #[test]
    fn incoming_message_deserializes_full() {
        let v = json!({
            "topic": "realtime:public:posts",
            "event": "postgres_changes",
            "payload": { "type": "INSERT" },
            "ref": "7"
        });
        let m: IncomingMessage = serde_json::from_value(v).unwrap();
        assert_eq!(m.topic, "realtime:public:posts");
        assert_eq!(m.event, "postgres_changes");
        assert_eq!(m.message_ref.as_deref(), Some("7"));
        assert_eq!(m.payload["type"], "INSERT");
    }

    #[test]
    fn incoming_message_ref_optional_absent() {
        let v = json!({ "topic": "t", "event": "e", "payload": {} });
        let m: IncomingMessage = serde_json::from_value(v).unwrap();
        assert!(m.message_ref.is_none());
    }

    #[test]
    fn incoming_message_payload_defaults_null() {
        let v = json!({ "topic": "t", "event": "e" });
        let m: IncomingMessage = serde_json::from_value(v).unwrap();
        assert_eq!(m.payload, Value::Null);
    }

    // --- ReplyPayload ---

    #[test]
    fn reply_payload_deserializes() {
        let v = json!({ "status": "ok", "response": { "postgres_changes": [] } });
        let r: ReplyPayload = serde_json::from_value(v).unwrap();
        assert_eq!(r.status, "ok");
        assert!(r.response["postgres_changes"].is_array());
    }

    #[test]
    fn reply_payload_defaults_empty() {
        let v = json!({});
        let r: ReplyPayload = serde_json::from_value(v).unwrap();
        assert_eq!(r.status, "");
        assert_eq!(r.response, Value::Null);
    }

    // --- event string constants ---

    #[test]
    fn event_constants_match_phoenix_protocol() {
        assert_eq!(events::JOIN, "phx_join");
        assert_eq!(events::LEAVE, "phx_leave");
        assert_eq!(events::REPLY, "phx_reply");
        assert_eq!(events::ERROR, "phx_error");
        assert_eq!(events::CLOSE, "phx_close");
        assert_eq!(events::HEARTBEAT, "heartbeat");
        assert_eq!(events::SYSTEM, "system");
    }

    #[test]
    fn supabase_event_constants() {
        assert_eq!(events::POSTGRES_CHANGES, "postgres_changes");
        assert_eq!(events::BROADCAST, "broadcast");
        assert_eq!(events::PRESENCE_STATE, "presence_state");
        assert_eq!(events::PRESENCE_DIFF, "presence_diff");
        assert_eq!(events::ACCESS_TOKEN, "access_token");
    }

    #[test]
    fn heartbeat_topic_is_phoenix() {
        assert_eq!(HEARTBEAT_TOPIC, "phoenix");
    }
}
