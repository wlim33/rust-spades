//! In-memory `Idempotency-Key` cache for state-mutating POSTs.
//!
//! When a client retries `POST /games/:id/transition` (e.g. after a network
//! blip), the underlying state mutation MUST NOT apply twice — playing a
//! card twice would either error (already played) or corrupt the trick
//! count. Clients that opt in by sending `Idempotency-Key: <uuid>` get the
//! first response replayed verbatim on retry for a short window.
//!
//! The cache is keyed by `(game_id, user_id, key)` so the same key string
//! from the same client on different games doesn't collide, and stores both
//! success and error outcomes (a client retrying after a 400 should see the
//! same 400, not a fresh attempt that might succeed under different state).
//!
//! Memory is bounded by the background sweeper in `main` calling
//! `sweep_expired`; entries are dropped after `TTL`.

use axum::http::StatusCode;
use spades_server::lock_util::MutexExt;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::dto::{ErrorResponse, TransitionResponse};

/// How long a cached outcome remains replayable. Network retry windows are
/// typically seconds, not minutes; 60s is generous enough to cover slow
/// mobile retries and tight enough to bound memory.
pub const TTL: Duration = Duration::from_secs(60);

/// What the cache replays on hit.
#[derive(Debug, Clone)]
pub enum CachedOutcome {
    Ok(TransitionResponse),
    Err(StatusCode, ErrorResponse),
}

#[derive(Debug, Clone)]
struct CacheEntry {
    stored_at: Instant,
    outcome: CachedOutcome,
}

#[derive(Debug, Default)]
pub struct IdempotencyCache {
    entries: Mutex<HashMap<(Uuid, Uuid, String), CacheEntry>>,
}

impl IdempotencyCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a cached outcome. Returns `None` if missing or if the entry
    /// has aged out — the caller should then run the operation fresh and
    /// `put` the result.
    pub fn get(&self, game_id: Uuid, user_id: Uuid, key: &str) -> Option<CachedOutcome> {
        let entries = self.entries.lock_or_recover();
        let entry = entries.get(&(game_id, user_id, key.to_string()))?;
        if entry.stored_at.elapsed() > TTL {
            return None;
        }
        Some(entry.outcome.clone())
    }

    /// Store an outcome under the key. Overwrites any prior entry.
    pub fn put(&self, game_id: Uuid, user_id: Uuid, key: String, outcome: CachedOutcome) {
        let mut entries = self.entries.lock_or_recover();
        entries.insert(
            (game_id, user_id, key),
            CacheEntry { stored_at: Instant::now(), outcome },
        );
    }

    /// Drop entries older than `TTL`. Returns the number removed.
    pub fn sweep_expired(&self) -> usize {
        let mut entries = self.entries.lock_or_recover();
        let before = entries.len();
        entries.retain(|_, e| e.stored_at.elapsed() <= TTL);
        before - entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(result: &str) -> CachedOutcome {
        CachedOutcome::Ok(TransitionResponse {
            success: true,
            result: result.to_string(),
        })
    }

    #[test]
    fn put_then_get_returns_same_outcome() {
        let cache = IdempotencyCache::new();
        let g = Uuid::new_v4();
        let u = Uuid::new_v4();
        cache.put(g, u, "k1".into(), ok("ok"));
        let out = cache.get(g, u, "k1").expect("hit");
        match out {
            CachedOutcome::Ok(resp) => assert_eq!(resp.result, "ok"),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn get_missing_returns_none() {
        let cache = IdempotencyCache::new();
        assert!(cache.get(Uuid::new_v4(), Uuid::new_v4(), "missing").is_none());
    }

    #[test]
    fn different_game_or_user_or_key_misses() {
        let cache = IdempotencyCache::new();
        let g = Uuid::new_v4();
        let u = Uuid::new_v4();
        cache.put(g, u, "k1".into(), ok("a"));
        // Same key on a different game is a fresh slot.
        assert!(cache.get(Uuid::new_v4(), u, "k1").is_none());
        // Same key from a different user is a fresh slot.
        assert!(cache.get(g, Uuid::new_v4(), "k1").is_none());
        // Different key from the same game+user is a fresh slot.
        assert!(cache.get(g, u, "other").is_none());
        // Original lookup still hits.
        assert!(cache.get(g, u, "k1").is_some());
    }

    #[test]
    fn err_outcome_replays_with_status_and_body() {
        let cache = IdempotencyCache::new();
        let g = Uuid::new_v4();
        let u = Uuid::new_v4();
        cache.put(
            g,
            u,
            "k".into(),
            CachedOutcome::Err(
                StatusCode::BAD_REQUEST,
                ErrorResponse { error: "boom".to_string() },
            ),
        );
        let out = cache.get(g, u, "k").expect("hit");
        match out {
            CachedOutcome::Err(s, body) => {
                assert_eq!(s, StatusCode::BAD_REQUEST);
                assert_eq!(body.error, "boom");
            }
            _ => panic!("expected Err"),
        }
    }

    #[test]
    fn sweep_drops_only_expired_entries() {
        let cache = IdempotencyCache::new();
        let g = Uuid::new_v4();
        let u = Uuid::new_v4();
        cache.put(g, u, "fresh".into(), ok("a"));

        // Backdate one entry past TTL by reaching into the mutex directly —
        // we own the test, and exposing a setter solely for tests would be
        // worse than the brief lock acquisition here.
        cache.put(g, u, "stale".into(), ok("b"));
        {
            let mut entries = cache.entries.lock_or_recover();
            let stale = entries
                .get_mut(&(g, u, "stale".to_string()))
                .expect("just inserted");
            stale.stored_at = Instant::now()
                .checked_sub(TTL + Duration::from_secs(1))
                .expect("can backdate before now");
        }

        assert_eq!(cache.sweep_expired(), 1);
        assert!(cache.get(g, u, "fresh").is_some());
        assert!(cache.get(g, u, "stale").is_none());
    }
}
