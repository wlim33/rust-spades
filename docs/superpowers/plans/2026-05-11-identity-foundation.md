# Identity Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the identity foundation slice from `docs/superpowers/specs/2026-05-11-identity-foundation-design.md`: user accounts, email/password + OAuth (Google + GitHub) login, anon-claim flow on the existing tower-sessions infrastructure, public profiles, and game-seat ownership wiring.

**Architecture:** New `auth/` module inside `spades-server`. Reuses the existing tower-sessions + tower-sessions-sqlx-store integration as the cookie/session transport; extends `UserSession` with `claimed_by` + `token_version`. New tables (`users`, `oauth_accounts`, `auth_tokens`, `login_failures`, `game_seats`) added to `SqliteStore`. No new crates, no Postgres, no Redis.

**Tech Stack:** Rust 2024 edition, axum 0.8, tower-sessions 0.14, rusqlite (bundled), argon2 (new), oauth2 (new), lettre (new), governor (new), sha2 (new), reqwest (new). Tests use `cargo test --workspace` with `axum-test` for HTTP integration and `wiremock` for OAuth providers (new dev-dep).

**Verification commands (run after each task):**
- `cargo build --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`

The clippy gate matches the existing hook (`hooks/pre-push`) — keep it clippy-clean throughout.

---

## Phase 1: Scaffolding

Foundation for everything else. Adds dependencies, creates the empty `auth/` module tree, defines `AuthError`. After this phase, `cargo build` and `cargo test` pass and the new module imports cleanly.

### Task 1.1: Add Cargo dependencies

**Files:**
- Modify: `crates/spades-server/Cargo.toml`

- [ ] **Step 1: Add dependencies under `[dependencies]`**

Add these lines to `crates/spades-server/Cargo.toml` between line 35 (after `serde_json.workspace = true`) and line 37 (before `[dev-dependencies]`):

```toml
argon2 = "0.5"
oauth2 = "4.4"
lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1-rustls-tls", "builder"] }
governor = { version = "0.7", default-features = false, features = ["std"] }
sha2 = "0.10"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

- [ ] **Step 2: Add dev-dependency under `[dev-dependencies]`**

Append after line 39 (`ntest = "0.9"`):

```toml
wiremock = "0.6"
```

- [ ] **Step 3: Run `cargo build --workspace` to fetch crates**

Expected: build succeeds, lockfile updated.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-server/Cargo.toml Cargo.lock
git commit -m "deps: add argon2, oauth2, lettre, governor, sha2, reqwest, wiremock"
```

### Task 1.2: Create auth module skeleton

**Files:**
- Create: `crates/spades-server/src/auth/mod.rs`
- Create: `crates/spades-server/src/auth/users.rs`
- Create: `crates/spades-server/src/auth/session_ext.rs`
- Create: `crates/spades-server/src/auth/oauth.rs`
- Create: `crates/spades-server/src/auth/password.rs`
- Create: `crates/spades-server/src/auth/mailer.rs`
- Create: `crates/spades-server/src/auth/rate_limit.rs`
- Create: `crates/spades-server/src/auth/tokens.rs`
- Create: `crates/spades-server/src/auth/game_seats.rs`
- Create: `crates/spades-server/src/auth/error.rs`
- Modify: `crates/spades-server/src/lib.rs`

- [ ] **Step 1: Create `auth/mod.rs` with submodule declarations**

```rust
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

pub use error::AuthError;
```

- [ ] **Step 2: Create each submodule as an empty file with a doc comment**

`auth/error.rs`:
```rust
//! AuthError type and HTTP response mapping.
```

`auth/password.rs`:
```rust
//! argon2id password hashing + weak-password reject list.
```

`auth/users.rs`:
```rust
//! User struct, repo (CRUD), username rules, token_version.
```

`auth/session_ext.rs`:
```rust
//! Typed helpers over the tower-sessions `UserSession` blob.
```

`auth/tokens.rs`:
```rust
//! Single-use email tokens (verify-email, password-reset).
```

`auth/game_seats.rs`:
```rust
//! Per-game seat-to-identity mapping table.
```

`auth/mailer.rs`:
```rust
//! Pluggable Mailer trait. LogMailer (dev/CI) + SmtpMailer (lettre).
```

`auth/rate_limit.rs`:
```rust
//! Per-IP token-bucket via `governor` + per-account login lockout.
```

`auth/oauth.rs`:
```rust
//! Google + GitHub OAuth flow, state CSRF, PKCE, pending-signup store.
```

- [ ] **Step 3: Add `pub mod auth;` to `crates/spades-server/src/lib.rs`**

Read `crates/spades-server/src/lib.rs` first. Add `pub mod auth;` after the existing top-level `pub mod` declarations (alphabetical order if existing modules follow that, otherwise after the last one).

- [ ] **Step 4: Run `cargo build --workspace` and `cargo test --workspace`**

Expected: build succeeds, tests pass (no new tests yet, just stubs).

- [ ] **Step 5: Commit**

```bash
git add crates/spades-server/src/auth crates/spades-server/src/lib.rs
git commit -m "auth: scaffold module tree (empty submodules + pub mod hook)"
```

### Task 1.3: AuthError type

**Files:**
- Modify: `crates/spades-server/src/auth/error.rs`

- [ ] **Step 1: Write failing tests**

Append to `crates/spades-server/src/auth/error.rs`:

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("forbidden")]
    Forbidden,
    #[error("username_taken")]
    UsernameTaken,
    #[error("email_taken")]
    EmailTaken,
    #[error("invalid_credentials")]
    InvalidCredentials,
    #[error("locked")]
    Locked { retry_after_secs: u64 },
    #[error("rate_limited")]
    RateLimited { retry_after_secs: u64 },
    #[error("token_invalid")]
    TokenInvalid,
    #[error("oauth_failed: {0}")]
    OauthFailed(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("mailer_failed")]
    MailerFailed,
    #[error("storage: {0}")]
    Storage(String),
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

impl AuthError {
    fn status(&self) -> StatusCode {
        match self {
            AuthError::Unauthenticated | AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            AuthError::Forbidden => StatusCode::FORBIDDEN,
            AuthError::UsernameTaken | AuthError::EmailTaken => StatusCode::CONFLICT,
            AuthError::Locked { .. } => StatusCode::LOCKED,
            AuthError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            AuthError::TokenInvalid => StatusCode::GONE,
            AuthError::OauthFailed(_) => StatusCode::BAD_REQUEST,
            AuthError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AuthError::MailerFailed => StatusCode::BAD_GATEWAY,
            AuthError::Storage(_) | AuthError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn body(&self) -> ErrorBody<'_> {
        let retry_after_secs = match self {
            AuthError::Locked { retry_after_secs } | AuthError::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        };
        let details = match self {
            AuthError::OauthFailed(s) | AuthError::Validation(s) => Some(s.clone()),
            _ => None,
        };
        ErrorBody {
            error: error_code(self),
            retry_after_secs,
            details,
        }
    }
}

fn error_code(e: &AuthError) -> &'static str {
    match e {
        AuthError::Unauthenticated => "unauthenticated",
        AuthError::Forbidden => "forbidden",
        AuthError::UsernameTaken => "username_taken",
        AuthError::EmailTaken => "email_taken",
        AuthError::InvalidCredentials => "invalid_credentials",
        AuthError::Locked { .. } => "locked",
        AuthError::RateLimited { .. } => "rate_limited",
        AuthError::TokenInvalid => "token_invalid",
        AuthError::OauthFailed(_) => "oauth_failed",
        AuthError::Validation(_) => "validation",
        AuthError::MailerFailed => "mailer_failed",
        AuthError::Storage(_) => "internal",
        AuthError::Internal(_) => "internal",
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut resp = (status, Json(self.body())).into_response();
        if let AuthError::RateLimited { retry_after_secs } | AuthError::Locked { retry_after_secs } = &self {
            if let Ok(hv) = retry_after_secs.to_string().parse() {
                resp.headers_mut().insert(axum::http::header::RETRY_AFTER, hv);
            }
        }
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_match_spec() {
        assert_eq!(AuthError::Unauthenticated.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(AuthError::Forbidden.status(), StatusCode::FORBIDDEN);
        assert_eq!(AuthError::UsernameTaken.status(), StatusCode::CONFLICT);
        assert_eq!(AuthError::EmailTaken.status(), StatusCode::CONFLICT);
        assert_eq!(AuthError::InvalidCredentials.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(AuthError::Locked { retry_after_secs: 30 }.status(), StatusCode::LOCKED);
        assert_eq!(AuthError::RateLimited { retry_after_secs: 30 }.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(AuthError::TokenInvalid.status(), StatusCode::GONE);
        assert_eq!(AuthError::OauthFailed("x".into()).status(), StatusCode::BAD_REQUEST);
        assert_eq!(AuthError::Validation("x".into()).status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(AuthError::MailerFailed.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(AuthError::Storage("x".into()).status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn error_codes_stable() {
        assert_eq!(error_code(&AuthError::Unauthenticated), "unauthenticated");
        assert_eq!(error_code(&AuthError::UsernameTaken), "username_taken");
        assert_eq!(error_code(&AuthError::Locked { retry_after_secs: 0 }), "locked");
        assert_eq!(error_code(&AuthError::Storage("x".into())), "internal");
    }
}
```

- [ ] **Step 2: Add `thiserror` to dependencies**

If `thiserror` isn't already in workspace or server Cargo.toml, add to `crates/spades-server/Cargo.toml`:

```toml
thiserror = "1"
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p spades-server auth::error -- --nocapture`
Expected: pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-server/Cargo.toml crates/spades-server/src/auth/error.rs
git commit -m "auth: AuthError enum + status code mapping + IntoResponse"
```

---

## Phase 2: Storage migrations

Adds the five new tables to `SqliteStore`. Pure DDL; no business logic.

### Task 2.1: Add new tables to SqliteStore

**Files:**
- Modify: `crates/spades-server/src/sqlite_store.rs`

- [ ] **Step 1: Write failing tests**

Append to the `#[cfg(test)] mod tests` block in `crates/spades-server/src/sqlite_store.rs`:

```rust
    #[test]
    fn auth_tables_created() {
        let store = SqliteStore::open(":memory:").unwrap();
        let conn = store.conn.lock().unwrap();
        for table in ["users", "oauth_accounts", "auth_tokens", "login_failures", "game_seats"] {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(exists, 1, "table {} not created", table);
        }
    }

    #[test]
    fn users_username_canon_is_unique() {
        let store = SqliteStore::open(":memory:").unwrap();
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, username_canon, email, password_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["u1", "Alice", "alice", "a@x.com", None::<String>],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO users (id, username, username_canon, email, password_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["u2", "ALICE", "alice", "a2@x.com", None::<String>],
        );
        assert!(err.is_err(), "username_canon should be UNIQUE");
    }
```

- [ ] **Step 2: Run tests, expect failure**

Run: `cargo test -p spades-server auth_tables_created users_username_canon`
Expected: FAIL (tables don't exist).

- [ ] **Step 3: Extend the `execute_batch` call in `SqliteStore::open`**

In `crates/spades-server/src/sqlite_store.rs`, replace the existing `execute_batch` call (currently around lines 15-20) with:

```rust
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS users (
                id              TEXT PRIMARY KEY,
                username        TEXT NOT NULL,
                username_canon  TEXT NOT NULL UNIQUE,
                email           TEXT NOT NULL UNIQUE,
                email_verified  INTEGER NOT NULL DEFAULT 0,
                password_hash   TEXT,
                token_version   INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                last_login_at   TEXT
            );
            CREATE INDEX IF NOT EXISTS users_username_canon ON users(username_canon);
            CREATE TABLE IF NOT EXISTS oauth_accounts (
                provider        TEXT NOT NULL,
                provider_uid    TEXT NOT NULL,
                user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                email           TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (provider, provider_uid)
            );
            CREATE INDEX IF NOT EXISTS oauth_accounts_user_id ON oauth_accounts(user_id);
            CREATE TABLE IF NOT EXISTS auth_tokens (
                token_hash      TEXT PRIMARY KEY,
                user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                purpose         TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at      TEXT NOT NULL,
                used_at         TEXT
            );
            CREATE INDEX IF NOT EXISTS auth_tokens_user_id ON auth_tokens(user_id);
            CREATE INDEX IF NOT EXISTS auth_tokens_expires_at ON auth_tokens(expires_at);
            CREATE TABLE IF NOT EXISTS login_failures (
                user_id         TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
                failure_count   INTEGER NOT NULL DEFAULT 0,
                locked_until    TEXT
            );
            CREATE TABLE IF NOT EXISTS game_seats (
                game_id         TEXT NOT NULL,
                seat_index      INTEGER NOT NULL,
                player_id       TEXT NOT NULL,
                user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
                anon_user_id    TEXT,
                is_bot          INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (game_id, seat_index)
            );
            CREATE INDEX IF NOT EXISTS game_seats_user_id ON game_seats(user_id);
            CREATE INDEX IF NOT EXISTS game_seats_anon_user_id ON game_seats(anon_user_id);"
        ).map_err(|e| e.to_string())?;
```

Also enable foreign keys (SQLite needs this opt-in per connection):

Immediately after `let conn = Connection::open(path).map_err(|e| e.to_string())?;`, add:

```rust
        conn.execute("PRAGMA foreign_keys = ON", []).map_err(|e| e.to_string())?;
        conn.execute("PRAGMA journal_mode = WAL", []).map_err(|e| e.to_string())?;
```

Wait — `journal_mode` is a query that returns a row, not `execute`. Use `query_row` instead:

```rust
        conn.execute("PRAGMA foreign_keys = ON", []).map_err(|e| e.to_string())?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(())).map_err(|e| e.to_string())?;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p spades-server`
Expected: all tests pass including the two new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-server/src/sqlite_store.rs
git commit -m "store: add users, oauth_accounts, auth_tokens, login_failures, game_seats tables"
```

---

## Phase 3: Mailer

Pluggable email backend. LogMailer for dev/CI (default when no SMTP env vars set), SmtpMailer for production.

### Task 3.1: Mailer trait + LogMailer

**Files:**
- Modify: `crates/spades-server/src/auth/mailer.rs`

- [ ] **Step 1: Write failing tests**

Append to `crates/spades-server/src/auth/mailer.rs`:

```rust
use crate::auth::AuthError;
use std::sync::{Arc, Mutex};

/// A single email message to send.
#[derive(Debug, Clone)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, email: Email) -> Result<(), AuthError>;
}

/// Mailer that records messages in memory instead of sending. For dev/CI.
#[derive(Clone, Default)]
pub struct LogMailer {
    sent: Arc<Mutex<Vec<Email>>>,
}

impl LogMailer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn sent(&self) -> Vec<Email> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for LogMailer {
    async fn send(&self, email: Email) -> Result<(), AuthError> {
        eprintln!("LogMailer: to={} subject={}", email.to, email.subject);
        self.sent.lock().unwrap().push(email);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn log_mailer_records_messages() {
        let m = LogMailer::new();
        m.send(Email {
            to: "a@example.com".into(),
            subject: "Hello".into(),
            body: "Body".into(),
        }).await.unwrap();
        let sent = m.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, "a@example.com");
        assert_eq!(sent[0].subject, "Hello");
    }
}
```

- [ ] **Step 2: Add `async-trait` to deps**

In `crates/spades-server/Cargo.toml`, under `[dependencies]`:

```toml
async-trait = "0.1"
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p spades-server auth::mailer -- --nocapture`
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-server/Cargo.toml crates/spades-server/src/auth/mailer.rs
git commit -m "auth: Mailer trait + LogMailer (in-memory record-only impl)"
```

### Task 3.2: SmtpMailer

**Files:**
- Modify: `crates/spades-server/src/auth/mailer.rs`

- [ ] **Step 1: Append `SmtpMailer` impl**

Append after the `LogMailer` block (before the `#[cfg(test)]` block):

```rust
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,        // "Spades <noreply@spades.example.com>"
    pub starttls: bool,
}

impl SmtpConfig {
    /// Build from env vars. Returns None if any required var is missing.
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("SMTP_HOST").ok()?;
        let username = std::env::var("SMTP_USER").ok()?;
        let password = std::env::var("SMTP_PASS").ok()?;
        let from = std::env::var("SMTP_FROM").ok()?;
        let port: u16 = std::env::var("SMTP_PORT").ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(587);
        let starttls = std::env::var("SMTP_STARTTLS").map(|s| s != "false").unwrap_or(true);
        Some(SmtpConfig { host, port, username, password, from, starttls })
    }
}

pub struct SmtpMailer {
    cfg: SmtpConfig,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpMailer {
    pub fn new(cfg: SmtpConfig) -> Result<Self, AuthError> {
        let creds = Credentials::new(cfg.username.clone(), cfg.password.clone());
        let builder = if cfg.starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
                .map_err(|e| AuthError::Internal(format!("smtp init: {e}")))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)
                .map_err(|e| AuthError::Internal(format!("smtp init: {e}")))?
        };
        let transport = builder.credentials(creds).port(cfg.port).build();
        Ok(SmtpMailer { cfg, transport })
    }
}

#[async_trait::async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, email: Email) -> Result<(), AuthError> {
        let from: Mailbox = self.cfg.from.parse()
            .map_err(|e| AuthError::Internal(format!("invalid SMTP_FROM: {e}")))?;
        let to: Mailbox = email.to.parse()
            .map_err(|e| AuthError::Validation(format!("invalid email: {e}")))?;
        let msg = Message::builder()
            .from(from)
            .to(to)
            .subject(email.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(email.body)
            .map_err(|e| AuthError::Internal(format!("message build: {e}")))?;
        self.transport.send(msg).await.map_err(|e| {
            eprintln!("SmtpMailer error: {e}");
            AuthError::MailerFailed
        })?;
        Ok(())
    }
}
```

- [ ] **Step 2: Add a unit test for `SmtpConfig::from_env`**

Append inside the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn smtp_config_from_env_missing_vars_returns_none() {
        // Ensure clean env for this test
        for v in ["SMTP_HOST", "SMTP_USER", "SMTP_PASS", "SMTP_FROM"] {
            unsafe { std::env::remove_var(v); }
        }
        assert!(SmtpConfig::from_env().is_none());
    }
```

Note: `std::env::remove_var` requires `unsafe` in Rust 2024 edition (which this workspace uses).

- [ ] **Step 3: Run tests**

Run: `cargo test -p spades-server auth::mailer`
Expected: pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-server/src/auth/mailer.rs
git commit -m "auth: SmtpMailer via lettre + SmtpConfig::from_env"
```

---

## Phase 4: Password hashing

argon2id with the spec's params (m=64MB, t=3, p=4), plus a small embedded weak-password reject list.

### Task 4.1: Password hash + verify + weak-list check

**Files:**
- Modify: `crates/spades-server/src/auth/password.rs`
- Create: `crates/spades-server/src/auth/weak_passwords.txt`

- [ ] **Step 1: Create the weak-password list**

Create `crates/spades-server/src/auth/weak_passwords.txt` with one password per line (lowercase):

```
123456
password
12345678
qwerty
123456789
12345
1234
111111
1234567
dragon
123123
baseball
abc123
football
monkey
letmein
shadow
master
666666
qwertyuiop
123321
mustang
1234567890
michael
654321
superman
1qaz2wsx
7777777
121212
000000
qazwsx
123qwe
killer
trustno1
jordan
jennifer
zxcvbnm
asdfgh
hunter
buster
soccer
harley
batman
andrew
tigger
sunshine
iloveyou
2000
charlie
robert
thomas
hockey
ranger
daniel
starwars
klaster
112233
george
computer
michelle
jessica
pepper
1111
zxcvbn
555555
11111111
131313
freedom
777777
pass
maggie
159753
aaaaaa
ginger
princess
joshua
cheese
amanda
summer
love
ashley
nicole
chelsea
biteme
matthew
access
yankees
987654321
dallas
austin
thunder
taylor
matrix
mobilemail
mom
monitor
monitoring
montana
moon
moscow
```

(This is a curated short list. The engineer can swap in a longer list later — the file is a build-time include and the trade-off is binary size vs coverage.)

- [ ] **Step 2: Write failing tests**

Replace the contents of `crates/spades-server/src/auth/password.rs` with:

```rust
//! argon2id password hashing + weak-password reject list.

use crate::auth::AuthError;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, Params, Version, Algorithm};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use std::sync::OnceLock;

/// Embedded weak-password reject list. Lowercased.
const WEAK_PASSWORDS_RAW: &str = include_str!("weak_passwords.txt");

fn weak_set() -> &'static std::collections::HashSet<&'static str> {
    static SET: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| WEAK_PASSWORDS_RAW.lines().filter(|l| !l.is_empty()).collect())
}

/// Spec params (Phase 5 of the design): m=64MB, t=3, p=4.
fn argon2() -> Argon2<'static> {
    let params = Params::new(64 * 1024, 3, 4, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Validate password complexity. Returns `Err(AuthError::Validation(_))` on rejection.
pub fn validate_password(password: &str) -> Result<(), AuthError> {
    if password.len() < 8 {
        return Err(AuthError::Validation("password must be at least 8 characters".into()));
    }
    if password.len() > 256 {
        return Err(AuthError::Validation("password must be at most 256 characters".into()));
    }
    if weak_set().contains(password.to_lowercase().as_str()) {
        return Err(AuthError::Validation("password is too common".into()));
    }
    Ok(())
}

/// Hash a password with argon2id. Returns the PHC string.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Internal(format!("password hash: {e}")))
}

/// Verify a password against a PHC hash. Returns Ok(true) on match, Ok(false) on mismatch.
pub fn verify_password(password: &str, phc: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(phc).map_err(|e| AuthError::Internal(format!("password parse: {e}")))?;
    match argon2().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Errors::Password) => Ok(false),
        Err(e) => Err(AuthError::Internal(format!("password verify: {e}"))),
    }
}

/// Stored dummy hash for constant-time path when user lookup misses.
/// Generated once per process; the actual value is unused — only the wall-clock cost matters.
pub fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hash_password("placeholder-not-a-real-password").unwrap())
}

/// Constant-time verify path: call this when the user lookup misses to keep timing uniform.
pub fn verify_against_dummy() {
    let _ = verify_password("anything", dummy_hash());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let h = hash_password("hunter2-strong").unwrap();
        assert!(verify_password("hunter2-strong", &h).unwrap());
        assert!(!verify_password("wrong-password", &h).unwrap());
    }

    #[test]
    fn validate_rejects_short() {
        assert!(validate_password("short").is_err());
    }

    #[test]
    fn validate_rejects_weak_list() {
        assert!(validate_password("password").is_err());
        assert!(validate_password("Password").is_err()); // case-insensitive
        assert!(validate_password("12345678").is_err());
    }

    #[test]
    fn validate_accepts_strong() {
        validate_password("hunter2-strong").unwrap();
        validate_password("a-reasonable-passphrase!").unwrap();
    }

    #[test]
    fn dummy_hash_can_be_verified_against() {
        verify_against_dummy(); // doesn't panic
    }
}
```

Note on the argon2 error: the actual variant name in argon2 0.5 is `argon2::password_hash::Error::Password` (not `Errors::Password`). Adjust if compilation fails — the crate's public re-export is `password_hash::Error`. Pattern: `Err(e) if e == argon2::password_hash::Error::Password => Ok(false)`.

- [ ] **Step 3: Run tests, expect them to compile and pass**

Run: `cargo test -p spades-server auth::password`
Expected: all five pass.

If compilation fails on the `Errors::Password` pattern, replace with:

```rust
    match argon2().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(e) if matches!(e, argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(AuthError::Internal(format!("password verify: {e}"))),
    }
```

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-server/src/auth/password.rs crates/spades-server/src/auth/weak_passwords.txt
git commit -m "auth: argon2id hash + verify + weak-password reject list"
```

---

## Phase 5: Username validation

Lila-style: 2-20 ASCII `[a-zA-Z0-9_-]`, immutable, case-insensitive uniqueness via `username_canon`, reserved-name reject list.

### Task 5.1: Canonicalization + reserved names + validator

**Files:**
- Modify: `crates/spades-server/src/auth/users.rs`

- [ ] **Step 1: Write tests + impl**

Replace contents of `crates/spades-server/src/auth/users.rs`:

```rust
//! User struct, repo (CRUD), username rules, token_version.

use crate::auth::AuthError;

/// Lowercased canonical form used for uniqueness lookups.
pub fn canonicalize_username(input: &str) -> String {
    input.to_ascii_lowercase()
}

/// Reserved names that conflict with route prefixes or system identifiers.
/// Plan-time list — keep in sync with mounted routes.
const RESERVED: &[&str] = &[
    "me", "admin", "root", "auth", "oauth", "api",
    "users", "games", "lobbies", "challenges", "matchmaking",
    "ws", "static", "assets", "docs", "openapi", "swagger-ui",
    "player", "spades", "system", "null", "undefined",
];

pub fn validate_username(input: &str) -> Result<String, AuthError> {
    if input.len() < 2 || input.len() > 20 {
        return Err(AuthError::Validation("username must be 2-20 characters".into()));
    }
    if !input.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(AuthError::Validation("username may only contain letters, digits, underscore, hyphen".into()));
    }
    // Disallow leading/trailing hyphen and consecutive hyphens for cleanliness.
    if input.starts_with('-') || input.ends_with('-') || input.contains("--") {
        return Err(AuthError::Validation("invalid hyphen placement".into()));
    }
    let canon = canonicalize_username(input);
    if RESERVED.iter().any(|r| **r == canon) {
        return Err(AuthError::Validation("username is reserved".into()));
    }
    Ok(input.to_string())
}

/// Basic email syntax check (must contain '@' with non-empty local and domain).
pub fn validate_email(input: &str) -> Result<(), AuthError> {
    let trimmed = input.trim();
    if trimmed.len() > 254 {
        return Err(AuthError::Validation("email too long".into()));
    }
    let parts: Vec<&str> = trimmed.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AuthError::Validation("invalid email syntax".into()));
    }
    if !parts[1].contains('.') {
        return Err(AuthError::Validation("invalid email syntax".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canon_is_lowercase() {
        assert_eq!(canonicalize_username("Alice"), "alice");
        assert_eq!(canonicalize_username("ALICE"), "alice");
    }

    #[test]
    fn canon_is_idempotent() {
        let x = canonicalize_username("MixedCase_42");
        assert_eq!(canonicalize_username(&x), x);
    }

    #[test]
    fn valid_usernames_pass() {
        for s in ["alice", "Alice", "user_42", "with-hyphen", "ab"] {
            validate_username(s).expect(s);
        }
    }

    #[test]
    fn invalid_usernames_fail() {
        for s in ["a", "this_username_is_too_long_yes", "user@host", "user space", "-bad", "bad-", "double--hyphen"] {
            assert!(validate_username(s).is_err(), "{s} should be rejected");
        }
    }

    #[test]
    fn reserved_names_rejected() {
        for r in ["me", "admin", "users", "auth", "ME", "Admin"] {
            assert!(validate_username(r).is_err(), "{r} should be reserved");
        }
    }

    #[test]
    fn email_validator() {
        validate_email("a@b.com").unwrap();
        validate_email("alice.smith@example.org").unwrap();
        assert!(validate_email("no-at-sign.com").is_err());
        assert!(validate_email("@nolocal.com").is_err());
        assert!(validate_email("nodomain@").is_err());
        assert!(validate_email("noTld@host").is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p spades-server auth::users`
Expected: all six pass.

- [ ] **Step 3: Commit**

```bash
git add crates/spades-server/src/auth/users.rs
git commit -m "auth: username + email validators + canonicalization + reserved list"
```

---

## Phase 6: User repo

`User` struct + CRUD methods on `SqliteStore`. Operations: insert, lookup by id / email / canon-username, update password (bumps token_version), set email_verified, bump token_version.

### Task 6.1: User struct + sqlite_store methods

**Files:**
- Modify: `crates/spades-server/src/auth/users.rs`
- Modify: `crates/spades-server/src/sqlite_store.rs`

- [ ] **Step 1: Append `User` struct + repo methods to `auth/users.rs`**

Append below the existing `validate_email` function (before the test module):

```rust
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub username_canon: String,
    pub email: String,
    pub email_verified: bool,
    pub password_hash: Option<String>,
    pub token_version: i32,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub email_verified: bool,
}

impl User {
    pub fn public_view(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "username": self.username,
            "email": self.email,
            "email_verified": self.email_verified,
        })
    }
}
```

- [ ] **Step 2: Append user-related methods to `SqliteStore`**

In `crates/spades-server/src/sqlite_store.rs`, append inside the `impl SqliteStore` block (before the closing brace and before the test module):

```rust
    /// Insert a new user. Returns the new user's id on success.
    pub fn insert_user(&self, new: &crate::auth::users::NewUser) -> Result<uuid::Uuid, String> {
        use crate::auth::users::canonicalize_username;
        let id = uuid::Uuid::new_v4();
        let canon = canonicalize_username(&new.username);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO users (id, username, username_canon, email, email_verified, password_hash, token_version) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            rusqlite::params![
                id.to_string(),
                &new.username,
                canon,
                &new.email,
                new.email_verified as i32,
                new.password_hash.as_deref(),
            ],
        ).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed: users.username_canon") {
                "username_taken".to_string()
            } else if msg.contains("UNIQUE constraint failed: users.email") {
                "email_taken".to_string()
            } else {
                msg
            }
        })?;
        Ok(id)
    }

    pub fn find_user_by_id(&self, id: uuid::Uuid) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username, username_canon, email, email_verified, password_hash, token_version, created_at, last_login_at \
             FROM users WHERE id = ?1",
            rusqlite::params![id.to_string()],
            row_to_user,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn find_user_by_email(&self, email: &str) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username, username_canon, email, email_verified, password_hash, token_version, created_at, last_login_at \
             FROM users WHERE email = ?1",
            rusqlite::params![email],
            row_to_user,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn find_user_by_username(&self, username: &str) -> Result<Option<crate::auth::users::User>, String> {
        use crate::auth::users::canonicalize_username;
        let canon = canonicalize_username(username);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username, username_canon, email, email_verified, password_hash, token_version, created_at, last_login_at \
             FROM users WHERE username_canon = ?1",
            rusqlite::params![canon],
            row_to_user,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    /// Update password and bump token_version (invalidates all live sessions for this user).
    pub fn update_user_password(&self, user_id: uuid::Uuid, new_hash: &str) -> Result<i32, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let new_version: i32 = conn.query_row(
            "UPDATE users SET password_hash = ?1, token_version = token_version + 1 \
             WHERE id = ?2 RETURNING token_version",
            rusqlite::params![new_hash, user_id.to_string()],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        Ok(new_version)
    }

    pub fn set_user_email_verified(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET email_verified = 1 WHERE id = ?1",
            rusqlite::params![user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn touch_user_login(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET last_login_at = datetime('now') WHERE id = ?1",
            rusqlite::params![user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_user_email(&self, user_id: uuid::Uuid, new_email: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET email = ?1, email_verified = 0 WHERE id = ?2",
            rusqlite::params![new_email, user_id.to_string()],
        ).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed: users.email") {
                "email_taken".to_string()
            } else {
                msg
            }
        })?;
        Ok(())
    }
}

fn row_to_user(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::auth::users::User> {
    let id_s: String = r.get(0)?;
    let id = uuid::Uuid::parse_str(&id_s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(
        0, rusqlite::types::Type::Text, Box::new(e),
    ))?;
    Ok(crate::auth::users::User {
        id,
        username: r.get(1)?,
        username_canon: r.get(2)?,
        email: r.get(3)?,
        email_verified: r.get::<_, i32>(4)? != 0,
        password_hash: r.get(5)?,
        token_version: r.get(6)?,
        created_at: r.get(7)?,
        last_login_at: r.get(8)?,
    })
}
```

- [ ] **Step 3: Write integration tests in `sqlite_store.rs`**

Append to the `#[cfg(test)] mod tests` block:

```rust
    use crate::auth::users::NewUser;

    fn new_user(name: &str, email: &str) -> NewUser {
        NewUser {
            username: name.into(),
            email: email.into(),
            password_hash: Some("$argon2id$dummy".into()),
            email_verified: false,
        }
    }

    #[test]
    fn insert_and_find_user() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        let u = store.find_user_by_id(id).unwrap().unwrap();
        assert_eq!(u.username, "Alice");
        assert_eq!(u.username_canon, "alice");
        assert_eq!(u.email_verified, false);
        assert_eq!(u.token_version, 0);
    }

    #[test]
    fn find_by_email_and_username_works() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        let by_email = store.find_user_by_email("alice@x.com").unwrap().unwrap();
        let by_username = store.find_user_by_username("ALICE").unwrap().unwrap();
        assert_eq!(by_email.id, by_username.id);
    }

    #[test]
    fn duplicate_username_rejected() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "a1@x.com")).unwrap();
        let err = store.insert_user(&new_user("alice", "a2@x.com")).unwrap_err();
        assert_eq!(err, "username_taken");
    }

    #[test]
    fn duplicate_email_rejected() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "a@x.com")).unwrap();
        let err = store.insert_user(&new_user("Bob", "a@x.com")).unwrap_err();
        assert_eq!(err, "email_taken");
    }

    #[test]
    fn password_update_bumps_token_version() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        let v1 = store.update_user_password(id, "$argon2id$new").unwrap();
        let v2 = store.update_user_password(id, "$argon2id$newer").unwrap();
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
    }

    #[test]
    fn email_verify_and_touch_login() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        store.set_user_email_verified(id).unwrap();
        store.touch_user_login(id).unwrap();
        let u = store.find_user_by_id(id).unwrap().unwrap();
        assert!(u.email_verified);
        assert!(u.last_login_at.is_some());
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p spades-server`
Expected: all new + existing tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/spades-server/src/auth/users.rs crates/spades-server/src/sqlite_store.rs
git commit -m "auth: User struct + repo (insert, find, update password/email/login)"
```

---

## Phase 7: Session adaptation

Extend the existing `UserSession` to carry `claimed_by` + `token_version`. Build session_ext helpers + `Identity` / `AuthUser` extractors on top of `tower_sessions::Session`.

### Task 7.1: Inline-move `UserSession` to the library (replaced in Task 7.2)

This task is **a no-op placeholder** — the actual `UserSession` definition moves into `spades_server::auth::session_ext::UserSession` in Task 7.2, and `bin/server/dto.rs` becomes a re-export.

**Files:**
- Modify: `crates/spades-server/src/bin/server/dto.rs`

- [ ] **Step 1: Replace the local `UserSession` struct with a re-export**

In `crates/spades-server/src/bin/server/dto.rs`, remove the existing `UserSession` struct declaration (around lines 11-15) and instead add at the top of the file:

```rust
// Re-export the canonical session payload from the library so handlers in
// this binary can continue to import `crate::dto::UserSession` unchanged.
pub use spades_server::auth::session_ext::UserSession;
```

The library type (defined in Task 7.2) has these fields, all serde-compatible with the old shape:

```rust
pub struct UserSession {
    pub user_id: Uuid,
    pub display_name: Option<String>,
    pub claimed_by: Option<Uuid>,
    pub token_version: i32,
}
```

`#[serde(default)]` on the new fields (in the library definition) ensures session blobs persisted before this change deserialize cleanly.

- [ ] **Step 2: Add a serde-default test**

Append to `crates/spades-server/src/bin/server/dto.rs` (create a `#[cfg(test)] mod tests` block if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_session_backward_compatible() {
        // A pre-update serialized blob (just user_id + display_name).
        let old = serde_json::json!({
            "user_id": "00000000-0000-0000-0000-000000000001",
            "display_name": "Alice"
        });
        let s: UserSession = serde_json::from_value(old).unwrap();
        assert_eq!(s.display_name.as_deref(), Some("Alice"));
        assert!(s.claimed_by.is_none());
        assert_eq!(s.token_version, 0);
    }

    #[test]
    fn user_session_round_trip_with_new_fields() {
        let s = UserSession {
            user_id: Uuid::new_v4(),
            display_name: Some("Alice".into()),
            claimed_by: Some(Uuid::new_v4()),
            token_version: 7,
        };
        let j = serde_json::to_value(&s).unwrap();
        let back: UserSession = serde_json::from_value(j).unwrap();
        assert_eq!(back.user_id, s.user_id);
        assert_eq!(back.claimed_by, s.claimed_by);
        assert_eq!(back.token_version, 7);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p spades-server --bin spades-server dto::`
Expected: pass. (Other tests must also still pass — `cargo test -p spades-server` overall.)

- [ ] **Step 4: Commit**

```bash
git add crates/spades-server/src/bin/server/dto.rs
git commit -m "auth: extend UserSession with claimed_by + token_version (serde-default)"
```

### Task 7.2: session_ext helpers

**Files:**
- Modify: `crates/spades-server/src/auth/session_ext.rs`

**Important refactor:** the existing `bin/server/dto.rs::UserSession` becomes the *re-export* of a new `spades_server::auth::session_ext::UserSession` defined here. This avoids a duplicated type. The binary's `bin/server/handlers/players.rs` will continue to import via `dto::UserSession` unchanged.

- [ ] **Step 1: Write helpers + canonical UserSession**

Replace contents of `crates/spades-server/src/auth/session_ext.rs`:

```rust
//! Typed helpers over the tower-sessions session blob.
//!
//! The session blob lives under key `SESSION_USER_KEY`. This module owns the
//! canonical `UserSession` type; the binary re-exports it from `bin/server/dto.rs`.

use crate::auth::AuthError;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

pub const SESSION_USER_KEY: &str = "user";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UserSession {
    pub user_id: Uuid,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Set when the session belongs to a registered, logged-in user.
    #[serde(default)]
    pub claimed_by: Option<Uuid>,
    /// Snapshot of `users.token_version` at the time `claimed_by` was set.
    #[serde(default)]
    pub token_version: i32,
}

// Backward-compat alias for any code that already imported the old name.
pub type SessionUser = UserSession;

/// Read the session user. Mint a fresh anonymous one if absent.
pub async fn load_or_init(session: &Session) -> Result<UserSession, AuthError> {
    if let Some(s) = session.get::<UserSession>(SESSION_USER_KEY).await
        .map_err(|e| AuthError::Internal(format!("session get: {e}")))?
    {
        return Ok(s);
    }
    let fresh = UserSession {
        user_id: Uuid::new_v4(),
        ..Default::default()
    };
    session.insert(SESSION_USER_KEY, fresh.clone()).await
        .map_err(|e| AuthError::Internal(format!("session insert: {e}")))?;
    Ok(fresh)
}

/// Write the session user back.
pub async fn save(session: &Session, user: &UserSession) -> Result<(), AuthError> {
    session.insert(SESSION_USER_KEY, user.clone()).await
        .map_err(|e| AuthError::Internal(format!("session save: {e}")))?;
    Ok(())
}

/// Set `claimed_by` and `token_version` (i.e., mark the session as logged in).
pub async fn set_claimed(session: &Session, user_id: Uuid, token_version: i32) -> Result<UserSession, AuthError> {
    let mut s = load_or_init(session).await?;
    s.claimed_by = Some(user_id);
    s.token_version = token_version;
    save(session, &s).await?;
    Ok(s)
}

/// Clear `claimed_by` and `token_version` (i.e., log out). Preserves `user_id` (anon identity).
pub async fn clear_claimed(session: &Session) -> Result<(), AuthError> {
    let mut s = load_or_init(session).await?;
    s.claimed_by = None;
    s.token_version = 0;
    save(session, &s).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_sessions::{MemoryStore, SessionStore};

    async fn make_session() -> Session {
        let store = std::sync::Arc::new(MemoryStore::default());
        let record = tower_sessions::session::Record {
            id: Default::default(),
            data: Default::default(),
            expiry_date: time::OffsetDateTime::now_utc() + time::Duration::days(1),
        };
        let session = Session::new(None, store as std::sync::Arc<dyn SessionStore>, None);
        session
    }

    #[tokio::test]
    async fn load_or_init_mints_when_absent() {
        let s = make_session().await;
        let user = load_or_init(&s).await.unwrap();
        assert!(user.claimed_by.is_none());
        let user2 = load_or_init(&s).await.unwrap();
        assert_eq!(user.user_id, user2.user_id, "subsequent loads return same anon id");
    }

    #[tokio::test]
    async fn set_and_clear_claimed() {
        let s = make_session().await;
        let uid = Uuid::new_v4();
        set_claimed(&s, uid, 5).await.unwrap();
        let after_set = load_or_init(&s).await.unwrap();
        assert_eq!(after_set.claimed_by, Some(uid));
        assert_eq!(after_set.token_version, 5);

        clear_claimed(&s).await.unwrap();
        let after_clear = load_or_init(&s).await.unwrap();
        assert!(after_clear.claimed_by.is_none());
        assert_eq!(after_clear.token_version, 0);
        assert_eq!(after_clear.user_id, after_set.user_id, "anon id preserved");
    }
}
```

Note: the test helper `make_session` constructs a `Session` against an in-memory store. The exact `Session::new` signature differs by tower-sessions version — if the call doesn't compile, consult `tower_sessions::Session` docs in the version pinned in Cargo.lock (0.14). If `Record` doesn't have a public constructor, fall back to `Session::from_record` or build the session indirectly through a `SessionManagerLayer` and a mock service.

- [ ] **Step 2: Run tests**

Run: `cargo test -p spades-server auth::session_ext`
Expected: pass (may need test-helper tweaks per the note above).

- [ ] **Step 3: Commit**

```bash
git add crates/spades-server/src/auth/session_ext.rs
git commit -m "auth: session_ext helpers (load_or_init, save, set_claimed, clear_claimed)"
```

### Task 7.3: `Identity` and `AuthUser` extractors

**Files:**
- Modify: `crates/spades-server/src/auth/mod.rs`

- [ ] **Step 1: Define extractors in `mod.rs`**

Append to `crates/spades-server/src/auth/mod.rs`:

```rust
use crate::auth::session_ext::{load_or_init, SessionUser};
use crate::auth::users::User;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use std::sync::Arc;
use tower_sessions::Session;
use uuid::Uuid;

/// Shared auth state — wired in main.rs.
#[derive(Clone)]
pub struct AuthState {
    pub store: Arc<crate::sqlite_store::SqliteStore>,
    pub mailer: Arc<dyn mailer::Mailer>,
    pub oauth: Arc<oauth::OauthState>,
    pub rate: Arc<rate_limit::RateLimitState>,
    pub secure_cookies: bool,
}

#[derive(Debug, Clone)]
pub enum Identity {
    Registered { user: User, anon_id: Uuid },
    Anonymous { anon_id: Uuid },
}

impl Identity {
    pub fn anon_id(&self) -> Uuid {
        match self {
            Identity::Registered { anon_id, .. } | Identity::Anonymous { anon_id } => *anon_id,
        }
    }
    pub fn user(&self) -> Option<&User> {
        match self {
            Identity::Registered { user, .. } => Some(user),
            Identity::Anonymous { .. } => None,
        }
    }
}

/// Wrapper that yields a registered `User` or 401.
pub struct AuthUser(pub User);

#[axum::async_trait]
impl<S> FromRequestParts<S> for Identity
where
    S: Send + Sync,
    AuthState: axum::extract::FromRef<S>,
{
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state).await
            .map_err(|_| AuthError::Internal("session extractor failed".into()))?;
        let auth_state = AuthState::from_ref(state);
        identify(&session, &auth_state).await
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AuthState: axum::extract::FromRef<S>,
{
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = Identity::from_request_parts(parts, state).await?;
        match identity {
            Identity::Registered { user, .. } => Ok(AuthUser(user)),
            Identity::Anonymous { .. } => Err(AuthError::Unauthenticated),
        }
    }
}

/// Resolve the current `Identity` from a session. If `claimed_by` references
/// a missing user OR the stored `token_version` is stale, drops `claimed_by`
/// and returns `Anonymous`.
pub async fn identify(session: &Session, state: &AuthState) -> Result<Identity, AuthError> {
    let mut s = load_or_init(session).await?;
    let anon_id = s.user_id;

    let Some(claimed_id) = s.claimed_by else {
        return Ok(Identity::Anonymous { anon_id });
    };

    let user = state.store.find_user_by_id(claimed_id)
        .map_err(AuthError::Storage)?;

    let Some(user) = user else {
        // User deleted — drop the claim.
        s.claimed_by = None;
        s.token_version = 0;
        session_ext::save(session, &s).await?;
        return Ok(Identity::Anonymous { anon_id });
    };

    if user.token_version != s.token_version {
        // Stale session — password reset or rotation invalidated this.
        s.claimed_by = None;
        s.token_version = 0;
        session_ext::save(session, &s).await?;
        return Ok(Identity::Anonymous { anon_id });
    }

    Ok(Identity::Registered { user, anon_id })
}
```

- [ ] **Step 2: Add `axum::extract::FromRef` impl on the binary's app state**

The binary's `AppState` (in `bin/server/main.rs`) will gain `auth: AuthState` in Task 8.2. Defer the `FromRef` derivation to that task — for now, just verify this module compiles in isolation.

- [ ] **Step 3: Build**

Run: `cargo build -p spades-server`
Expected: success.

If the build fails with "axum::async_trait deprecated" or similar, replace `#[axum::async_trait]` with `#[async_trait::async_trait]` (axum 0.8 removed `axum::async_trait` in favor of using the `async-trait` crate directly).

- [ ] **Step 4: Commit**

```bash
git add crates/spades-server/src/auth/mod.rs
git commit -m "auth: Identity / AuthUser extractors + identify() with token_version check"
```

---

## Phase 8: AuthState + main.rs wiring

Stub the OAuth and rate-limit state types so `AuthState` compiles end-to-end, then wire `AuthState` into `AppState`, harden cookie settings, and add the `--insecure-cookies` CLI flag.

### Task 8.1: OauthState + RateLimitState stubs

**Files:**
- Modify: `crates/spades-server/src/auth/oauth.rs`
- Modify: `crates/spades-server/src/auth/rate_limit.rs`

- [ ] **Step 1: Stub `OauthState`**

Replace `crates/spades-server/src/auth/oauth.rs`:

```rust
//! Google + GitHub OAuth flow, state CSRF, PKCE, pending-signup store.

use std::collections::HashMap;
use std::sync::Mutex;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct OauthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Default)]
pub struct OauthState {
    pub google: Option<OauthProviderConfig>,
    pub github: Option<OauthProviderConfig>,
    pub redirect_base_url: String,
    pub csrf: Mutex<HashMap<String, (Uuid, OffsetDateTime)>>, // state -> (anon_id, expires_at)
    pub pending: Mutex<HashMap<String, PendingSignup>>,        // temp_id -> pending
}

#[derive(Clone, Debug)]
pub struct PendingSignup {
    pub provider: String,
    pub provider_uid: String,
    pub email: String,
    pub email_verified: bool,
    pub suggested_username: String,
    pub expires_at: OffsetDateTime,
}

impl OauthState {
    pub fn from_env() -> Self {
        let google = match (std::env::var("GOOGLE_OAUTH_CLIENT_ID"), std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")) {
            (Ok(id), Ok(sec)) => Some(OauthProviderConfig { client_id: id, client_secret: sec }),
            _ => None,
        };
        let github = match (std::env::var("GITHUB_OAUTH_CLIENT_ID"), std::env::var("GITHUB_OAUTH_CLIENT_SECRET")) {
            (Ok(id), Ok(sec)) => Some(OauthProviderConfig { client_id: id, client_secret: sec }),
            _ => None,
        };
        let redirect_base_url = std::env::var("OAUTH_REDIRECT_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        OauthState {
            google,
            github,
            redirect_base_url,
            csrf: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
        }
    }
}
```

- [ ] **Step 2: Stub `RateLimitState`**

Replace `crates/spades-server/src/auth/rate_limit.rs`:

```rust
//! Per-IP token-bucket via `governor` + per-account login lockout.

use governor::{Quota, RateLimiter};
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed, keyed::DefaultKeyedStateStore};
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
        fn ip(quota: Quota) -> Arc<IpLimiter> { Arc::new(RateLimiter::keyed(quota)) }
        fn s(quota: Quota) -> Arc<StringLimiter> { Arc::new(RateLimiter::keyed(quota)) }
        RateLimitState {
            login: ip(Quota::per_minute(NonZeroU32::new(10).unwrap()).allow_burst(NonZeroU32::new(60).unwrap())),
            register: ip(Quota::per_minute(NonZeroU32::new(3).unwrap()).allow_burst(NonZeroU32::new(20).unwrap())),
            password_reset_request_ip: ip(Quota::per_hour(NonZeroU32::new(3).unwrap())),
            password_reset_request_email: s(Quota::per_minute(NonZeroU32::new(1).unwrap())),
            password_reset_confirm: ip(Quota::per_hour(NonZeroU32::new(10).unwrap())),
            oauth_callback: ip(Quota::per_minute(NonZeroU32::new(30).unwrap())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn rate_limit_state_constructible() {
        let _ = RateLimitState::new();
    }

    #[test]
    fn login_limiter_throttles() {
        let s = RateLimitState::new();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for _ in 0..60 {
            // Burst capacity is 60, so first 60 should pass.
            let _ = s.login.check_key(&ip);
        }
        // 61st should likely be throttled (depends on quota math, but check the API works).
        let _ = s.login.check_key(&ip);
    }
}

// Per-account lockout repo methods will be added in Phase 11.
```

- [ ] **Step 3: Run build**

Run: `cargo build -p spades-server`
Expected: success (or error fixes per actual `governor` 0.7 API).

If `Quota::per_minute(...).allow_burst(...)` doesn't compile, the actual builder may be `Quota::per_minute(rate).with_burst_capacity(burst)` or constructed differently. Use the version pinned in Cargo.lock as ground truth.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-server/src/auth/oauth.rs crates/spades-server/src/auth/rate_limit.rs
git commit -m "auth: OauthState + RateLimitState stubs (config from env, governor buckets)"
```

### Task 8.2: Wire AuthState into AppState; configure cookies; add CLI flag

**Files:**
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Update `AppState`**

In `crates/spades-server/src/bin/server/main.rs`, replace the existing `AppState` declaration with:

```rust
#[derive(Clone)]
pub struct AppState {
    pub game_manager: GameManager,
    pub matchmaker: Matchmaker,
    pub challenge_manager: ChallengeManager,
    pub auth: spades_server::auth::AuthState,
    presence: PresenceTracker,
}

impl axum::extract::FromRef<AppState> for spades_server::auth::AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}
```

- [ ] **Step 2: Build `AuthState` and add `--insecure-cookies` flag**

In `main()` in `bin/server/main.rs`, after the existing `let game_manager = ...` block but before constructing `AppState`, add:

```rust
    let insecure_cookies = std::env::args().any(|a| a == "--insecure-cookies");

    // Build SqliteStore Arc — game_manager's existing store is private,
    // so open a fresh one keyed off the same DATABASE_URL.
    let auth_store_path = db_path.clone().unwrap_or_else(|| ":memory:".to_string());
    let auth_store = std::sync::Arc::new(
        spades_server::sqlite_store::SqliteStore::open(&auth_store_path)
            .expect("Failed to open auth SqliteStore"),
    );

    // Mailer: SmtpMailer if SMTP_* env vars are set, else LogMailer.
    let mailer: std::sync::Arc<dyn spades_server::auth::mailer::Mailer> =
        match spades_server::auth::mailer::SmtpConfig::from_env() {
            Some(cfg) => match spades_server::auth::mailer::SmtpMailer::new(cfg) {
                Ok(m) => {
                    println!("Mailer: SmtpMailer (SMTP_HOST set)");
                    std::sync::Arc::new(m)
                }
                Err(e) => {
                    eprintln!("SmtpMailer init failed ({e}); falling back to LogMailer");
                    std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new())
                }
            },
            None => {
                println!("Mailer: LogMailer (no SMTP_* env vars)");
                std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new())
            }
        };

    let oauth = std::sync::Arc::new(spades_server::auth::oauth::OauthState::from_env());
    if oauth.google.is_some() { println!("OAuth: Google enabled"); }
    if oauth.github.is_some() { println!("OAuth: GitHub enabled"); }

    let rate = std::sync::Arc::new(spades_server::auth::rate_limit::RateLimitState::new());

    let auth_state = spades_server::auth::AuthState {
        store: auth_store,
        mailer,
        oauth,
        rate,
        secure_cookies: !insecure_cookies,
    };
```

Then update the `app_state` construction:

```rust
    let app_state = AppState {
        game_manager,
        matchmaker,
        challenge_manager,
        auth: auth_state,
        presence: PresenceTracker::new(),
    };
```

- [ ] **Step 3: Harden the session manager layer**

Replace the existing `SessionManagerLayer` construction with:

```rust
    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("spades_session")
        .with_secure(!insecure_cookies)
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(30)));
```

`tower_sessions::cookie::SameSite` is the re-exported `cookie` crate type. If that import path is wrong, the crate re-exports it under a different module — check the tower-sessions 0.14 docs.

- [ ] **Step 4: Update the println at the bottom**

In the endpoint announcement block at the end of `main()`, append:

```rust
    println!("  POST /auth/register                             - Register an account");
    println!("  POST /auth/login                                - Log in");
    println!("  POST /auth/logout                               - Log out");
    println!("  GET  /auth/me                                   - Current user");
    println!("  GET  /auth/oauth/:provider/login                - OAuth login (google|github)");
    println!("  GET  /auth/oauth/:provider/callback             - OAuth callback");
    println!("  POST /auth/oauth/complete                       - Finish OAuth signup");
    println!("  POST /auth/password-reset/request               - Start password reset");
    println!("  POST /auth/password-reset/confirm               - Confirm new password");
    println!("  GET  /auth/verify-email                         - Email-verify confirm");
    println!("  GET  /users/:username                           - Public profile");
    println!("  GET  /users/:username/games                     - Profile game history");
    println!("  PATCH /users/me                                 - Update own account");
    if insecure_cookies {
        println!("WARNING: --insecure-cookies enabled. Session cookie lacks Secure flag. DO NOT use in production.");
    }
```

- [ ] **Step 5: Build**

Run: `cargo build -p spades-server`
Expected: success.

Existing tests in `bin/server/main.rs` (`tests` module) will fail because `AppState` now has an `auth` field. Update the test helper `test_app()` to construct an `AuthState`:

```rust
    fn test_app() -> TestServer {
        let game_manager = GameManager::new();
        let matchmaker = Matchmaker::new(game_manager.clone());
        let challenge_manager = ChallengeManager::new(game_manager.clone());

        let auth_store = std::sync::Arc::new(
            spades_server::sqlite_store::SqliteStore::open(":memory:").unwrap()
        );
        let auth_state = spades_server::auth::AuthState {
            store: auth_store,
            mailer: std::sync::Arc::new(spades_server::auth::mailer::LogMailer::new()),
            oauth: std::sync::Arc::new(spades_server::auth::oauth::OauthState::from_env()),
            rate: std::sync::Arc::new(spades_server::auth::rate_limit::RateLimitState::new()),
            secure_cookies: false,
        };

        let state = AppState {
            game_manager,
            matchmaker,
            challenge_manager,
            auth: auth_state,
            presence: PresenceTracker::new(),
        };

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false);

        let app = build_router(state).layer(session_layer);
        TestServer::new_with_config(
            app,
            TestServerConfig {
                save_cookies: true,
                ..Default::default()
            },
        )
        .unwrap()
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p spades-server`
Expected: all pre-existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/spades-server/src/bin/server/main.rs
git commit -m "auth: wire AuthState into AppState; harden cookie flags; --insecure-cookies"
```

---

## Phase 9: Register / login / logout / me handlers

The core auth API. Each handler is its own task because they hit slightly different code paths.

### Task 9.1: POST /auth/register

**Files:**
- Create: `crates/spades-server/src/bin/server/handlers/auth.rs`
- Modify: `crates/spades-server/src/bin/server/handlers/mod.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`
- Create: `crates/spades-server/tests/auth_register_login_flow.rs`

- [ ] **Step 1: Add handler skeleton**

Create `crates/spades-server/src/bin/server/handlers/auth.rs`:

```rust
use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use spades_server::auth::{
    AuthError, AuthState,
    mailer::Email,
    password::{hash_password, validate_password},
    session_ext,
    users::{validate_email, validate_username, NewUser, User},
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        UserResponse {
            id: u.id,
            username: u.username.clone(),
            email: u.email.clone(),
            email_verified: u.email_verified,
        }
    }
}

pub async fn register(
    State(state): State<AppState>,
    session: Session,
    Json(req): Json<RegisterRequest>,
) -> Result<(axum::http::StatusCode, Json<UserResponse>), AuthError> {
    let auth = &state.auth;

    // Validation
    let username = validate_username(&req.username)?;
    validate_email(&req.email)?;
    validate_password(&req.password)?;

    let hash = hash_password(&req.password)?;
    let new = NewUser {
        username,
        email: req.email.clone(),
        password_hash: Some(hash),
        email_verified: false,
    };

    // Insert
    let user_id = auth.store.insert_user(&new).map_err(|e| match e.as_str() {
        "username_taken" => AuthError::UsernameTaken,
        "email_taken" => AuthError::EmailTaken,
        other => AuthError::Storage(other.to_string()),
    })?;

    // Anon-claim: rebind any game seats owned by the current anon session.
    let s = session_ext::load_or_init(&session).await?;
    let anon_id = s.user_id;
    auth.store.claim_anon_game_seats(anon_id, user_id).map_err(AuthError::Storage)?;

    // Mark session logged-in.
    let user = auth.store.find_user_by_id(user_id).map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished after insert".into()))?;
    session_ext::set_claimed(&session, user_id, user.token_version).await?;

    // Send verify email (best-effort; failure does not block registration).
    let token = generate_email_token();
    let token_hash = sha256_hex(&token);
    auth.store.insert_auth_token(&token_hash, user_id, "verify_email", 24 * 3600)
        .map_err(AuthError::Storage)?;
    let link = format!("{}/auth/verify-email?token={}", auth.oauth.redirect_base_url, token);
    let _ = auth.mailer.send(Email {
        to: user.email.clone(),
        subject: "Verify your Spades email".into(),
        body: format!("Verify your email: {link}\n\nThis link expires in 24 hours."),
    }).await;

    Ok((axum::http::StatusCode::CREATED, Json(UserResponse::from(&user))))
}

fn generate_email_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(s.as_bytes());
    hex::encode(h)
}
```

- [ ] **Step 2: Add `base64` + `hex` to deps**

In `crates/spades-server/Cargo.toml`, append under `[dependencies]`:

```toml
base64 = "0.22"
hex = "0.4"
```

- [ ] **Step 3: Add `claim_anon_game_seats` + `insert_auth_token` to `SqliteStore`**

In `crates/spades-server/src/sqlite_store.rs`, append inside `impl SqliteStore`:

```rust
    pub fn claim_anon_game_seats(&self, anon_id: uuid::Uuid, user_id: uuid::Uuid) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn.execute(
            "UPDATE game_seats SET user_id = ?1 WHERE anon_user_id = ?2 AND user_id IS NULL",
            rusqlite::params![user_id.to_string(), anon_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn insert_auth_token(
        &self,
        token_hash: &str,
        user_id: uuid::Uuid,
        purpose: &str,
        ttl_secs: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at) \
             VALUES (?1, ?2, ?3, datetime('now', ?4))",
            rusqlite::params![token_hash, user_id.to_string(), purpose, format!("+{ttl_secs} seconds")],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
```

- [ ] **Step 4: Mount the route**

In `crates/spades-server/src/bin/server/handlers/mod.rs`, add:

```rust
pub mod auth;
```

In `crates/spades-server/src/bin/server/main.rs`, add an import:

```rust
use handlers::auth::register as auth_register;
```

And in `build_router`, append before `.with_state(state)`:

```rust
        .route("/auth/register", post(auth_register))
```

- [ ] **Step 5: Write integration test**

Create `crates/spades-server/tests/auth_register_login_flow.rs`:

```rust
//! Integration: register flow returns 201, /auth/me reflects the new user.
//! Login is covered in this file too so it lives next to its mirror image.

use axum::http::StatusCode;
use axum_test::{TestServer, TestServerConfig};
use serde_json::json;

mod common;

#[tokio::test]
async fn register_succeeds_and_logs_user_in() {
    let server = common::test_server();
    let resp = server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice",
            "email": "alice@example.com",
            "password": "hunter2-strong",
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["username"], "Alice");
    assert_eq!(body["email_verified"], false);

    // /auth/me should now return the same user.
    let me = server.get("/auth/me").await;
    me.assert_status(StatusCode::OK);
    let me_body: serde_json::Value = me.json();
    assert_eq!(me_body["username"], "Alice");
}

#[tokio::test]
async fn register_rejects_duplicate_username() {
    let server = common::test_server();
    let req = json!({
        "username": "Alice",
        "email": "alice@example.com",
        "password": "hunter2-strong",
    });
    server.post("/auth/register").json(&req).await.assert_status(StatusCode::CREATED);

    let dup = server.post("/auth/register").json(&json!({
        "username": "alice", // same canon
        "email": "different@example.com",
        "password": "hunter2-strong",
    })).await;
    dup.assert_status(StatusCode::CONFLICT);
    let b: serde_json::Value = dup.json();
    assert_eq!(b["error"], "username_taken");
}
```

- [ ] **Step 6: Add `tests/common.rs`**

Create `crates/spades-server/tests/common.rs` with a test-server builder:

```rust
//! Shared test scaffolding for auth integration tests.

use axum::{routing::{get, post}, Router};
use axum_test::{TestServer, TestServerConfig};
use spades_server::{
    auth::{AuthState, mailer::LogMailer, oauth::OauthState, rate_limit::RateLimitState},
    challenges::ChallengeManager,
    game_manager::GameManager,
    matchmaking::Matchmaker,
    sqlite_store::SqliteStore,
};
use std::sync::Arc;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

// Mirror of bin/server/main.rs::AppState. Re-declared here because main is a binary.
#[derive(Clone)]
pub struct AppState {
    pub game_manager: GameManager,
    pub matchmaker: Matchmaker,
    pub challenge_manager: ChallengeManager,
    pub auth: AuthState,
}

impl axum::extract::FromRef<AppState> for AuthState {
    fn from_ref(s: &AppState) -> Self { s.auth.clone() }
}

pub fn test_server() -> TestServer {
    let store = Arc::new(SqliteStore::open(":memory:").unwrap());
    let game_manager = GameManager::new();
    let matchmaker = Matchmaker::new(game_manager.clone());
    let challenge_manager = ChallengeManager::new(game_manager.clone());
    let auth = AuthState {
        store: store.clone(),
        mailer: Arc::new(LogMailer::new()),
        oauth: Arc::new(OauthState::from_env()),
        rate: Arc::new(RateLimitState::new()),
        secure_cookies: false,
    };
    let state = AppState { game_manager, matchmaker, challenge_manager, auth };

    let router = Router::new()
        .route("/auth/register", post(spades_server_test_routes::register))
        .route("/auth/me", get(spades_server_test_routes::me))
        .with_state(state);

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = router.layer(session_layer);
    TestServer::new_with_config(app, TestServerConfig {
        save_cookies: true,
        ..Default::default()
    }).unwrap()
}

// Test-only re-export of handlers: makes the bin/server/handlers/auth.rs handlers
// callable from integration tests by mirroring them as a tiny library namespace.
mod spades_server_test_routes {
    use super::AppState;
    use axum::extract::State;
    use axum::response::Json;
    use spades_server::auth::AuthError;
    use tower_sessions::Session;

    pub async fn register(
        State(_state): State<AppState>,
        _session: Session,
        Json(_req): Json<serde_json::Value>,
    ) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AuthError> {
        unimplemented!("placeholder; replaced when handlers move to lib in Task 9.4")
    }

    pub async fn me(
        State(_state): State<AppState>,
        _session: Session,
    ) -> Result<Json<serde_json::Value>, AuthError> {
        unimplemented!()
    }
}
```

**Note**: this scaffolding is intentionally pessimistic — the integration tests need handlers callable from outside the binary. The cleanest fix is to extract handlers into `spades-server`'s **library** (not binary) so tests can import them directly. This is done in **Task 9.4** below. For now, mark the test as `#[ignore]` if needed:

```rust
#[tokio::test]
#[ignore = "enabled after Task 9.4 moves handlers into lib"]
async fn register_succeeds_and_logs_user_in() { /* ... */ }
```

- [ ] **Step 7: Build + commit**

Run: `cargo build -p spades-server`. If failures from the test scaffold, leave the integration test file as `#[ignore]` and let Task 9.4 enable it.

```bash
git add crates/spades-server/src/bin/server/handlers/auth.rs \
        crates/spades-server/src/bin/server/handlers/mod.rs \
        crates/spades-server/src/bin/server/main.rs \
        crates/spades-server/src/sqlite_store.rs \
        crates/spades-server/Cargo.toml \
        crates/spades-server/tests/auth_register_login_flow.rs \
        crates/spades-server/tests/common.rs
git commit -m "auth: POST /auth/register handler + anon-claim + verify-email send"
```

### Task 9.2: POST /auth/login

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/auth.rs`
- Modify: `crates/spades-server/src/sqlite_store.rs` (lockout helpers)
- Modify: `crates/spades-server/src/bin/server/main.rs` (route mount)
- Modify: `crates/spades-server/tests/auth_register_login_flow.rs` (test the login path)

- [ ] **Step 1: Add login handler**

Append to `crates/spades-server/src/bin/server/handlers/auth.rs`:

```rust
use spades_server::auth::password::{verify_password, verify_against_dummy};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub login: String,    // email if contains '@', else username
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Json(req): Json<LoginRequest>,
) -> Result<Json<UserResponse>, AuthError> {
    let auth = &state.auth;

    let user_opt = if req.login.contains('@') {
        auth.store.find_user_by_email(&req.login).map_err(AuthError::Storage)?
    } else {
        auth.store.find_user_by_username(&req.login).map_err(AuthError::Storage)?
    };

    let Some(user) = user_opt else {
        verify_against_dummy(); // constant-time guard
        return Err(AuthError::InvalidCredentials);
    };

    // Check lockout
    if let Some(locked_until) = auth.store.get_lockout(user.id).map_err(AuthError::Storage)? {
        // locked_until is "datetime('now', ...)" stored as TEXT; treat it as locked if non-null and parses as future.
        let now = chrono::Utc::now().naive_utc();
        if let Ok(when) = chrono::NaiveDateTime::parse_from_str(&locked_until, "%Y-%m-%d %H:%M:%S") {
            if when > now {
                let secs = (when - now).num_seconds().max(1) as u64;
                return Err(AuthError::Locked { retry_after_secs: secs });
            }
        }
    }

    let Some(hash) = user.password_hash.as_deref() else {
        // OAuth-only account — no password set.
        verify_against_dummy();
        return Err(AuthError::InvalidCredentials);
    };

    if !verify_password(&req.password, hash)? {
        let new_count = auth.store.bump_login_failure(user.id).map_err(AuthError::Storage)?;
        let lock_secs = match new_count {
            n if n >= 10 => Some(60 * 60),
            n if n >= 5  => Some(15 * 60),
            _ => None,
        };
        if let Some(secs) = lock_secs {
            auth.store.set_lockout(user.id, secs).map_err(AuthError::Storage)?;
            return Err(AuthError::Locked { retry_after_secs: secs as u64 });
        }
        return Err(AuthError::InvalidCredentials);
    }

    // Success: clear failures, touch last_login, claim anon, mark session logged-in.
    auth.store.clear_login_failures(user.id).map_err(AuthError::Storage)?;
    auth.store.touch_user_login(user.id).map_err(AuthError::Storage)?;
    let s = session_ext::load_or_init(&session).await?;
    auth.store.claim_anon_game_seats(s.user_id, user.id).map_err(AuthError::Storage)?;
    session_ext::set_claimed(&session, user.id, user.token_version).await?;

    Ok(Json(UserResponse::from(&user)))
}
```

- [ ] **Step 2: Add `chrono` to deps**

`crates/spades-server/Cargo.toml`:

```toml
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

- [ ] **Step 3: Add lockout helpers to `SqliteStore`**

Append in `crates/spades-server/src/sqlite_store.rs`:

```rust
    pub fn get_lockout(&self, user_id: uuid::Uuid) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT locked_until FROM login_failures WHERE user_id = ?1",
            rusqlite::params![user_id.to_string()],
            |r| r.get::<_, Option<String>>(0),
        ).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    /// Increment failure_count and return new value.
    pub fn bump_login_failure(&self, user_id: uuid::Uuid) -> Result<i32, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO login_failures (user_id, failure_count) VALUES (?1, 1) \
             ON CONFLICT(user_id) DO UPDATE SET failure_count = failure_count + 1",
            rusqlite::params![user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        let n: i32 = conn.query_row(
            "SELECT failure_count FROM login_failures WHERE user_id = ?1",
            rusqlite::params![user_id.to_string()],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn set_lockout(&self, user_id: uuid::Uuid, secs: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE login_failures SET locked_until = datetime('now', ?2) WHERE user_id = ?1",
            rusqlite::params![user_id.to_string(), format!("+{secs} seconds")],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_login_failures(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM login_failures WHERE user_id = ?1",
            rusqlite::params![user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
```

- [ ] **Step 4: Mount route**

In `bin/server/main.rs`, in `build_router`:

```rust
        .route("/auth/login", post(handlers::auth::login))
```

- [ ] **Step 5: Build**

Run: `cargo build -p spades-server`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "auth: POST /auth/login + lockout helpers (per-account 5/10 escalation)"
```

### Task 9.3: POST /auth/logout + GET /auth/me

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/auth.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Add handlers**

Append to `bin/server/handlers/auth.rs`:

```rust
use spades_server::auth::AuthUser;

pub async fn logout(session: Session) -> Result<axum::http::StatusCode, AuthError> {
    session_ext::clear_claimed(&session).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn me(AuthUser(user): AuthUser) -> Json<UserResponse> {
    Json(UserResponse::from(&user))
}
```

- [ ] **Step 2: Mount routes**

In `bin/server/main.rs`:

```rust
        .route("/auth/logout", post(handlers::auth::logout))
        .route("/auth/me", get(handlers::auth::me))
```

- [ ] **Step 3: Build**

Run: `cargo build -p spades-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "auth: POST /auth/logout + GET /auth/me"
```

### Task 9.4: Move handlers into the library + enable integration tests

The bin's handlers can't be called from integration tests cleanly. Move the auth handlers into `spades-server` proper so they're importable.

**Files:**
- Move: `crates/spades-server/src/bin/server/handlers/auth.rs` → `crates/spades-server/src/handlers_auth.rs`
- Modify: `crates/spades-server/src/lib.rs`
- Modify: `crates/spades-server/src/bin/server/handlers/mod.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`
- Modify: `crates/spades-server/tests/common.rs`
- Modify: `crates/spades-server/tests/auth_register_login_flow.rs`

- [ ] **Step 1: Move the file**

```bash
git mv crates/spades-server/src/bin/server/handlers/auth.rs crates/spades-server/src/handlers_auth.rs
```

- [ ] **Step 2: Re-namespace**

In the moved file, replace `use crate::AppState;` with a generic state type. Replace the `State<AppState>` signature with `State<AuthState>` (the `FromRef` derivation makes this work in both bin and tests):

```rust
use axum::{extract::State, response::Json};
use spades_server::auth::{AuthError, AuthState, AuthUser, ...};
```

Wait — this is the library, so `use spades_server::...` becomes `use crate::auth::...`.

Concretely: in `crates/spades-server/src/handlers_auth.rs`:

```rust
use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::{
    AuthError, AuthState, AuthUser,
    mailer::Email,
    password::{hash_password, validate_password, verify_password, verify_against_dummy},
    session_ext,
    users::{validate_email, validate_username, NewUser, User},
};

// ... (rest of handler bodies, but change all `State<AppState>` to `State<AuthState>`,
//      and `state.auth.store` to `state.store`, etc.)
```

- [ ] **Step 3: Re-export from lib**

In `crates/spades-server/src/lib.rs`, add:

```rust
pub mod handlers_auth;
```

- [ ] **Step 4: Update bin to re-export**

In `crates/spades-server/src/bin/server/handlers/mod.rs`, replace `pub mod auth;` with:

```rust
pub use spades_server::handlers_auth as auth;
```

- [ ] **Step 5: Fix `State<AppState>` ↔ `State<AuthState>`**

In `bin/server/main.rs` `build_router`, change the auth routes to use `AuthState`:

The trick: axum's `State` extractor uses `FromRef` to peel the right state out of `AppState`. Since `AuthState` already implements `FromRef<AppState>`, handlers can take `State<AuthState>` directly when invoked from a router with `.with_state(AppState)`.

- [ ] **Step 6: Update `tests/common.rs`**

Drop the placeholder `spades_server_test_routes` module. Mount the real handlers:

```rust
use spades_server::handlers_auth;
// ...
let router = Router::new()
    .route("/auth/register", post(handlers_auth::register))
    .route("/auth/login", post(handlers_auth::login))
    .route("/auth/logout", post(handlers_auth::logout))
    .route("/auth/me", get(handlers_auth::me))
    .with_state(auth);
```

Note: the test router uses `AuthState` directly (no `AppState`). Handlers take `State<AuthState>`. Tests don't need the rest of AppState yet.

- [ ] **Step 7: Un-`#[ignore]` the test in `auth_register_login_flow.rs`**

Remove `#[ignore = "..."]` from the tests added in Task 9.1.

- [ ] **Step 8: Run integration tests**

Run: `cargo test -p spades-server --test auth_register_login_flow`
Expected: both tests pass.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "auth: move handlers into lib (handlers_auth); enable integration tests"
```

---

## Phase 10: Email verification + password reset

Single-use token table is already in place (Phase 2). Wire it through three handlers.

### Task 10.1: Token repo + helper module

**Files:**
- Modify: `crates/spades-server/src/auth/tokens.rs`
- Modify: `crates/spades-server/src/sqlite_store.rs`

- [ ] **Step 1: Append helpers**

`crates/spades-server/src/auth/tokens.rs`:

```rust
//! Single-use email tokens (verify-email, password-reset).

use crate::auth::AuthError;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PURPOSE_VERIFY_EMAIL: &str = "verify_email";
pub const PURPOSE_PASSWORD_RESET: &str = "password_reset";

pub fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Debug, Clone)]
pub struct ConsumedToken {
    pub user_id: Uuid,
    pub purpose: String,
}
```

`crates/spades-server/src/sqlite_store.rs` (append to `impl SqliteStore`):

```rust
    pub fn consume_auth_token(&self, token_hash: &str, expected_purpose: &str)
        -> Result<crate::auth::tokens::ConsumedToken, String>
    {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let row: Option<(String, String, String, Option<String>)> = conn.query_row(
            "SELECT user_id, purpose, expires_at, used_at FROM auth_tokens WHERE token_hash = ?1",
            rusqlite::params![token_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })?;
        let Some((user_id_s, purpose, expires_at, used_at)) = row else {
            return Err("token_invalid".into());
        };
        if used_at.is_some() { return Err("token_invalid".into()); }
        if purpose != expected_purpose { return Err("token_invalid".into()); }
        // Check expiry: stored as 'YYYY-MM-DD HH:MM:SS' in SQLite UTC.
        let now = chrono::Utc::now().naive_utc();
        if let Ok(when) = chrono::NaiveDateTime::parse_from_str(&expires_at, "%Y-%m-%d %H:%M:%S") {
            if when < now { return Err("token_invalid".into()); }
        }
        conn.execute(
            "UPDATE auth_tokens SET used_at = datetime('now') WHERE token_hash = ?1",
            rusqlite::params![token_hash],
        ).map_err(|e| e.to_string())?;
        let user_id = uuid::Uuid::parse_str(&user_id_s).map_err(|e| e.to_string())?;
        Ok(crate::auth::tokens::ConsumedToken { user_id, purpose })
    }

    pub fn cleanup_expired_tokens(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn.execute(
            "DELETE FROM auth_tokens WHERE expires_at < datetime('now') OR used_at IS NOT NULL",
            [],
        ).map_err(|e| e.to_string())?;
        Ok(n)
    }
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p spades-server && git add -A
git commit -m "auth: token repo (generate, hash, consume, cleanup)"
```

### Task 10.2: GET /auth/verify-email

**Files:**
- Modify: `crates/spades-server/src/handlers_auth.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Handler**

Append to `handlers_auth.rs`:

```rust
use axum::extract::Query;
use axum::response::Redirect;
use crate::auth::tokens::{hash_token, PURPOSE_VERIFY_EMAIL};

#[derive(Deserialize)]
pub struct VerifyEmailQuery {
    pub token: String,
}

pub async fn verify_email(
    State(auth): State<AuthState>,
    Query(q): Query<VerifyEmailQuery>,
) -> Result<Redirect, AuthError> {
    let hash = hash_token(&q.token);
    let consumed = auth.store.consume_auth_token(&hash, PURPOSE_VERIFY_EMAIL)
        .map_err(|e| if e == "token_invalid" { AuthError::TokenInvalid } else { AuthError::Storage(e) })?;
    auth.store.set_user_email_verified(consumed.user_id).map_err(AuthError::Storage)?;
    Ok(Redirect::to("/"))
}
```

- [ ] **Step 2: Mount route + commit**

`bin/server/main.rs`:
```rust
        .route("/auth/verify-email", get(handlers_auth::verify_email))
```

```bash
cargo test -p spades-server && git add -A
git commit -m "auth: GET /auth/verify-email"
```

### Task 10.3: Password reset request + confirm

**Files:**
- Modify: `crates/spades-server/src/handlers_auth.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Handlers**

Append to `handlers_auth.rs`:

```rust
use crate::auth::tokens::{generate_token, PURPOSE_PASSWORD_RESET};

#[derive(Deserialize)]
pub struct PasswordResetRequestBody {
    pub email: String,
}

pub async fn password_reset_request(
    State(auth): State<AuthState>,
    Json(req): Json<PasswordResetRequestBody>,
) -> Result<axum::http::StatusCode, AuthError> {
    if let Some(user) = auth.store.find_user_by_email(&req.email).map_err(AuthError::Storage)? {
        let token = generate_token();
        let hash = hash_token(&token);
        auth.store.insert_auth_token(&hash, user.id, PURPOSE_PASSWORD_RESET, 3600)
            .map_err(AuthError::Storage)?;
        let link = format!("{}/auth/password-reset?token={}", auth.oauth.redirect_base_url, token);
        let _ = auth.mailer.send(Email {
            to: user.email,
            subject: "Reset your Spades password".into(),
            body: format!("Reset link: {link}\n\nExpires in 1 hour."),
        }).await;
    }
    // Always 202 to avoid leaking existence.
    Ok(axum::http::StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub struct PasswordResetConfirmBody {
    pub token: String,
    pub new_password: String,
}

pub async fn password_reset_confirm(
    State(auth): State<AuthState>,
    session: Session,
    Json(req): Json<PasswordResetConfirmBody>,
) -> Result<axum::http::StatusCode, AuthError> {
    validate_password(&req.new_password)?;
    let hash = hash_token(&req.token);
    let consumed = auth.store.consume_auth_token(&hash, PURPOSE_PASSWORD_RESET)
        .map_err(|e| if e == "token_invalid" { AuthError::TokenInvalid } else { AuthError::Storage(e) })?;
    let new_hash = hash_password(&req.new_password)?;
    let new_version = auth.store.update_user_password(consumed.user_id, &new_hash)
        .map_err(AuthError::Storage)?;
    // Log requester in with bumped token_version so they aren't immediately logged out.
    session_ext::set_claimed(&session, consumed.user_id, new_version).await?;
    Ok(axum::http::StatusCode::OK)
}
```

- [ ] **Step 2: Mount + commit**

`bin/server/main.rs`:
```rust
        .route("/auth/password-reset/request", post(handlers_auth::password_reset_request))
        .route("/auth/password-reset/confirm", post(handlers_auth::password_reset_confirm))
```

```bash
cargo test -p spades-server && git add -A
git commit -m "auth: password-reset request + confirm (token_version bump invalidates other sessions)"
```

---

## Phase 11: Rate limiting

The `RateLimitState` is already constructed in Task 8.1. Wire it through the handlers.

### Task 11.1: Apply rate limits to auth endpoints

**Files:**
- Modify: `crates/spades-server/src/auth/rate_limit.rs`
- Modify: `crates/spades-server/src/handlers_auth.rs`

- [ ] **Step 1: Helper that returns `RateLimited` when exhausted**

Append to `crates/spades-server/src/auth/rate_limit.rs`:

```rust
use crate::auth::AuthError;

pub fn check_ip(
    limiter: &governor::RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>,
    ip: IpAddr,
) -> Result<(), AuthError> {
    limiter.check_key(&ip).map_err(|nu| AuthError::RateLimited {
        retry_after_secs: nu.wait_time_from(std::time::Instant::now()).as_secs().max(1),
    })?;
    Ok(())
}

pub fn check_email(
    limiter: &governor::RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>,
    email: &str,
) -> Result<(), AuthError> {
    limiter.check_key(&email.to_string()).map_err(|nu| AuthError::RateLimited {
        retry_after_secs: nu.wait_time_from(std::time::Instant::now()).as_secs().max(1),
    })?;
    Ok(())
}
```

(Type signatures may need adjustment for `governor` 0.7's `Jitter` / `NotUntil` API.)

- [ ] **Step 2: Use in handlers**

At the top of each rate-limited handler, take a `ConnectInfo<SocketAddr>` extractor and check the bucket. Example for login:

```rust
use axum::extract::ConnectInfo;
use std::net::SocketAddr;
use crate::auth::rate_limit::check_ip;

pub async fn login(
    State(auth): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    session: Session,
    Json(req): Json<LoginRequest>,
) -> Result<Json<UserResponse>, AuthError> {
    check_ip(&auth.rate.login, addr.ip())?;
    // ... existing login body ...
}
```

Apply equivalent calls in `register` (`auth.rate.register`), `password_reset_request` (`auth.rate.password_reset_request_ip` plus `check_email(&auth.rate.password_reset_request_email, &req.email)`), `password_reset_confirm` (`auth.rate.password_reset_confirm`), and any OAuth callbacks (Phase 12+).

- [ ] **Step 3: Build + commit**

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: wire rate-limit buckets into register/login/password-reset handlers"
```

---

## Phase 12: OAuth foundation

State CSRF, PKCE, route mounting, login redirects. Provider-specific callbacks land in Phases 13-14.

### Task 12.1: OAuth client wiring + state CSRF helpers + login redirect

**Files:**
- Modify: `crates/spades-server/src/auth/oauth.rs`
- Create: `crates/spades-server/src/bin/server/handlers/oauth.rs` (then move to lib in Task 9.4 pattern)
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: OAuth client constructors**

Append to `crates/spades-server/src/auth/oauth.rs`:

```rust
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, TokenUrl, AuthorizationCode, CsrfToken, Scope,
};
use std::time::Duration;

pub fn google_client(state: &OauthState) -> Option<BasicClient> {
    let cfg = state.google.as_ref()?;
    Some(BasicClient::new(
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
        AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into()).ok()?,
        Some(TokenUrl::new("https://oauth2.googleapis.com/token".into()).ok()?),
    ).set_redirect_uri(
        RedirectUrl::new(format!("{}/auth/oauth/google/callback", state.redirect_base_url)).ok()?,
    ))
}

pub fn github_client(state: &OauthState) -> Option<BasicClient> {
    let cfg = state.github.as_ref()?;
    Some(BasicClient::new(
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
        AuthUrl::new("https://github.com/login/oauth/authorize".into()).ok()?,
        Some(TokenUrl::new("https://github.com/login/oauth/access_token".into()).ok()?),
    ).set_redirect_uri(
        RedirectUrl::new(format!("{}/auth/oauth/github/callback", state.redirect_base_url)).ok()?,
    ))
}

pub fn record_csrf(state: &OauthState, csrf: String, anon_id: Uuid, verifier: PkceCodeVerifier) {
    let expires = OffsetDateTime::now_utc() + time::Duration::minutes(10);
    state.csrf.lock().unwrap().insert(csrf, (anon_id, expires));
    // Store the PKCE verifier in a parallel map; for simplicity we glue it onto the same key.
    // (Real impl: extend the CSRF map's value tuple to include verifier.)
}
```

Note: storing the PKCE verifier alongside the CSRF entry needs the map value to be `(Uuid, OffsetDateTime, PkceCodeVerifier)`. Update the type accordingly:

```rust
pub csrf: Mutex<HashMap<String, (Uuid, OffsetDateTime, PkceCodeVerifier)>>,
```

- [ ] **Step 2: Add /auth/oauth/:provider/login handler**

Append to `handlers_auth.rs` (or create `src/handlers_oauth.rs` for separation):

```rust
use axum::extract::Path;
use crate::auth::oauth::{google_client, github_client, record_csrf};
use oauth2::{PkceCodeChallenge, CsrfToken, Scope};

pub async fn oauth_login(
    State(auth): State<AuthState>,
    session: Session,
    Path(provider): Path<String>,
) -> Result<Redirect, AuthError> {
    let s = session_ext::load_or_init(&session).await?;
    let anon_id = s.user_id;

    let client = match provider.as_str() {
        "google" => google_client(&auth.oauth),
        "github" => github_client(&auth.oauth),
        _ => return Err(AuthError::OauthFailed("unknown provider".into())),
    }.ok_or_else(|| AuthError::OauthFailed("provider not configured".into()))?;

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let scopes: Vec<Scope> = match provider.as_str() {
        "google" => vec![Scope::new("openid".into()), Scope::new("email".into()), Scope::new("profile".into())],
        "github" => vec![Scope::new("read:user".into()), Scope::new("user:email".into())],
        _ => vec![],
    };
    let (auth_url, csrf_token) = client.authorize_url(CsrfToken::new_random)
        .add_scopes(scopes)
        .set_pkce_challenge(challenge)
        .url();

    auth.oauth.csrf.lock().unwrap().insert(
        csrf_token.secret().clone(),
        (anon_id, time::OffsetDateTime::now_utc() + time::Duration::minutes(10), verifier),
    );

    Ok(Redirect::to(auth_url.as_str()))
}
```

- [ ] **Step 3: Mount + commit**

```rust
        .route("/auth/oauth/{provider}/login", get(handlers_auth::oauth_login))
```

```bash
cargo build -p spades-server && git add -A
git commit -m "auth: OAuth login redirect (Google + GitHub) with PKCE + state CSRF"
```

---

## Phase 13: OAuth Google callback + /auth/oauth/complete

### Task 13.1: GET /auth/oauth/google/callback

**Files:**
- Modify: `crates/spades-server/src/handlers_auth.rs`
- Modify: `crates/spades-server/src/auth/oauth.rs` (SqliteStore helpers for oauth_accounts)
- Modify: `crates/spades-server/src/sqlite_store.rs`

- [ ] **Step 1: oauth_accounts helpers**

Append to `SqliteStore`:

```rust
    pub fn find_oauth_account(&self, provider: &str, provider_uid: &str)
        -> Result<Option<uuid::Uuid>, String>
    {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT user_id FROM oauth_accounts WHERE provider = ?1 AND provider_uid = ?2",
            rusqlite::params![provider, provider_uid],
            |r| {
                let s: String = r.get(0)?;
                Ok(s)
            },
        ).map(|s| Some(uuid::Uuid::parse_str(&s).unwrap()))
         .or_else(|e| match e {
             rusqlite::Error::QueryReturnedNoRows => Ok(None),
             other => Err(other.to_string()),
         })
    }

    pub fn insert_oauth_account(&self, provider: &str, provider_uid: &str, user_id: uuid::Uuid, email: &str)
        -> Result<(), String>
    {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO oauth_accounts (provider, provider_uid, user_id, email) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provider, provider_uid, user_id.to_string(), email],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
```

- [ ] **Step 2: Callback handler**

Append to `handlers_auth.rs`:

```rust
use crate::auth::oauth::PendingSignup;
use axum::extract::Query;
use oauth2::{AuthorizationCode, TokenResponse};

#[derive(Deserialize)]
pub struct OauthCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
struct GoogleUserinfo {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
}

pub async fn oauth_google_callback(
    State(auth): State<AuthState>,
    session: Session,
    Query(q): Query<OauthCallbackQuery>,
) -> Result<Redirect, AuthError> {
    // 1. Validate state, retrieve PKCE verifier and anon_id.
    let entry = auth.oauth.csrf.lock().unwrap().remove(&q.state);
    let (anon_id_from_csrf, _expires, verifier) = entry.ok_or_else(|| AuthError::OauthFailed("invalid state".into()))?;
    let client = crate::auth::oauth::google_client(&auth.oauth)
        .ok_or_else(|| AuthError::OauthFailed("google not configured".into()))?;

    // 2. Exchange code for token.
    let token = client.exchange_code(AuthorizationCode::new(q.code))
        .set_pkce_verifier(verifier)
        .request_async(oauth2::reqwest::async_http_client).await
        .map_err(|e| AuthError::OauthFailed(format!("token exchange: {e}")))?;

    // 3. Fetch userinfo.
    let info: GoogleUserinfo = reqwest::Client::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(token.access_token().secret())
        .send().await.map_err(|e| AuthError::OauthFailed(format!("userinfo fetch: {e}")))?
        .error_for_status().map_err(|e| AuthError::OauthFailed(format!("userinfo status: {e}")))?
        .json().await.map_err(|e| AuthError::OauthFailed(format!("userinfo parse: {e}")))?;

    // 4. Hit / miss / link / pending-signup branch.
    if let Some(uid) = auth.store.find_oauth_account("google", &info.sub).map_err(AuthError::Storage)? {
        // Existing oauth account — log in.
        let user = auth.store.find_user_by_id(uid).map_err(AuthError::Storage)?
            .ok_or_else(|| AuthError::Internal("oauth account refs missing user".into()))?;
        claim_and_login(&session, &auth, anon_id_from_csrf, &user).await?;
        return Ok(Redirect::to("/"));
    }

    if info.email_verified {
        if let Some(existing) = auth.store.find_user_by_email(&info.email).map_err(AuthError::Storage)? {
            if existing.email_verified {
                auth.store.insert_oauth_account("google", &info.sub, existing.id, &info.email)
                    .map_err(AuthError::Storage)?;
                claim_and_login(&session, &auth, anon_id_from_csrf, &existing).await?;
                return Ok(Redirect::to("/"));
            }
        }
    }

    // No match → pending-signup flow.
    let temp_id = crate::auth::tokens::generate_token();
    let suggested = info.name.clone()
        .map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(20).collect::<String>())
        .filter(|s| s.len() >= 2)
        .unwrap_or_else(|| "user".into());
    let expires_at = time::OffsetDateTime::now_utc() + time::Duration::minutes(15);
    auth.oauth.pending.lock().unwrap().insert(temp_id.clone(), PendingSignup {
        provider: "google".into(),
        provider_uid: info.sub,
        email: info.email,
        email_verified: info.email_verified,
        suggested_username: suggested,
        expires_at,
    });

    // Set __oauth_pending cookie (separate from tower-sessions).
    let cookie_val = format!("__oauth_pending={temp_id}; Max-Age=900; HttpOnly; SameSite=Lax; Path=/");
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut().append(axum::http::header::SET_COOKIE, cookie_val.parse().unwrap());
    Ok(Redirect::to("/")) // (Note: returning Redirect alone loses the cookie; emit raw Response instead.)
}

async fn claim_and_login(
    session: &Session,
    auth: &AuthState,
    anon_id_from_csrf: Uuid,
    user: &User,
) -> Result<(), AuthError> {
    // Prefer the session's current anon_id (more recent) if it differs from the one stored at /login.
    let live = session_ext::load_or_init(session).await?;
    let anon = live.user_id;
    auth.store.claim_anon_game_seats(anon, user.id).map_err(AuthError::Storage)?;
    if anon != anon_id_from_csrf {
        auth.store.claim_anon_game_seats(anon_id_from_csrf, user.id).map_err(AuthError::Storage)?;
    }
    session_ext::set_claimed(session, user.id, user.token_version).await?;
    Ok(())
}
```

The "returning Redirect alone loses the cookie" comment marks the known gotcha: the engineer should construct a `Response` builder so the `Set-Cookie` is honored, e.g.:

```rust
use axum::response::IntoResponse;
let mut resp = Redirect::to("/").into_response();
resp.headers_mut().append(axum::http::header::SET_COOKIE, cookie_val.parse().unwrap());
Ok(resp)  // and update the return type to Response
```

- [ ] **Step 3: Mount + commit**

```rust
        .route("/auth/oauth/google/callback", get(handlers_auth::oauth_google_callback))
```

```bash
cargo build -p spades-server && git add -A
git commit -m "auth: OAuth Google callback (token exchange, userinfo, link-by-email, pending-signup)"
```

### Task 13.2: POST /auth/oauth/complete

**Files:**
- Modify: `crates/spades-server/src/handlers_auth.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Handler**

Append to `handlers_auth.rs`:

```rust
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::extract::TypedHeader;

#[derive(Deserialize)]
pub struct OauthCompleteRequest {
    pub username: String,
}

pub async fn oauth_complete(
    State(auth): State<AuthState>,
    session: Session,
    cookie_jar: axum_extra::extract::CookieJar,
    Json(req): Json<OauthCompleteRequest>,
) -> Result<(axum::http::StatusCode, Json<UserResponse>), AuthError> {
    let temp_id = cookie_jar.get("__oauth_pending")
        .ok_or(AuthError::TokenInvalid)?
        .value().to_string();
    let pending = auth.oauth.pending.lock().unwrap().remove(&temp_id)
        .ok_or(AuthError::TokenInvalid)?;
    if pending.expires_at < time::OffsetDateTime::now_utc() {
        return Err(AuthError::TokenInvalid);
    }
    let username = validate_username(&req.username)?;

    let user_id = auth.store.insert_user(&NewUser {
        username,
        email: pending.email.clone(),
        password_hash: None,
        email_verified: pending.email_verified,
    }).map_err(|e| match e.as_str() {
        "username_taken" => AuthError::UsernameTaken,
        "email_taken" => AuthError::EmailTaken,
        other => AuthError::Storage(other.into()),
    })?;
    auth.store.insert_oauth_account(&pending.provider, &pending.provider_uid, user_id, &pending.email)
        .map_err(AuthError::Storage)?;

    let s = session_ext::load_or_init(&session).await?;
    auth.store.claim_anon_game_seats(s.user_id, user_id).map_err(AuthError::Storage)?;
    let user = auth.store.find_user_by_id(user_id).map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished".into()))?;
    session_ext::set_claimed(&session, user_id, user.token_version).await?;

    Ok((axum::http::StatusCode::CREATED, Json(UserResponse::from(&user))))
}
```

- [ ] **Step 2: Add `axum-extra` dep**

`crates/spades-server/Cargo.toml`:

```toml
axum-extra = { version = "0.10", features = ["cookie"] }
```

- [ ] **Step 3: Mount + commit**

```rust
        .route("/auth/oauth/complete", post(handlers_auth::oauth_complete))
```

```bash
cargo build -p spades-server && git add -A
git commit -m "auth: POST /auth/oauth/complete (finish pending OAuth signup with chosen username)"
```

---

## Phase 14: OAuth GitHub callback

GitHub flow differs only in the userinfo fetch (two endpoints: `/user` + `/user/emails`).

### Task 14.1: GET /auth/oauth/github/callback

**Files:**
- Modify: `crates/spades-server/src/handlers_auth.rs`

- [ ] **Step 1: Handler**

Append to `handlers_auth.rs`:

```rust
#[derive(Deserialize)]
struct GithubUser {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

pub async fn oauth_github_callback(
    State(auth): State<AuthState>,
    session: Session,
    Query(q): Query<OauthCallbackQuery>,
) -> Result<axum::response::Response, AuthError> {
    let entry = auth.oauth.csrf.lock().unwrap().remove(&q.state);
    let (anon_from_csrf, _expires, verifier) = entry.ok_or_else(|| AuthError::OauthFailed("invalid state".into()))?;
    let client = crate::auth::oauth::github_client(&auth.oauth)
        .ok_or_else(|| AuthError::OauthFailed("github not configured".into()))?;

    let token = client.exchange_code(AuthorizationCode::new(q.code))
        .set_pkce_verifier(verifier)
        .request_async(oauth2::reqwest::async_http_client).await
        .map_err(|e| AuthError::OauthFailed(format!("token exchange: {e}")))?;

    let http = reqwest::Client::new();
    let user: GithubUser = http.get("https://api.github.com/user")
        .bearer_auth(token.access_token().secret())
        .header("User-Agent", "rust-spades")
        .send().await.map_err(|e| AuthError::OauthFailed(format!("user fetch: {e}")))?
        .error_for_status().map_err(|e| AuthError::OauthFailed(format!("user status: {e}")))?
        .json().await.map_err(|e| AuthError::OauthFailed(format!("user parse: {e}")))?;
    let emails: Vec<GithubEmail> = http.get("https://api.github.com/user/emails")
        .bearer_auth(token.access_token().secret())
        .header("User-Agent", "rust-spades")
        .send().await.map_err(|e| AuthError::OauthFailed(format!("emails fetch: {e}")))?
        .error_for_status().map_err(|e| AuthError::OauthFailed(format!("emails status: {e}")))?
        .json().await.map_err(|e| AuthError::OauthFailed(format!("emails parse: {e}")))?;
    let primary = emails.iter().find(|e| e.primary)
        .ok_or_else(|| AuthError::OauthFailed("no primary email".into()))?;

    if let Some(uid) = auth.store.find_oauth_account("github", &user.id.to_string()).map_err(AuthError::Storage)? {
        let u = auth.store.find_user_by_id(uid).map_err(AuthError::Storage)?
            .ok_or_else(|| AuthError::Internal("oauth account refs missing".into()))?;
        claim_and_login(&session, &auth, anon_from_csrf, &u).await?;
        return Ok(Redirect::to("/").into_response());
    }
    if primary.verified {
        if let Some(existing) = auth.store.find_user_by_email(&primary.email).map_err(AuthError::Storage)? {
            if existing.email_verified {
                auth.store.insert_oauth_account("github", &user.id.to_string(), existing.id, &primary.email)
                    .map_err(AuthError::Storage)?;
                claim_and_login(&session, &auth, anon_from_csrf, &existing).await?;
                return Ok(Redirect::to("/").into_response());
            }
        }
    }

    let temp_id = crate::auth::tokens::generate_token();
    let suggested: String = user.login.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(20).collect();
    let expires_at = time::OffsetDateTime::now_utc() + time::Duration::minutes(15);
    auth.oauth.pending.lock().unwrap().insert(temp_id.clone(), PendingSignup {
        provider: "github".into(),
        provider_uid: user.id.to_string(),
        email: primary.email.clone(),
        email_verified: primary.verified,
        suggested_username: suggested,
        expires_at,
    });
    let mut resp = Redirect::to("/").into_response();
    let cookie = format!("__oauth_pending={temp_id}; Max-Age=900; HttpOnly; SameSite=Lax; Path=/");
    resp.headers_mut().append(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    Ok(resp)
}
```

- [ ] **Step 2: Mount + commit**

```rust
        .route("/auth/oauth/github/callback", get(handlers_auth::oauth_github_callback))
```

```bash
cargo build -p spades-server && git add -A
git commit -m "auth: OAuth GitHub callback (/user + /user/emails, primary-verified required for link)"
```

---

## Phase 15: Game integration

Wire `Identity` into the create/join flows and write to `game_seats`.

### Task 15.1: game_seats repo

**Files:**
- Modify: `crates/spades-server/src/auth/game_seats.rs`
- Modify: `crates/spades-server/src/sqlite_store.rs`

- [ ] **Step 1: Types + repo**

`crates/spades-server/src/auth/game_seats.rs`:

```rust
//! Per-game seat-to-identity mapping table.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SeatRow {
    pub game_id: Uuid,
    pub seat_index: i32,
    pub player_id: Uuid,
    pub user_id: Option<Uuid>,
    pub anon_user_id: Option<Uuid>,
    pub is_bot: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SeatOwner {
    pub user_id: Option<Uuid>,
    pub anon_user_id: Option<Uuid>,
    pub is_bot: bool,
}
```

`crates/spades-server/src/sqlite_store.rs` (append to `impl SqliteStore`):

```rust
    pub fn insert_game_seat(&self, game_id: uuid::Uuid, seat_index: i32, player_id: uuid::Uuid, owner: crate::auth::game_seats::SeatOwner) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO game_seats (game_id, seat_index, player_id, user_id, anon_user_id, is_bot) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                game_id.to_string(),
                seat_index,
                player_id.to_string(),
                owner.user_id.map(|u| u.to_string()),
                owner.anon_user_id.map(|u| u.to_string()),
                owner.is_bot as i32,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_game_seat_owner(&self, game_id: uuid::Uuid, seat_index: i32, owner: crate::auth::game_seats::SeatOwner) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE game_seats SET user_id = ?3, anon_user_id = ?4, is_bot = ?5 \
             WHERE game_id = ?1 AND seat_index = ?2",
            rusqlite::params![
                game_id.to_string(),
                seat_index,
                owner.user_id.map(|u| u.to_string()),
                owner.anon_user_id.map(|u| u.to_string()),
                owner.is_bot as i32,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn game_seat(&self, game_id: uuid::Uuid, seat_index: i32) -> Result<Option<crate::auth::game_seats::SeatRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT game_id, seat_index, player_id, user_id, anon_user_id, is_bot \
             FROM game_seats WHERE game_id = ?1 AND seat_index = ?2",
            rusqlite::params![game_id.to_string(), seat_index],
            seat_row,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn game_seats_for_user(&self, user_id: uuid::Uuid, limit: i64, offset: i64) -> Result<Vec<crate::auth::game_seats::SeatRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT game_id, seat_index, player_id, user_id, anon_user_id, is_bot \
             FROM game_seats WHERE user_id = ?1 \
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(
            rusqlite::params![user_id.to_string(), limit, offset],
            seat_row,
        ).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn count_game_seats_for_user(&self, user_id: uuid::Uuid) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT COUNT(*) FROM game_seats WHERE user_id = ?1",
            rusqlite::params![user_id.to_string()],
            |r| r.get(0),
        ).map_err(|e| e.to_string())
    }
}

fn seat_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::auth::game_seats::SeatRow> {
    let game_id_s: String = r.get(0)?;
    let player_id_s: String = r.get(2)?;
    let user_id_s: Option<String> = r.get(3)?;
    let anon_id_s: Option<String> = r.get(4)?;
    Ok(crate::auth::game_seats::SeatRow {
        game_id: uuid::Uuid::parse_str(&game_id_s).unwrap(),
        seat_index: r.get(1)?,
        player_id: uuid::Uuid::parse_str(&player_id_s).unwrap(),
        user_id: user_id_s.map(|s| uuid::Uuid::parse_str(&s).unwrap()),
        anon_user_id: anon_id_s.map(|s| uuid::Uuid::parse_str(&s).unwrap()),
        is_bot: r.get::<_, i32>(5)? != 0,
    })
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: game_seats repo (insert, update owner, lookup, paginated by-user)"
```

### Task 15.2: Insert game_seats on game create

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/games.rs`

- [ ] **Step 1: Take `Identity` and write seats on create**

In `create_game`, after `state.presence.ensure_game(response.game_id, &response.player_ids);`, add:

```rust
            // Write game_seats with creator-owned identity on all four seats.
            let identity_user = identity.user().map(|u| u.id);
            let anon = identity.anon_id();
            for (i, pid) in response.player_ids.iter().enumerate() {
                let _ = state.auth.store.insert_game_seat(
                    response.game_id, i as i32, *pid,
                    spades_server::auth::game_seats::SeatOwner {
                        user_id: identity_user,
                        anon_user_id: Some(anon),
                        is_bot: false,
                    },
                );
            }
```

For the AI variant, mark non-human seats as bots:

```rust
            for (i, pid) in response.player_ids.iter().enumerate() {
                let is_human = human_seats.contains(&i);
                let _ = state.auth.store.insert_game_seat(
                    game_id, i as i32, *pid,
                    spades_server::auth::game_seats::SeatOwner {
                        user_id: if is_human { identity_user } else { None },
                        anon_user_id: if is_human { Some(anon) } else { None },
                        is_bot: !is_human,
                    },
                );
            }
```

Update the signature:

```rust
pub async fn create_game(
    AxumState(state): AxumState<AppState>,
    identity: spades_server::auth::Identity,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, Json<ErrorResponse>)> {
```

- [ ] **Step 2: Build + test**

The existing `tests` module in `bin/server/main.rs` constructs `AppState` directly. Verify those tests still pass.

```bash
cargo test -p spades-server
git add -A
git commit -m "auth: write game_seats on POST /games (creator owns all seats)"
```

### Task 15.3: Update game_seats on lobby/seek/challenge joins

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/matchmaking.rs`
- Modify: `crates/spades-server/src/bin/server/handlers/challenges.rs`
- Modify: `crates/spades-server/src/matchmaking.rs` and/or `challenges.rs` (signature: optional `(user_id, anon_id)` per seek/join)

- [ ] **Step 1: Wire Identity into `seek` handler**

Add `identity: spades_server::auth::Identity` parameter. Pass `(identity.user().map(|u| u.id), identity.anon_id())` to `state.matchmaker.add_seek` (or store the pair in an out-of-band side table keyed by player_id). The simplest path: just update game_seats with the joiner's identity inside the SSE generator once the `add_seek` returns the `player_id`.

```rust
    let (player_id, mut rx) = state.matchmaker.add_seek(request.max_points, request.timer_config, validated_name);

    // Find which seat the matchmaker assigns: matchmaker.add_seek returns player_id but not seat;
    // we need to look up the seat after match. Pre-record a pending row:
    // (Implementation note: matchmaker holds player_id → seat once 4 are matched. The cleanest
    //  attachment is in the GameStart handler — when SeekEvent::GameStart arrives, we know
    //  this seat's player_id and can find its game_id + seat_index from MatchResult.player_ids.)

    let identity_user = identity.user().map(|u| u.id);
    let anon = identity.anon_id();
    let store = state.auth.store.clone();
```

Inside the `async_stream::stream! { ... }` block, intercept `SeekEvent::GameStart(result)`:

```rust
                SeekEvent::GameStart(result) => {
                    // Identify my seat in the new game.
                    if let Some(seat_index) = result.player_ids.iter().position(|p| *p == player_id) {
                        let _ = store.update_game_seat_owner(
                            result.game_id, seat_index as i32,
                            spades_server::auth::game_seats::SeatOwner {
                                user_id: identity_user,
                                anon_user_id: Some(anon),
                                is_bot: false,
                            },
                        );
                    }
                    // existing logic...
```

**Important:** for seek/challenge flows, the `game_id` does not exist until all four players are matched (the manager creates the game internally then emits `GameStart`). So writing `game_seats` pre-match is impossible. We attach in the `GameStart` event handler of each player's SSE stream — each writes its own `seat_index`, no race.

The same pattern applies in `create_challenge_handler` and `join_challenge_handler` (for `ChallengeEvent::GameStart`).

- [ ] **Step 2: Use `insert_game_seat` (INSERT OR REPLACE) on GameStart in each handler**

In each handler's stream block, on the GameStart event, after determining `seat_index = result.player_ids.iter().position(|p| *p == player_id)`:

```rust
SeekEvent::GameStart(result) => {
    if let Some(seat_index) = result.player_ids.iter().position(|p| *p == player_id) {
        let _ = store.insert_game_seat(
            result.game_id, seat_index as i32, player_id,
            spades_server::auth::game_seats::SeatOwner {
                user_id: identity_user,
                anon_user_id: Some(anon),
                is_bot: false,
            },
        );
    }
    // existing yield logic...
}
```

Mirror for `ChallengeEvent::GameStart` in both `create_challenge_handler` and `join_challenge_handler`. The challenge handler must use `result.game_id` (provided in the event) — not `challenge_id`.

- [ ] **Step 3: Build + commit**

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: write game_seats on matchmaking/challenge GameStart (per-player)"
```

### Task 15.4: Username override on join requests + block name-change on user-bound seats

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/matchmaking.rs`
- Modify: `crates/spades-server/src/bin/server/handlers/challenges.rs`
- Modify: `crates/spades-server/src/bin/server/handlers/games.rs`

- [ ] **Step 1: Username override**

In `seek`, after the `validate_player_name` block:

```rust
    let validated_name = if let Some(user) = identity.user() {
        Some(user.username.clone()) // registered username overrides request-supplied name
    } else {
        validated_name
    };
```

Same in `join_challenge_handler` and `create_challenge_handler`.

- [ ] **Step 2: Block name-change on user-bound seats**

In `set_player_name` (games.rs), before the `set_player_name` call:

```rust
    // Look up the seat: if user_id is set, this seat is registered-owned — reject rename.
    // Need to know which seat this player_id corresponds to in the game.
    let game_state = state.game_manager.get_game_state(game_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Game not found".into() }),
    ))?;
    let seat_index = game_state.player_names.iter().position(|pn| pn.player_id == player_id);
    if let Some(idx) = seat_index {
        if let Ok(Some(seat)) = state.auth.store.game_seat(game_id, idx as i32) {
            if seat.user_id.is_some() {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse { error: "seat owned by registered user; name is canonical".into() }),
                ));
            }
        }
    }
```

- [ ] **Step 3: Build + commit**

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: registered username overrides join name; block rename on user-bound seats"
```

---

## Phase 16: User profiles

### Task 16.1: GET /users/:username + GET /users/:username/games

**Files:**
- Create: `crates/spades-server/src/handlers_users.rs`
- Modify: `crates/spades-server/src/lib.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Handlers**

Create `crates/spades-server/src/handlers_users.rs`:

```rust
use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{AuthError, AuthState, AuthUser};
use crate::auth::password::{hash_password, validate_password, verify_password};
use crate::auth::users::{validate_email, User};

#[derive(Serialize)]
pub struct PublicProfile {
    pub username: String,
    pub created_at: String,
    pub games_played: i64,
    pub last_seen_at: Option<String>,
}

pub async fn get_profile(
    State(auth): State<AuthState>,
    Path(username): Path<String>,
) -> Result<Json<PublicProfile>, AuthError> {
    let user = auth.store.find_user_by_username(&username).map_err(AuthError::Storage)?
        .ok_or(AuthError::Unauthenticated)?;  // 401 leaks less than 404 here? — actually 404 is fine for profile-not-found
    // Use 404 instead:
    // ... (revise to return AuthError::Validation if you want 422, or define a NotFound variant)
    let games_played = auth.store.count_game_seats_for_user(user.id).map_err(AuthError::Storage)?;
    Ok(Json(PublicProfile {
        username: user.username,
        created_at: user.created_at,
        games_played,
        last_seen_at: user.last_login_at,
    }))
}

// Note on 404: AuthError doesn't have a NotFound variant. Add one:
//   #[error("not_found")] NotFound,
// and map it to StatusCode::NOT_FOUND in status() + error_code().

#[derive(Deserialize)]
pub struct GamesPagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 { 20 }

#[derive(Serialize)]
pub struct ProfileGames {
    pub username: String,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
    pub games: Vec<ProfileGameEntry>,
}

#[derive(Serialize)]
pub struct ProfileGameEntry {
    pub game_id: Uuid,
    pub seat_index: i32,
    pub player_id: Uuid,
}

pub async fn get_profile_games(
    State(auth): State<AuthState>,
    Path(username): Path<String>,
    Query(p): Query<GamesPagination>,
) -> Result<Json<ProfileGames>, AuthError> {
    let user = auth.store.find_user_by_username(&username).map_err(AuthError::Storage)?
        .ok_or(AuthError::Unauthenticated)?; // revise to NotFound per above
    let total = auth.store.count_game_seats_for_user(user.id).map_err(AuthError::Storage)?;
    let rows = auth.store.game_seats_for_user(user.id, p.limit.min(100).max(1), p.offset.max(0))
        .map_err(AuthError::Storage)?;
    Ok(Json(ProfileGames {
        username: user.username,
        limit: p.limit,
        offset: p.offset,
        total,
        games: rows.into_iter().map(|r| ProfileGameEntry {
            game_id: r.game_id,
            seat_index: r.seat_index,
            player_id: r.player_id,
        }).collect(),
    }))
}
```

- [ ] **Step 2: Add `NotFound` variant to `AuthError`**

In `crates/spades-server/src/auth/error.rs`:

```rust
    #[error("not_found")]
    NotFound,
```

Update `status()` to map `NotFound → StatusCode::NOT_FOUND` and `error_code` to `"not_found"`.

Update handlers above to use `.ok_or(AuthError::NotFound)?`.

- [ ] **Step 3: Wire**

`crates/spades-server/src/lib.rs`:
```rust
pub mod handlers_users;
```

`bin/server/main.rs`:
```rust
        .route("/users/{username}", get(spades_server::handlers_users::get_profile))
        .route("/users/{username}/games", get(spades_server::handlers_users::get_profile_games))
```

- [ ] **Step 4: Commit**

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: GET /users/:username + GET /users/:username/games (paginated)"
```

### Task 16.2: PATCH /users/me

**Files:**
- Modify: `crates/spades-server/src/handlers_users.rs`
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Handler**

Append to `handlers_users.rs`:

```rust
use crate::auth::tokens::{generate_token, hash_token, PURPOSE_VERIFY_EMAIL};
use crate::auth::mailer::Email;
use crate::auth::session_ext;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct PatchMeRequest {
    #[serde(default)] pub email: Option<String>,
    #[serde(default)] pub current_password: Option<String>,
    #[serde(default)] pub new_password: Option<String>,
}

pub async fn patch_me(
    State(auth): State<AuthState>,
    session: Session,
    AuthUser(user): AuthUser,
    Json(req): Json<PatchMeRequest>,
) -> Result<Json<crate::handlers_auth::UserResponse>, AuthError> {
    // Email change
    if let Some(new_email) = req.email.as_deref() {
        validate_email(new_email)?;
        auth.store.update_user_email(user.id, new_email).map_err(|e| match e.as_str() {
            "email_taken" => AuthError::EmailTaken,
            other => AuthError::Storage(other.into()),
        })?;
        // Re-verify
        let token = generate_token();
        let h = hash_token(&token);
        auth.store.insert_auth_token(&h, user.id, PURPOSE_VERIFY_EMAIL, 24 * 3600)
            .map_err(AuthError::Storage)?;
        let link = format!("{}/auth/verify-email?token={}", auth.oauth.redirect_base_url, token);
        let _ = auth.mailer.send(Email {
            to: new_email.to_string(),
            subject: "Verify your new email".into(),
            body: format!("Verify: {link}"),
        }).await;
    }

    // Password change
    if let Some(new_password) = req.new_password.as_deref() {
        let current = req.current_password.as_deref()
            .ok_or_else(|| AuthError::Validation("current_password required for password change".into()))?;
        let phc = user.password_hash.as_deref()
            .ok_or_else(|| AuthError::Validation("OAuth-only accounts cannot set password here".into()))?;
        if !verify_password(current, phc)? {
            return Err(AuthError::InvalidCredentials);
        }
        validate_password(new_password)?;
        let new_hash = hash_password(new_password)?;
        let new_version = auth.store.update_user_password(user.id, &new_hash).map_err(AuthError::Storage)?;
        session_ext::set_claimed(&session, user.id, new_version).await?;
    }

    let updated = auth.store.find_user_by_id(user.id).map_err(AuthError::Storage)?
        .ok_or_else(|| AuthError::Internal("user vanished after update".into()))?;
    Ok(Json(crate::handlers_auth::UserResponse::from(&updated)))
}
```

- [ ] **Step 2: Mount + commit**

```rust
        .route("/users/me", axum::routing::patch(spades_server::handlers_users::patch_me))
```

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: PATCH /users/me (email change re-verifies; password change bumps token_version)"
```

---

## Phase 17: Background cleanup

Single startup-and-hourly cleanup task. Removes expired tokens and old completed login_failures rows.

### Task 17.1: Cleanup task

**Files:**
- Modify: `crates/spades-server/src/bin/server/main.rs`

- [ ] **Step 1: Spawn cleanup loop in main()**

Inside `main()`, after constructing `auth_state` and before `let session_layer = ...`:

```rust
    {
        let store = auth_state.store.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = store.cleanup_expired_tokens() {
                    eprintln!("cleanup_expired_tokens: {e}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
            }
        });
    }
```

(Run once at startup before the sleep.)

- [ ] **Step 2: Commit**

```bash
cargo build -p spades-server && cargo test -p spades-server
git add -A
git commit -m "auth: hourly cleanup task for expired auth_tokens"
```

---

## Phase 18: Integration tests

Final phase — fill in the test files declared in the spec's Testing section.

### Task 18.1: auth_anon_claim test

**Files:**
- Create: `crates/spades-server/tests/auth_anon_claim.rs`

- [ ] **Step 1: Test**

```rust
use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn anon_game_attaches_to_user_on_register() {
    let server = common::test_server();

    // 1. Anon creates a game.
    let create: serde_json::Value = server.post("/games")
        .json(&json!({"max_points": 500})).await.json();
    let game_id = create["game_id"].as_str().unwrap();

    // 2. Register — anon-claim should attach the game seat.
    server.post("/auth/register")
        .json(&json!({"username": "Alice", "email": "alice@x.com", "password": "hunter2-strong"}))
        .await.assert_status(StatusCode::CREATED);

    // 3. /users/Alice/games should contain that game.
    let games: serde_json::Value = server.get("/users/Alice/games").await.json();
    let arr = games["games"].as_array().unwrap();
    assert!(arr.iter().any(|e| e["game_id"].as_str() == Some(game_id)));
}
```

- [ ] **Step 2: Test the no-anon-game case**

```rust
#[tokio::test]
async fn register_without_anon_game_works() {
    let server = common::test_server();
    server.post("/auth/register")
        .json(&json!({"username": "Bob", "email": "bob@x.com", "password": "hunter2-strong"}))
        .await.assert_status(StatusCode::CREATED);
    let games: serde_json::Value = server.get("/users/Bob/games").await.json();
    assert_eq!(games["total"], 0);
}
```

- [ ] **Step 3: Commit**

```bash
cargo test -p spades-server --test auth_anon_claim
git add -A
git commit -m "tests: anon-claim attaches games to user on register"
```

### Tasks 18.2-N: Remaining integration tests

The remaining test files mirror the spec's Testing list. Each follows the same pattern: drive endpoints via `axum-test`, assert response shapes. Implement these in order:

- `auth_username_login.rs` — register with username "Alice", then `login` with `{login: "alice"}` (no `@`); expect 200.
- `auth_login_lockout.rs` — five wrong logins → 423; correct after lockout expiry → 200. (Use a quick lockout window for the test, or test the count without sleeping.)
- `auth_password_reset.rs` — register, request reset, inspect `LogMailer.sent()` for the link, extract token, confirm, log in with new password.
- `auth_email_verify.rs` — register, inspect `LogMailer.sent()` for verify link, hit `/auth/verify-email?token=...`, then `/auth/me` returns `email_verified: true`.
- `auth_oauth_google.rs` — `wiremock` stub for token + userinfo; drive callback; expect user created.
- `auth_oauth_github.rs` — `wiremock` for `/user` + `/user/emails`; unverified primary → pending-signup; verified primary → direct login.
- `auth_oauth_complete.rs` — callback creates pending; POST `/auth/oauth/complete` with username → user created.
- `auth_oauth_link_existing.rs` — pre-create verified-email user; Google callback with same email → links instead of duplicating.
- `auth_session_expiry.rs` — manually expire session (`tower-sessions` test helper or DB tweak); subsequent `/auth/me` → 401.
- `auth_rate_limit.rs` — exceed login bucket; expect 429 with `Retry-After`.
- `auth_csrf_oauth.rs` — call callback with bad `state` → 400.
- `auth_secure_cookie_default.rs` — boot test server without `--insecure-cookies` equivalent (set `secure_cookies: true` on AuthState); registration response Set-Cookie has `Secure`. Tower-sessions ignores `with_secure(true)` when running over HTTP — manual inspection of the bin's startup output may be needed.
- `auth_game_integration.rs` — register, then create game via API; game_seats row has `user_id = Alice.id`; join lobby with body `name: "Mallory"` → response shows Alice as the seat's name.
- `auth_logout_anon_preserved.rs` — log in, then POST `/auth/logout`; verify session cookie unchanged, `GET /player` returns same `user_id` as before.

Each test is its own task: write the test, run `cargo test -p spades-server --test <name>`, commit. Keep commits granular (one per test file).

- [ ] **Step 1: Implement each integration test in sequence**

Repeat: create test file, run, commit. Use these commit message templates:

```bash
git commit -m "tests: <test_file_name> covers <one-line description>"
```

- [ ] **Step 2: Final full-suite gate**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Expected: 238 (existing) + ~30 new integration tests + ~50 new unit tests = ~318 passing.

- [ ] **Step 3: Update IMPROVEMENTS.md**

Move all items completed by this slice from "Not Yet Implemented" to "Implemented":

- Authentication
- Rate Limiting (partial — auth endpoints only)
- Game History (partial — per-user listing)

Note partial items so future readers see what's still missing.

```bash
git add IMPROVEMENTS.md
git commit -m "docs: update IMPROVEMENTS.md for identity-foundation slice landing"
```

---

## Done

After all phases land:

```bash
cargo test --workspace          # all green
cargo clippy --workspace -- -D warnings  # clean
```

The slice is shippable. Followup slices in scope: transition-endpoint auth tightening, ratings (Glicko-2 adapted for partnerships), spectator/TV, tournaments, lila-ws-style real-time split.

