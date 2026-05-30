//! Per-IP token-bucket via `governor` + per-account login lockout.
//!
//! Auth routes are keyed by client IP. Game/match/challenge mutation routes
//! are keyed by the per-session `anon_id` (carried through login) so a single
//! household NAT doesn't share one bucket across four legitimate players.

use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use uuid::Uuid;

type IpLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;
type StringLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;
type UuidLimiter = RateLimiter<Uuid, DefaultKeyedStateStore<Uuid>, DefaultClock>;

pub struct RateLimitState {
    pub login: Arc<IpLimiter>,
    pub register: Arc<IpLimiter>,
    pub password_reset_request_ip: Arc<IpLimiter>,
    pub password_reset_request_email: Arc<StringLimiter>,
    pub password_reset_confirm: Arc<IpLimiter>,
    pub oauth_callback: Arc<IpLimiter>,
    /// POST /games — game creation, per-user.
    pub create_game: Arc<UuidLimiter>,
    /// POST /games/:id/transition — moves, per-user. Generous burst because
    /// legitimate fast-play (e.g., ending-of-round trick burst with multiple
    /// concurrent tables) can briefly exceed the sustained rate.
    pub transition: Arc<UuidLimiter>,
    /// POST /matchmaking/seek — open a seek, per-user.
    pub create_seek: Arc<UuidLimiter>,
    /// POST /challenges and POST /challenges/:id/join/:seat — challenge
    /// creation and join, per-user.
    pub challenge_action: Arc<UuidLimiter>,
    /// POST /games/:id/chat — in-game chat, per-user.
    pub chat_message: Arc<UuidLimiter>,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitState {
    pub fn new() -> Self {
        fn ip_lim(quota: Quota) -> Arc<IpLimiter> {
            Arc::new(RateLimiter::keyed(quota))
        }
        fn s_lim(quota: Quota) -> Arc<StringLimiter> {
            Arc::new(RateLimiter::keyed(quota))
        }
        fn u_lim(quota: Quota) -> Arc<UuidLimiter> {
            Arc::new(RateLimiter::keyed(quota))
        }
        RateLimitState {
            login: ip_lim(
                Quota::per_minute(NonZeroU32::new(10).unwrap())
                    .allow_burst(NonZeroU32::new(60).unwrap()),
            ),
            register: ip_lim(
                Quota::per_minute(NonZeroU32::new(3).unwrap())
                    .allow_burst(NonZeroU32::new(20).unwrap()),
            ),
            password_reset_request_ip: ip_lim(Quota::per_hour(NonZeroU32::new(3).unwrap())),
            password_reset_request_email: s_lim(Quota::per_minute(NonZeroU32::new(1).unwrap())),
            password_reset_confirm: ip_lim(Quota::per_hour(NonZeroU32::new(10).unwrap())),
            oauth_callback: ip_lim(Quota::per_minute(NonZeroU32::new(30).unwrap())),
            create_game: u_lim(
                Quota::per_minute(NonZeroU32::new(5).unwrap())
                    .allow_burst(NonZeroU32::new(10).unwrap()),
            ),
            transition: u_lim(
                Quota::per_minute(NonZeroU32::new(60).unwrap())
                    .allow_burst(NonZeroU32::new(120).unwrap()),
            ),
            create_seek: u_lim(
                Quota::per_minute(NonZeroU32::new(10).unwrap())
                    .allow_burst(NonZeroU32::new(20).unwrap()),
            ),
            challenge_action: u_lim(
                Quota::per_minute(NonZeroU32::new(10).unwrap())
                    .allow_burst(NonZeroU32::new(20).unwrap()),
            ),
            chat_message: u_lim(
                Quota::per_minute(NonZeroU32::new(30).unwrap())
                    .allow_burst(NonZeroU32::new(60).unwrap()),
            ),
        }
    }
}

pub fn check_ip(
    limiter: &governor::RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>,
    ip: IpAddr,
) -> Result<(), crate::auth::AuthError> {
    limiter.check_key(&ip).map_err(|nu| {
        let wait_secs = nu
            .wait_time_from(std::time::Instant::now())
            .as_secs()
            .max(1);
        crate::auth::AuthError::RateLimited {
            retry_after_secs: wait_secs,
        }
    })?;
    Ok(())
}

pub fn check_email(
    limiter: &governor::RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>,
    email: &str,
) -> Result<(), crate::auth::AuthError> {
    limiter.check_key(&email.to_string()).map_err(|nu| {
        let wait_secs = nu
            .wait_time_from(std::time::Instant::now())
            .as_secs()
            .max(1);
        crate::auth::AuthError::RateLimited {
            retry_after_secs: wait_secs,
        }
    })?;
    Ok(())
}

pub fn check_user(
    limiter: &governor::RateLimiter<Uuid, DefaultKeyedStateStore<Uuid>, DefaultClock>,
    user_id: Uuid,
) -> Result<(), crate::auth::AuthError> {
    limiter.check_key(&user_id).map_err(|nu| {
        let wait_secs = nu
            .wait_time_from(std::time::Instant::now())
            .as_secs()
            .max(1);
        crate::auth::AuthError::RateLimited {
            retry_after_secs: wait_secs,
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthError;

    #[test]
    fn rate_limit_state_constructible() {
        let _ = RateLimitState::new();
    }

    #[test]
    fn check_user_enforces_burst_then_rate_limits() {
        let state = RateLimitState::new();
        let uid = Uuid::new_v4();
        // create_game burst is 10 — first 10 succeed, 11th fails.
        for i in 0..10 {
            assert!(
                check_user(&state.create_game, uid).is_ok(),
                "call {i} should be within burst"
            );
        }
        let err =
            check_user(&state.create_game, uid).expect_err("11th call should be rate-limited");
        match err {
            AuthError::RateLimited { retry_after_secs } => {
                assert!(retry_after_secs >= 1, "retry_after should be at least 1s");
            }
            _ => panic!("expected RateLimited, got {err:?}"),
        }
        // Different user → fresh bucket.
        assert!(check_user(&state.create_game, Uuid::new_v4()).is_ok());
    }

    #[test]
    fn check_user_buckets_are_per_limiter() {
        let state = RateLimitState::new();
        let uid = Uuid::new_v4();
        // Exhaust create_game burst (10) — transition burst (120) is unaffected.
        for _ in 0..10 {
            let _ = check_user(&state.create_game, uid);
        }
        assert!(check_user(&state.create_game, uid).is_err());
        assert!(
            check_user(&state.transition, uid).is_ok(),
            "different limiter has its own bucket"
        );
    }
}
