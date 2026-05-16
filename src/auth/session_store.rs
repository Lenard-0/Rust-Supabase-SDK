//! Pluggable persistence for the current [`Session`].
//!
//! The default is [`InMemorySessionStore`] — a process-local `RwLock`. Plug in
//! your own implementation via [`ClientBuilder::session_store`](crate::ClientBuilder::session_store)
//! to persist to disk, the OS keyring, or a custom KV store.

use std::sync::RwLock;

use super::types::Session;

/// A backing store for the active session.
///
/// All methods are synchronous to keep them callable from middleware paths.
/// Implementations should be cheap to clone via `Arc`.
pub trait SessionStore: Send + Sync + std::fmt::Debug {
    fn get(&self) -> Option<Session>;
    fn set(&self, session: Session);
    fn clear(&self);
}

/// Default in-memory store. Cheap; not persisted across restarts.
#[derive(Debug, Default)]
pub struct InMemorySessionStore {
    inner: RwLock<Option<Session>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionStore for InMemorySessionStore {
    fn get(&self) -> Option<Session> {
        match self.inner.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn set(&self, session: Session) {
        match self.inner.write() {
            Ok(mut guard) => *guard = Some(session),
            Err(poisoned) => *poisoned.into_inner() = Some(session),
        }
    }

    fn clear(&self) {
        match self.inner.write() {
            Ok(mut guard) => *guard = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn make_session(token: &str) -> Session {
        Session {
            access_token: token.into(),
            token_type: "bearer".into(),
            expires_in: 3600,
            expires_at: Utc::now().timestamp() + 3600,
            refresh_token: "rt".into(),
            user: serde_json::from_value(json!({
                "id": "u1", "aud": "auth", "role": "auth",
                "created_at": "2024-01-01T00:00:00Z"
            }))
            .unwrap(),
        }
    }

    #[test]
    fn starts_empty() {
        let store = InMemorySessionStore::new();
        assert!(store.get().is_none());
    }

    #[test]
    fn set_then_get_returns_session() {
        let store = InMemorySessionStore::new();
        store.set(make_session("tok-1"));
        assert_eq!(store.get().unwrap().access_token, "tok-1");
    }

    #[test]
    fn set_overwrites_previous_session() {
        let store = InMemorySessionStore::new();
        store.set(make_session("old"));
        store.set(make_session("new"));
        assert_eq!(store.get().unwrap().access_token, "new");
    }

    #[test]
    fn clear_removes_session() {
        let store = InMemorySessionStore::new();
        store.set(make_session("tok"));
        store.clear();
        assert!(store.get().is_none());
    }

    #[test]
    fn clear_on_empty_is_a_noop() {
        let store = InMemorySessionStore::new();
        store.clear(); // should not panic
        assert!(store.get().is_none());
    }

    #[test]
    fn default_equals_new() {
        let store = InMemorySessionStore::default();
        assert!(store.get().is_none());
    }

    #[test]
    fn store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InMemorySessionStore>();
    }

    #[test]
    fn get_clones_session_independently() {
        let store = InMemorySessionStore::new();
        store.set(make_session("tok"));
        let s1 = store.get().unwrap();
        store.set(make_session("new-tok"));
        // s1 still holds the snapshot taken before the overwrite
        assert_eq!(s1.access_token, "tok");
        assert_eq!(store.get().unwrap().access_token, "new-tok");
    }

    #[test]
    fn user_preserved_through_session_store() {
        let store = InMemorySessionStore::new();
        let session = make_session("tok");
        let user_id = session.user.id.clone();
        store.set(session);
        assert_eq!(store.get().unwrap().user.id, user_id);
    }

    /// Poison the inner lock from a thread that panics while holding the write
    /// guard, then verify `get`/`set`/`clear` still work via `poisoned.into_inner()`.
    #[test]
    fn get_recovers_from_poisoned_lock() {
        use std::sync::Arc;
        let store = Arc::new(InMemorySessionStore::new());
        store.set(make_session("before-poison"));

        let s = Arc::clone(&store);
        let handle = std::thread::spawn(move || {
            // Grab the write lock, then panic — this poisons the RwLock.
            let _guard = s.inner.write().unwrap();
            panic!("intentional poison");
        });
        assert!(handle.join().is_err(), "thread should have panicked");

        // The lock is now poisoned. `get` must still return Some(...).
        let s = store.get();
        assert!(s.is_some());
        assert_eq!(s.unwrap().access_token, "before-poison");
    }

    #[test]
    fn set_recovers_from_poisoned_lock() {
        use std::sync::Arc;
        let store = Arc::new(InMemorySessionStore::new());

        let s = Arc::clone(&store);
        let handle = std::thread::spawn(move || {
            let _guard = s.inner.write().unwrap();
            panic!("intentional poison");
        });
        assert!(handle.join().is_err());

        // After poisoning, `set` must still succeed.
        store.set(make_session("after-poison"));
        assert_eq!(store.get().unwrap().access_token, "after-poison");
    }

    #[test]
    fn clear_recovers_from_poisoned_lock() {
        use std::sync::Arc;
        let store = Arc::new(InMemorySessionStore::new());
        store.set(make_session("x"));

        let s = Arc::clone(&store);
        let handle = std::thread::spawn(move || {
            let _guard = s.inner.write().unwrap();
            panic!("intentional poison");
        });
        assert!(handle.join().is_err());

        // After poisoning, `clear` must still succeed.
        store.clear();
        assert!(store.get().is_none());
    }
}
