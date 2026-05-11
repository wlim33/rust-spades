//! Per-IP token-bucket via `governor` + per-account login lockout.

use governor::{Quota, RateLimiter};
use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

type IpLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;
type StringLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

pub struct RateLimitState {
    pub login: Arc<IpLimiter>,
    pub register: Arc<IpLimiter>,
    pub password_reset_request_ip: Arc<IpLimiter>,
    pub password_reset_request_email: Arc<StringLimiter>,
    pub password_reset_confirm: Arc<IpLimiter>,
    pub oauth_callback: Arc<IpLimiter>,
}

impl Default for RateLimitState {
    fn default() -> Self { Self::new() }
}

impl RateLimitState {
    pub fn new() -> Self {
        fn ip_lim(quota: Quota) -> Arc<IpLimiter> { Arc::new(RateLimiter::keyed(quota)) }
        fn s_lim(quota: Quota) -> Arc<StringLimiter> { Arc::new(RateLimiter::keyed(quota)) }
        RateLimitState {
            login: ip_lim(Quota::per_minute(NonZeroU32::new(10).unwrap()).allow_burst(NonZeroU32::new(60).unwrap())),
            register: ip_lim(Quota::per_minute(NonZeroU32::new(3).unwrap()).allow_burst(NonZeroU32::new(20).unwrap())),
            password_reset_request_ip: ip_lim(Quota::per_hour(NonZeroU32::new(3).unwrap())),
            password_reset_request_email: s_lim(Quota::per_minute(NonZeroU32::new(1).unwrap())),
            password_reset_confirm: ip_lim(Quota::per_hour(NonZeroU32::new(10).unwrap())),
            oauth_callback: ip_lim(Quota::per_minute(NonZeroU32::new(30).unwrap())),
        }
    }
}

pub fn check_ip(
    limiter: &governor::RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>,
    ip: IpAddr,
) -> Result<(), crate::auth::AuthError> {
    limiter.check_key(&ip).map_err(|nu| {
        let wait_secs = nu.wait_time_from(std::time::Instant::now()).as_secs().max(1);
        crate::auth::AuthError::RateLimited { retry_after_secs: wait_secs }
    })?;
    Ok(())
}

pub fn check_email(
    limiter: &governor::RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>,
    email: &str,
) -> Result<(), crate::auth::AuthError> {
    limiter.check_key(&email.to_string()).map_err(|nu| {
        let wait_secs = nu.wait_time_from(std::time::Instant::now()).as_secs().max(1);
        crate::auth::AuthError::RateLimited { retry_after_secs: wait_secs }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_state_constructible() {
        let _ = RateLimitState::new();
    }
}
