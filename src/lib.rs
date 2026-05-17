//! An async Rust client for [Supabase](https://supabase.com).
//!
//! Mirrors the `supabase-js` surface area where it makes sense and pushes
//! Rust-native ergonomics elsewhere: a chainable PostgREST builder, typed row
//! queries via [`from_row`], a pluggable [`SessionStore`] for auth persistence,
//! retry/backoff for 429s, and feature-gated realtime + edge functions.
//!
//! # Quickstart
//!
//! ```no_run
//! use rust_supabase_sdk::SupabaseClient;
//!
//! #[tokio::main]
//! async fn main() -> rust_supabase_sdk::Result<()> {
//!     let client = SupabaseClient::new(
//!         std::env::var("SUPABASE_URL").unwrap_or_default(),
//!         std::env::var("SUPABASE_API_KEY").unwrap_or_default(),
//!         None,
//!     );
//!
//!     let rows: Vec<serde_json::Value> = client
//!         .from("countries")
//!         .select("id,name")
//!         .eq("region", "Europe")
//!         .order("name", true)
//!         .limit(10)
//!         .await?;
//!
//!     for row in rows {
//!         println!("{row}");
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Modules
//!
//! - [`postgrest`] — chainable query builder + typed row queries.
//! - [`auth`] — sign-in (email / phone / OTP / OAuth / anonymous), recovery,
//!   admin user management, and the [`SessionStore`] trait.
//! - [`storage`] — buckets, object CRUD, signed URLs, image transforms.
//! - [`rpc`] — call Postgres functions.
//! - [`functions`] — invoke Supabase Edge Functions (feature `functions`).
//! - [`realtime`] — websocket subscriptions to `postgres_changes`,
//!   broadcast, and presence (feature `realtime`, off by default).
//!
//! # Feature flags
//!
//! | Flag         | Default | What it does                                  |
//! |--------------|:-------:|-----------------------------------------------|
//! | `postgrest`  | ✅      | PostgREST query builder.                      |
//! | `auth`       | ✅      | Sign-in flows, OAuth, admin user management.  |
//! | `storage`    | ✅      | Buckets, objects, signed URLs.                |
//! | `functions`  | ✅      | Edge Functions invocation.                    |
//! | `realtime`   | —       | Websocket subscriptions (opt-in).             |
//! | `rustls`     | ✅      | TLS via `rustls` (default).                   |
//! | `native-tls` | —       | OS-native TLS instead of `rustls`.            |
//!
//! # Customizing the client
//!
//! Use [`SupabaseClient::builder`] when you need a custom schema, retry policy,
//! HTTP client, persistent session store, or extra headers:
//!
//! ```no_run
//! use std::time::Duration;
//! use rust_supabase_sdk::{SupabaseClient, RetryConfig};
//!
//! let client = SupabaseClient::builder("https://proj.supabase.co", "anon-key")
//!     .timeout(Duration::from_secs(30))
//!     .retry(RetryConfig::new(3, Duration::from_millis(100)))
//!     .user_agent("my-app/1.0")
//!     .schema("public")
//!     .build();
//! ```
//!
//! # Code generation
//!
//! The companion `cargo-supabase` binary introspects a project's PostgREST
//! schema and emits Rust row structs ready for `client.from_row::<T>()`:
//!
//! ```text
//! cargo install cargo-supabase
//! cargo supabase gen types --url $SUPABASE_URL --apikey $SUPABASE_SERVICE_ROLE_KEY \
//!     --output src/generated.rs
//! ```
//!
//! # Examples
//!
//! Worked examples for every major surface area live under `examples/` in the
//! [repository](https://github.com/Lenard-0/Rust-Supabase-SDK).
//!
//! [`from_row`]: crate::SupabaseClient::from_row
//! [`SessionStore`]: auth::session_store::SessionStore

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![warn(clippy::all)]

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

pub mod auth;
pub mod error;
#[cfg(feature = "functions")]
pub mod functions;
pub mod postgrest;
#[cfg(feature = "realtime")]
pub mod realtime;
pub mod rpc;
pub mod storage;
pub mod universals;

pub use auth::{
    session_store::{InMemorySessionStore, SessionStore},
    types::{Identity, Session, User},
    Auth, AuthAdmin,
};
pub use error::{AuthError, PostgrestError, Result, StorageError, SupabaseError};
pub use postgrest::Row;

/// Retry policy applied to transient (429/5xx) HTTP failures.
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_backoff: Duration,
}

impl RetryConfig {
    pub const fn new(max_retries: u32, base_backoff: Duration) -> Self {
        Self { max_retries, base_backoff }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self { max_retries: 5, base_backoff: Duration::from_millis(50) }
    }
}

/// The main Supabase client. Cheap to `clone` — internal state is `Arc`-shared.
///
/// Build via [`SupabaseClient::new`] for the common case, or [`SupabaseClient::builder`]
/// when you need a custom schema, additional headers, a pre-configured HTTP client,
/// or a persistent session store.
#[derive(Debug, Clone)]
pub struct SupabaseClient {
    pub url: String,
    pub api_key: String,
    /// Legacy fallback bearer token for clients constructed without a session store.
    /// New code should prefer signing in via the auth namespace, which populates
    /// the internal session store automatically.
    pub access_token: Option<String>,
    pub(crate) schema: Option<String>,
    pub(crate) extra_headers: Vec<(String, String)>,
    pub(crate) http: reqwest::Client,
    pub(crate) session_store: Arc<dyn SessionStore>,
    pub(crate) retry: RetryConfig,
}

impl SupabaseClient {
    /// Construct a client with default HTTP settings and an in-memory session store.
    pub fn new(
        supabase_url: impl Into<String>,
        private_key: impl Into<String>,
        access_token: Option<String>,
    ) -> Self {
        Self {
            url: supabase_url.into(),
            api_key: private_key.into(),
            access_token,
            schema: None,
            extra_headers: Vec::new(),
            http: reqwest::Client::new(),
            session_store: Arc::new(InMemorySessionStore::new()),
            retry: RetryConfig::default(),
        }
    }

    /// Start building a customized client.
    pub fn builder(url: impl Into<String>, api_key: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            url: url.into(),
            api_key: api_key.into(),
            access_token: None,
            schema: None,
            extra_headers: Vec::new(),
            http: None,
            session_store: None,
            retry: RetryConfig::default(),
            timeout: None,
            user_agent: None,
        }
    }

    /// Return a clone of this client with the schema header set.
    pub fn schema(&self, schema: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.schema = Some(schema.into());
        next
    }

    /// Return a clone of this client with the access token replaced.
    pub fn with_access_token(&self, token: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.access_token = Some(token.into());
        next
    }

    /// The bearer token applied to outgoing requests when no per-request override
    /// is supplied. Prefers the live session, then the legacy `access_token` field,
    /// then the api key (anon role).
    pub(crate) fn effective_bearer(&self) -> String {
        if let Some(session) = self.session_store.get() {
            return session.access_token;
        }
        if let Some(token) = &self.access_token {
            return token.clone();
        }
        self.api_key.clone()
    }
}

/// Builder for customizing a [`SupabaseClient`].
pub struct ClientBuilder {
    url: String,
    api_key: String,
    access_token: Option<String>,
    schema: Option<String>,
    extra_headers: Vec<(String, String)>,
    http: Option<reqwest::Client>,
    session_store: Option<Arc<dyn SessionStore>>,
    retry: RetryConfig,
    timeout: Option<Duration>,
    user_agent: Option<String>,
}

impl std::fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("url", &self.url)
            .field("schema", &self.schema)
            .field("extra_headers", &self.extra_headers)
            .field("retry", &self.retry)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl ClientBuilder {
    pub fn access_token(mut self, token: impl Into<String>) -> Self {
        self.access_token = Some(token.into());
        self
    }

    pub fn schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = Some(schema.into());
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((name.into(), value.into()));
        self
    }

    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http = Some(client);
        self
    }

    /// Plug in a custom [`SessionStore`]. If unset, an in-memory store is used.
    pub fn session_store<S: SessionStore + 'static>(mut self, store: S) -> Self {
        self.session_store = Some(Arc::new(store));
        self
    }

    /// Override the default retry policy.
    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Cap the number of automatic retries on 429.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.retry.max_retries = n;
        self
    }

    /// Per-request timeout, applied when building the internal `reqwest::Client`.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Custom `User-Agent` header.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    pub fn build(self) -> SupabaseClient {
        let http = self.http.unwrap_or_else(|| {
            let mut b = reqwest::Client::builder();
            if let Some(t) = self.timeout {
                b = b.timeout(t);
            }
            if let Some(ua) = &self.user_agent {
                b = b.user_agent(ua);
            }
            b.build().unwrap_or_default()
        });
        SupabaseClient {
            url: self.url,
            api_key: self.api_key,
            access_token: self.access_token,
            schema: self.schema,
            extra_headers: self.extra_headers,
            http,
            session_store: self
                .session_store
                .unwrap_or_else(|| Arc::new(InMemorySessionStore::new())),
            retry: self.retry,
        }
    }
}

/// Generate a fresh v4 UUID as a `String`. Useful for client-side primary keys.
pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- RetryConfig ---

    #[test]
    fn retry_config_new() {
        let r = RetryConfig::new(3, Duration::from_millis(100));
        assert_eq!(r.max_retries, 3);
        assert_eq!(r.base_backoff, Duration::from_millis(100));
    }

    #[test]
    fn retry_config_default() {
        let r = RetryConfig::default();
        assert_eq!(r.max_retries, 5);
        assert_eq!(r.base_backoff, Duration::from_millis(50));
    }

    #[test]
    fn retry_config_is_copy() {
        let r = RetryConfig::default();
        let r2 = r; // Copy
        assert_eq!(r.max_retries, r2.max_retries);
    }

    // --- generate_id ---

    #[test]
    fn generate_id_returns_uuid_format() {
        let id = generate_id();
        // UUID v4: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx (36 chars with hyphens)
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5, "UUID should have 5 hyphen-separated groups");
        assert_eq!(parts[2].chars().next(), Some('4'), "third group should start with 4 (v4)");
    }

    #[test]
    fn generate_id_is_unique() {
        let a = generate_id();
        let b = generate_id();
        assert_ne!(a, b, "two generated IDs should differ");
    }

    // --- SupabaseClient::new ---

    #[test]
    fn client_new_stores_fields() {
        let c = SupabaseClient::new("https://proj.supabase.co", "anon-key", None);
        assert_eq!(c.url, "https://proj.supabase.co");
        assert_eq!(c.api_key, "anon-key");
        assert!(c.access_token.is_none());
    }

    #[test]
    fn client_new_with_access_token() {
        let c = SupabaseClient::new("https://proj.supabase.co", "key", Some("tok".into()));
        assert_eq!(c.access_token.as_deref(), Some("tok"));
    }

    // --- SupabaseClient::schema / with_access_token ---

    #[test]
    fn client_schema_returns_new_instance_with_schema() {
        let base = SupabaseClient::new("https://proj.supabase.co", "k", None);
        let scoped = base.schema("extensions");
        // Original untouched.
        assert!(base.schema.is_none());
        assert_eq!(scoped.schema.as_deref(), Some("extensions"));
    }

    #[test]
    fn client_with_access_token_replaces_token() {
        let base = SupabaseClient::new("https://proj.supabase.co", "k", Some("old".into()));
        let patched = base.with_access_token("new");
        assert_eq!(patched.access_token.as_deref(), Some("new"));
        // Original unchanged.
        assert_eq!(base.access_token.as_deref(), Some("old"));
    }

    // --- effective_bearer priority ---

    #[test]
    fn effective_bearer_falls_back_to_api_key() {
        let c = SupabaseClient::new("https://x.co", "anon-key", None);
        assert_eq!(c.effective_bearer(), "anon-key");
    }

    #[test]
    fn effective_bearer_prefers_access_token_over_api_key() {
        let c = SupabaseClient::new("https://x.co", "anon", Some("user-jwt".into()));
        assert_eq!(c.effective_bearer(), "user-jwt");
    }

    #[test]
    fn effective_bearer_prefers_session_over_access_token() {
        use crate::auth::types::User;
        use crate::auth::session_store::InMemorySessionStore;
        use crate::auth::types::Session;
        use chrono::Utc;
        let store = InMemorySessionStore::new();
        let user: User = serde_json::from_value(serde_json::json!({
            "id": "u1", "aud": "auth", "role": "auth", "created_at": "2024-01-01T00:00:00Z"
        })).unwrap();
        store.set(Session {
            access_token: "session-jwt".into(),
            token_type: "bearer".into(),
            expires_in: 3600,
            expires_at: Utc::now().timestamp() + 3600,
            refresh_token: "rt".into(),
            user,
        });
        let c = SupabaseClient::builder("https://x.co", "anon")
            .access_token("legacy-token")
            .session_store(store)
            .build();
        assert_eq!(c.effective_bearer(), "session-jwt");
    }

    // --- ClientBuilder ---

    #[test]
    fn builder_sets_fields() {
        let c = SupabaseClient::builder("https://proj.co", "api-key")
            .access_token("tok")
            .schema("myschema")
            .header("X-Custom", "val")
            .max_retries(3)
            .timeout(Duration::from_secs(30))
            .user_agent("test-agent/1.0")
            .build();
        assert_eq!(c.url, "https://proj.co");
        assert_eq!(c.api_key, "api-key");
        assert_eq!(c.access_token.as_deref(), Some("tok"));
        assert_eq!(c.schema.as_deref(), Some("myschema"));
        assert_eq!(c.extra_headers, vec![("X-Custom".into(), "val".into())]);
        assert_eq!(c.retry.max_retries, 3);
    }

    #[test]
    fn builder_retry_full() {
        let c = SupabaseClient::builder("https://x.co", "k")
            .retry(RetryConfig::new(10, Duration::from_secs(1)))
            .build();
        assert_eq!(c.retry.max_retries, 10);
        assert_eq!(c.retry.base_backoff, Duration::from_secs(1));
    }

    #[test]
    fn builder_multiple_headers_accumulated() {
        let c = SupabaseClient::builder("https://x.co", "k")
            .header("A", "1")
            .header("B", "2")
            .build();
        assert_eq!(c.extra_headers.len(), 2);
    }

    #[test]
    fn builder_debug_does_not_panic() {
        let b = SupabaseClient::builder("https://x.co", "k").schema("public");
        let _ = format!("{b:?}");
    }

    #[test]
    fn builder_http_client_overrides_default() {
        let custom = reqwest::Client::builder()
            .user_agent("custom-ua")
            .build()
            .unwrap();
        let c = SupabaseClient::builder("https://x.co", "k")
            .http_client(custom)
            .build();
        // We can't compare reqwest::Client instances; just prove the build path runs.
        assert_eq!(c.url, "https://x.co");
    }
}
