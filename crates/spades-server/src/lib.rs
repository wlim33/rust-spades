//! Server-side modules for the Spades game: matchmaking, challenges, game manager,
//! SQLite persistence, name validation, and OpenAPI helpers.
//!
//! The `spades-server` binary in this crate wires these together with axum.

#![allow(clippy::collapsible_if, clippy::large_enum_variant)]

pub mod auth;
pub mod bands;
pub mod challenges;
pub mod game_actor;
pub mod game_manager;
pub mod handlers_auth;
pub mod handlers_leaderboard;
pub mod handlers_users;
pub mod leaderboard;
pub mod lock_util;
pub mod matchmaking;
pub mod oasgen_impls;
pub mod ratings;
pub mod sqlite_store;
pub mod validation;
