//! Identity layer: registered users, sessions (via tower-sessions extension),
//! OAuth (Google + GitHub), email verification, password reset, rate limiting,
//! seat-to-identity mapping.

pub mod error;
pub mod password;
pub mod users;
pub mod session_ext;
pub mod tokens;
pub mod game_seats;
pub mod mailer;
pub mod rate_limit;
pub mod oauth;

// pub use error::AuthError; // uncommented in Task 1.3
