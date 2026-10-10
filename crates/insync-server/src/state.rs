//! Shared application state: database, event bus and rate limit buckets.

use crate::db::Db;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::broadcast;

/// Per-relation event fan-out. Events are persisted first, then broadcast;
/// WebSocket subscribers receive frames from the moment they join.
#[derive(Default)]
pub struct EventBus {
    inner: Mutex<HashMap<String, broadcast::Sender<insync_core::EventFrame>>>,
}

impl EventBus {
    pub fn subscribe(&self, relation_id: &str) -> broadcast::Receiver<insync_core::EventFrame> {
        let mut map = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.entry(relation_id.to_string())
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }

    pub fn publish(&self, frame: &insync_core::EventFrame) {
        let map = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = map.get(&frame.relation_id) {
            // no subscribers is fine (the event is persisted for catch-up)
            let _ = sender.send(frame.clone());
        }
    }
}

/// Fixed-window rate limiter. Buckets are keyed by caller-provided strings
/// and reset when the window elapses.
#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, (i64, u32)>>,
}

impl RateLimiter {
    /// Returns `Err(retry_after_ms)` when over the limit.
    pub fn check(&self, key: &str, limit: u32, window_ms: i64, now: i64) -> Result<(), i64> {
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = buckets.entry(key.to_string()).or_insert((now, 0));
        if now - entry.0 >= window_ms {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 > limit {
            let retry_after = window_ms - (now - entry.0);
            Err(retry_after.max(1))
        } else {
            Ok(())
        }
    }

    /// Clears a bucket, e.g. after a successful login.
    pub fn clear(&self, key: &str) {
        self.buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub bus: std::sync::Arc<EventBus>,
    pub rate: std::sync::Arc<RateLimiter>,
    pub token_ttl: Duration,
}

impl AppState {
    pub fn new(db: Db, token_ttl: Duration) -> Result<Self, String> {
        Ok(Self {
            db,
            bus: std::sync::Arc::new(EventBus::default()),
            rate: std::sync::Arc::new(RateLimiter::default()),
            token_ttl,
        })
    }
}
