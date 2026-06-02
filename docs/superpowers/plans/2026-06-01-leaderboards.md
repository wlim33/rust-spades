# Leaderboards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add all-time and monthly leaderboards that rank players by a conservative Glicko-2 score (`rating − 2·RD`), showing the top 10 eligible players.

**Architecture:** A pure-SQL ranking query over the existing `users` and `game_seats` tables — no new schema, migration, or background jobs. A "season" is a monthly activity filter that re-ranks current ratings; it is not a separate rating track. The backend exposes `GET /leaderboard?period=…`; the frontend adds a `/leaderboard` page with All-time / This-month tabs.

**Tech Stack:** Rust (axum 0.8, rusqlite, chrono), TypeScript (lit-html, `@preact/signals-core`, vitest).

---

## Background for the implementer

You are working in a Cargo workspace (`crates/spades-core`, `crates/spades-server`) plus a Vite/TypeScript frontend in `web/`.

Key existing facts you must rely on:

- **Ratings** live on the `users` table: `rating REAL DEFAULT 1500.0`, `rd REAL DEFAULT 350.0`, `volatility REAL DEFAULT 0.06`. The `Rating` struct and `DEFAULT_RATING` are in `crates/spades-server/src/ratings.rs`.
- **`game_seats`** (`game_id, seat_index, player_id, user_id, anon_user_id, is_bot, created_at`) links users to games. `user_id` is `NULL` for anon/bot seats. `created_at` defaults to `datetime('now')`, formatted `YYYY-MM-DD HH:MM:SS` (UTC) — this is the activity timestamp.
- **Existing profile endpoint** `GET /users/{username}` (`crates/spades-server/src/handlers_users.rs`) is the pattern to mirror: it uses `State(AuthState)`, returns `Result<Json<…>, AuthError>`, and rounds rating/rd to `i32` for display.
- **`AuthError`** (`crates/spades-server/src/auth/error.rs`): `AuthError::Storage(String)` → 500; `AuthError::Validation(String)` → **422** (UNPROCESSABLE_ENTITY). We use `Validation` for a malformed `period` (the codebase's convention for bad input; the spec said "400" loosely — follow the codebase and use 422).
- **Frontend route pattern**: `web/src/routes/profile.ts` (signals + `effect` + lit-html wrapped in `appShell`). Routes are registered in `web/src/main.ts`. The nav lives in `web/src/ui/components/header.ts`. Profile-style endpoints use **hand-written** client types in `web/src/state/user-types.ts` and the `request<T>()` helper from `web/src/api/client.ts` (not the generated OpenAPI schema).
- **Important lit-html quirk** (see `profile.ts`): read all signal `.value`s into locals at the top of the `effect` callback *before* building the template, to avoid a happy-dom/lit-html nested-conditional re-render bug.

### Commands

`cargo` may not be on PATH from the tool shell. Prefix Rust commands with:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

- Run one Rust test: `cargo test -p spades-server <test_name>`
- Run all Rust tests: `cargo test --workspace`
- Format Rust: `cargo fmt`
- Run frontend component tests: `pnpm -C web test:component`
- Run a single frontend test file: `pnpm -C web exec vitest run --project=component tests/component/leaderboard.spec.ts`

### Commit hygiene

The repo sometimes has unrelated work staged. **Always commit with explicit pathspecs** (`git add <specific files>` / `git commit -- <files>`), never `git add -A` / `git add .`.

---

## File structure

**Backend (new):**
- `crates/spades-server/src/leaderboard.rs` — domain module: constants (`MIN_GAMES`, `LEADERBOARD_SIZE`, `RD_CONSERVATISM`), `LeaderboardWindow` enum, `LeaderboardRow` struct, `month_bounds()` helper. No axum/HTTP here.
- `crates/spades-server/src/handlers_leaderboard.rs` — HTTP layer: `LeaderboardQuery`, `LeaderboardEntry`, `LeaderboardResponse` DTOs, `parse_period()`, `get_leaderboard()` handler.
- `crates/spades-server/tests/leaderboard.rs` — integration tests.

**Backend (modified):**
- `crates/spades-server/src/lib.rs` — declare the two new modules.
- `crates/spades-server/src/sqlite_store.rs` — add `SqliteStore::leaderboard()` + store unit tests.
- `crates/spades-server/src/bin/server/main.rs` — register the `/leaderboard` route.
- `crates/spades-server/tests/common.rs` — register the `/leaderboard` route on the shared test router.

**Frontend (new):**
- `web/src/routes/leaderboard.ts` — the leaderboard page route module.
- `web/tests/component/leaderboard.spec.ts` — component test.

**Frontend (modified):**
- `web/src/state/user-types.ts` — `LeaderboardPeriod`, `LeaderboardEntry`, `Leaderboard` types.
- `web/src/main.ts` — import and register `/leaderboard`.
- `web/src/ui/components/header.ts` — add a Leaderboard nav link.
- `web/src/ui/design.css` — minimal leaderboard styling.

---

## Task 1: Leaderboard domain module (constants, types, `month_bounds`)

**Files:**
- Create: `crates/spades-server/src/leaderboard.rs`
- Modify: `crates/spades-server/src/lib.rs` (add `pub mod leaderboard;`)

- [ ] **Step 1: Declare the module**

In `crates/spades-server/src/lib.rs`, add this single line in alphabetical position among the `pub mod` declarations — after `pub mod handlers_users;` and before `pub mod lock_util;`:

```rust
pub mod leaderboard;
```

(Declare only `leaderboard` here. The `handlers_leaderboard` module is created and declared in Task 3 — declaring it now would leave the crate uncompilable, since its file doesn't exist yet.)

- [ ] **Step 2: Write the failing test for `month_bounds`**

Create `crates/spades-server/src/leaderboard.rs` with the full module below (types + helper + test). Write it complete in one go since the pieces are interdependent:

```rust
//! Leaderboard domain types and helpers.
//!
//! Pure data + a date helper — no HTTP, no axum. The SQL ranking lives in
//! `sqlite_store::SqliteStore::leaderboard`; the HTTP surface lives in
//! `handlers_leaderboard`.

/// Minimum number of games (game-seat rows) a player must have to appear
/// on any board. Conservative scoring already sinks unproven players; this
/// gate keeps the tail tidy.
pub const MIN_GAMES: i64 = 5;

/// Maximum number of rows any board returns.
pub const LEADERBOARD_SIZE: i64 = 10;

/// Conservatism multiplier `k` in the ranking score `rating - k * rd`.
/// k = 2 is the standard Glicko "lower-confidence-bound" choice.
pub const RD_CONSERVATISM: f64 = 2.0;

/// Which players a board covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderboardWindow {
    /// All players, ranked by current rating.
    AllTime,
    /// Players who have a game-seat in the given UTC calendar month,
    /// ranked by current rating. `month` is 1-based (1 = January).
    Month { year: i32, month: u32 },
}

/// One ranked player as returned by the store. Display rounding happens in
/// the HTTP layer.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardRow {
    pub username: String,
    pub rating: f64,
    pub rd: f64,
    /// All-time game-seat count (matches the profile's "games_played").
    pub games_played: i64,
    /// `rating - RD_CONSERVATISM * rd`.
    pub score: f64,
}

/// Inclusive-start / exclusive-end timestamp bounds for the given UTC
/// calendar month, formatted to match `game_seats.created_at`
/// (`YYYY-MM-DD HH:MM:SS`). Lexicographic string comparison on this fixed
/// format is equivalent to chronological comparison.
pub fn month_bounds(year: i32, month: u32) -> (String, String) {
    use chrono::NaiveDate;
    let start = NaiveDate::from_ymd_opt(year, month, 1)
        .expect("valid year/month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid year/month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    let fmt = "%Y-%m-%d %H:%M:%S";
    (start.format(fmt).to_string(), end.format(fmt).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_basic_and_year_rollover() {
        assert_eq!(
            month_bounds(2026, 6),
            (
                "2026-06-01 00:00:00".to_string(),
                "2026-07-01 00:00:00".to_string()
            )
        );
        // December rolls into the next January.
        assert_eq!(
            month_bounds(2026, 12),
            (
                "2026-12-01 00:00:00".to_string(),
                "2027-01-01 00:00:00".to_string()
            )
        );
    }
}
```

- [ ] **Step 3: Run the test to verify it passes (compiles + correct)**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades-server month_bounds_basic_and_year_rollover
```
Expected: PASS (1 test). This also confirms the module compiles and `chrono` is available (it is — already a dependency).

- [ ] **Step 4: Format and commit**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt
git add crates/spades-server/src/leaderboard.rs crates/spades-server/src/lib.rs
git commit -m "feat(leaderboard): domain module — constants, window, month_bounds"
```

---

## Task 2: `SqliteStore::leaderboard()` ranking query

**Files:**
- Modify: `crates/spades-server/src/sqlite_store.rs` (add method after `count_game_seats_for_user`, ~line 696; add tests in the `mod tests` block)

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `#[cfg(test)] mod tests { … }` block at the bottom of `crates/spades-server/src/sqlite_store.rs` (after the `game_seat_crud_and_lookups` test). They reference a small `seed_seats` helper defined alongside them:

```rust
    /// Insert `n` distinct game-seats owned by `user` (seat 0 of `n`
    /// different games). Used to push a user past the MIN_GAMES gate.
    fn seed_seats(store: &SqliteStore, user: Uuid, n: usize) {
        for _ in 0..n {
            store
                .insert_game_seat(
                    Uuid::new_v4(),
                    0,
                    Uuid::new_v4(),
                    crate::auth::game_seats::SeatOwner {
                        user_id: Some(user),
                        anon_user_id: None,
                        is_bot: false,
                    },
                )
                .unwrap();
        }
    }

    #[test]
    fn leaderboard_ranks_by_conservative_score() {
        use crate::leaderboard::LeaderboardWindow;
        use crate::ratings::Rating;
        let store = SqliteStore::open(":memory:").unwrap();
        let high_rd = store.insert_user(&new_user("highrd", "h@x.com")).unwrap();
        let low_rd = store.insert_user(&new_user("lowrd", "l@x.com")).unwrap();
        seed_seats(&store, high_rd, 5);
        seed_seats(&store, low_rd, 5);
        // high_rd: 1800 - 2*300 = 1200
        store
            .set_user_rating(
                high_rd,
                &Rating { rating: 1800.0, rd: 300.0, volatility: 0.06 },
            )
            .unwrap();
        // low_rd: 1600 - 2*50 = 1500  → ranks ABOVE high_rd despite lower raw rating
        store
            .set_user_rating(
                low_rd,
                &Rating { rating: 1600.0, rd: 50.0, volatility: 0.06 },
            )
            .unwrap();

        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].username, "lowrd");
        assert_eq!(rows[1].username, "highrd");
        assert!((rows[0].score - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn leaderboard_excludes_players_below_min_games() {
        use crate::leaderboard::LeaderboardWindow;
        let store = SqliteStore::open(":memory:").unwrap();
        let vet = store.insert_user(&new_user("vet", "v@x.com")).unwrap();
        let rook = store.insert_user(&new_user("rook", "r@x.com")).unwrap();
        seed_seats(&store, vet, 5);
        seed_seats(&store, rook, 4); // below the gate
        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].username, "vet");
    }

    #[test]
    fn leaderboard_caps_at_limit() {
        use crate::leaderboard::LeaderboardWindow;
        use crate::ratings::Rating;
        let store = SqliteStore::open(":memory:").unwrap();
        for i in 0..12 {
            let u = store
                .insert_user(&new_user(&format!("p{i}"), &format!("p{i}@x.com")))
                .unwrap();
            seed_seats(&store, u, 5);
            store
                .set_user_rating(
                    u,
                    &Rating { rating: 1500.0 + i as f64, rd: 50.0, volatility: 0.06 },
                )
                .unwrap();
        }
        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 10, "top-10 cap");
        assert_eq!(rows[0].username, "p11", "highest score first");
    }

    #[test]
    fn leaderboard_excludes_anon_and_bot_seats() {
        use crate::leaderboard::LeaderboardWindow;
        let store = SqliteStore::open(":memory:").unwrap();
        let real = store.insert_user(&new_user("real", "r@x.com")).unwrap();
        seed_seats(&store, real, 5);
        for _ in 0..5 {
            store
                .insert_game_seat(
                    Uuid::new_v4(),
                    0,
                    Uuid::new_v4(),
                    crate::auth::game_seats::SeatOwner {
                        user_id: None,
                        anon_user_id: Some(Uuid::new_v4()),
                        is_bot: false,
                    },
                )
                .unwrap();
        }
        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].username, "real");
    }

    #[test]
    fn leaderboard_month_window_filters_by_activity() {
        use crate::leaderboard::LeaderboardWindow;
        use chrono::Datelike;
        let store = SqliteStore::open(":memory:").unwrap();
        let active = store.insert_user(&new_user("active", "a@x.com")).unwrap();
        let inactive = store.insert_user(&new_user("inactive", "i@x.com")).unwrap();
        seed_seats(&store, active, 5); // created now → current month
        seed_seats(&store, inactive, 5);
        // Backdate inactive's seats to January 2020.
        store
            .with_tx(|conn| {
                conn.execute(
                    "UPDATE game_seats SET created_at = '2020-01-15 12:00:00' WHERE user_id = ?1",
                    rusqlite::params![inactive.to_string()],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
            .unwrap();

        // January 2020 board: only the backdated player.
        let jan = store
            .leaderboard(LeaderboardWindow::Month { year: 2020, month: 1 }, 5, 10)
            .unwrap();
        assert_eq!(jan.len(), 1);
        assert_eq!(jan[0].username, "inactive");

        // Current month board: only the player active now.
        let now = chrono::Utc::now().naive_utc();
        let cur = store
            .leaderboard(
                LeaderboardWindow::Month { year: now.year(), month: now.month() },
                5,
                10,
            )
            .unwrap();
        assert!(cur.iter().any(|r| r.username == "active"));
        assert!(!cur.iter().any(|r| r.username == "inactive"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades-server leaderboard_ranks_by_conservative_score
```
Expected: FAIL — compile error `no method named 'leaderboard' found for struct SqliteStore`.

- [ ] **Step 3: Implement `SqliteStore::leaderboard()`**

Add this method to the `impl SqliteStore` block, immediately after `count_game_seats_for_user` (around line 696, before `with_tx`):

```rust
    /// Top players for a board, ranked by the conservative Glicko score
    /// `rating - RD_CONSERVATISM * rd` (descending; raw rating breaks ties).
    ///
    /// Only users with a `game_seats` row are considered (the JOIN), so
    /// anon/bot seats (`user_id IS NULL`) never appear. `min_games` gates on
    /// the all-time seat count. For `Month`, an extra `EXISTS` requires at
    /// least one seat in that UTC month; the score still uses the user's
    /// current rating (we keep no rating history).
    pub fn leaderboard(
        &self,
        window: crate::leaderboard::LeaderboardWindow,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<crate::leaderboard::LeaderboardRow>, String> {
        use crate::leaderboard::{LeaderboardRow, LeaderboardWindow, RD_CONSERVATISM, month_bounds};
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        let (month_clause, bounds) = match window {
            LeaderboardWindow::AllTime => (String::new(), None),
            LeaderboardWindow::Month { year, month } => (
                "AND EXISTS (SELECT 1 FROM game_seats m \
                 WHERE m.user_id = u.id AND m.created_at >= ?2 AND m.created_at < ?3)"
                    .to_string(),
                Some(month_bounds(year, month)),
            ),
        };

        // RD_CONSERVATISM and limit are compile-time numerics, not user
        // input, so interpolating them into the SQL is safe.
        let sql = format!(
            "SELECT u.username, u.rating, u.rd, COUNT(gs.game_id) AS games \
             FROM users u \
             JOIN game_seats gs ON gs.user_id = u.id \
             WHERE 1 = 1 {month_clause} \
             GROUP BY u.id \
             HAVING games >= ?1 \
             ORDER BY (u.rating - {RD_CONSERVATISM} * u.rd) DESC, u.rating DESC \
             LIMIT {limit}"
        );

        let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<LeaderboardRow> {
            let rating: f64 = r.get(1)?;
            let rd: f64 = r.get(2)?;
            Ok(LeaderboardRow {
                username: r.get(0)?,
                rating,
                rd,
                games_played: r.get(3)?,
                score: rating - RD_CONSERVATISM * rd,
            })
        };

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = match &bounds {
            None => stmt
                .query_map(rusqlite::params![min_games], map_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>(),
            Some((start, end)) => stmt
                .query_map(rusqlite::params![min_games, start, end], map_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>(),
        };
        rows.map_err(|e| e.to_string())
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades-server leaderboard_
```
Expected: PASS — 5 tests (`leaderboard_ranks_by_conservative_score`, `leaderboard_excludes_players_below_min_games`, `leaderboard_caps_at_limit`, `leaderboard_excludes_anon_and_bot_seats`, `leaderboard_month_window_filters_by_activity`).

- [ ] **Step 5: Format and commit**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt
git add crates/spades-server/src/sqlite_store.rs
git commit -m "feat(leaderboard): conservative-score ranking query in the store"
```

---

## Task 3: HTTP handler + route wiring

**Files:**
- Create: `crates/spades-server/src/handlers_leaderboard.rs`
- Modify: `crates/spades-server/src/lib.rs` (declare `pub mod handlers_leaderboard;`)
- Modify: `crates/spades-server/src/bin/server/main.rs` (register route)
- Modify: `crates/spades-server/tests/common.rs` (register route on test router)
- Create: `crates/spades-server/tests/leaderboard.rs` (integration tests)

- [ ] **Step 1: Declare the module**

In `crates/spades-server/src/lib.rs`, add (alphabetically, right before `pub mod handlers_users;`):

```rust
pub mod handlers_leaderboard;
```

- [ ] **Step 2: Write the failing integration tests**

Create `crates/spades-server/tests/leaderboard.rs`:

```rust
use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn leaderboard_empty_when_no_eligible_players() {
    let server = common::test_server();
    let resp = server.get("/leaderboard").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["period"], "all-time");
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn leaderboard_invalid_period_is_rejected() {
    let server = common::test_server();
    let resp = server.get("/leaderboard?period=yesterday").await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn leaderboard_this_month_period_echoed() {
    let server = common::test_server();
    let resp = server.get("/leaderboard?period=this-month").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    // period echoes the resolved YYYY-MM (e.g. "2026-06").
    let period = body["period"].as_str().unwrap();
    assert_eq!(period.len(), 7, "period should look like YYYY-MM, got {period}");
    assert_eq!(&period[4..5], "-");
}

#[tokio::test]
async fn leaderboard_lists_eligible_player() {
    let env = common::test_env();
    let reg: serde_json::Value = env
        .server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .json();
    let uid = uuid::Uuid::parse_str(reg["id"].as_str().unwrap()).unwrap();
    for _ in 0..5 {
        env.store
            .insert_game_seat(
                uuid::Uuid::new_v4(),
                0,
                uuid::Uuid::new_v4(),
                spades_server::auth::game_seats::SeatOwner {
                    user_id: Some(uid),
                    anon_user_id: None,
                    is_bot: false,
                },
            )
            .unwrap();
    }
    let resp = env.server.get("/leaderboard").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["username"], "Alice");
    assert_eq!(entries[0]["rank"], 1);
    // Default-rated player: 1500 - 2*350 = 800.
    assert_eq!(entries[0]["score"], 800);
}
```

- [ ] **Step 3: Run to verify they fail**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades-server --test leaderboard
```
Expected: FAIL — compile error (`handlers_leaderboard` not found / route missing). That's expected until Steps 4–6.

- [ ] **Step 4: Implement the handler module**

Create `crates/spades-server/src/handlers_leaderboard.rs`:

```rust
//! `GET /leaderboard` — top players by conservative Glicko score.
//!
//! Mirrors the `handlers_users` pattern: `State(AuthState)`, hand-written
//! DTOs, `Result<Json<…>, AuthError>`. Ranking math lives in the store; the
//! board's domain types/constants live in `crate::leaderboard`.

use axum::extract::{Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthError, AuthState};
use crate::leaderboard::{LEADERBOARD_SIZE, LeaderboardWindow, MIN_GAMES};

#[derive(Deserialize)]
pub struct LeaderboardQuery {
    /// `all-time` (default), `this-month`, or an explicit `YYYY-MM`.
    #[serde(default)]
    pub period: Option<String>,
}

#[derive(Serialize)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub username: String,
    pub rating: i32,
    pub rd: i32,
    pub games_played: i64,
    pub score: i32,
}

#[derive(Serialize)]
pub struct LeaderboardResponse {
    /// The resolved period label: `all-time` or `YYYY-MM`.
    pub period: String,
    pub entries: Vec<LeaderboardEntry>,
}

/// Resolve the `period` query param into a label + window. Returns
/// `AuthError::Validation` (422) for anything malformed.
fn parse_period(period: Option<&str>) -> Result<(String, LeaderboardWindow), AuthError> {
    use chrono::Datelike;
    match period {
        None | Some("all-time") => Ok(("all-time".to_string(), LeaderboardWindow::AllTime)),
        Some("this-month") => {
            let now = chrono::Utc::now().naive_utc();
            let (year, month) = (now.year(), now.month());
            Ok((
                format!("{year:04}-{month:02}"),
                LeaderboardWindow::Month { year, month },
            ))
        }
        Some(s) => {
            // Explicit YYYY-MM.
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() == 2 && parts[0].len() == 4 {
                if let (Ok(year), Ok(month)) =
                    (parts[0].parse::<i32>(), parts[1].parse::<u32>())
                {
                    if (1..=12).contains(&month) {
                        return Ok((
                            format!("{year:04}-{month:02}"),
                            LeaderboardWindow::Month { year, month },
                        ));
                    }
                }
            }
            Err(AuthError::Validation(format!("invalid period: {s}")))
        }
    }
}

pub async fn get_leaderboard(
    State(auth): State<AuthState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>, AuthError> {
    let (period, window) = parse_period(q.period.as_deref())?;
    let rows = auth
        .store
        .leaderboard(window, MIN_GAMES, LEADERBOARD_SIZE)
        .map_err(AuthError::Storage)?;
    let entries = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| LeaderboardEntry {
            rank: i as i64 + 1,
            username: r.username,
            rating: r.rating.round() as i32,
            rd: r.rd.round() as i32,
            games_played: r.games_played,
            score: r.score.round() as i32,
        })
        .collect();
    Ok(Json(LeaderboardResponse { period, entries }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_period_defaults_and_validates() {
        assert!(matches!(
            parse_period(None).unwrap().1,
            LeaderboardWindow::AllTime
        ));
        assert!(matches!(
            parse_period(Some("all-time")).unwrap().1,
            LeaderboardWindow::AllTime
        ));
        let (label, win) = parse_period(Some("2020-01")).unwrap();
        assert_eq!(label, "2020-01");
        assert_eq!(win, LeaderboardWindow::Month { year: 2020, month: 1 });
        // this-month resolves to a YYYY-MM label.
        assert_eq!(parse_period(Some("this-month")).unwrap().0.len(), 7);
        // Malformed inputs are rejected.
        assert!(parse_period(Some("yesterday")).is_err());
        assert!(parse_period(Some("2020-13")).is_err());
        assert!(parse_period(Some("20-01")).is_err());
    }
}
```

- [ ] **Step 5: Register the route on the production router**

In `crates/spades-server/src/bin/server/main.rs`, find the user-profile route block (the `.route("/users/{username}/games", …)` line, ~line 188). Immediately after it, add:

```rust
        .route(
            "/leaderboard",
            get(spades_server::handlers_leaderboard::get_leaderboard),
        )
```

(`get` is already imported in this file — it's used by the surrounding routes.)

- [ ] **Step 6: Register the route on the shared test router**

In `crates/spades-server/tests/common.rs`, find the `.route("/users/{username}/games", …)` line in the router builder. Immediately after it, add:

```rust
        .route(
            "/leaderboard",
            get(spades_server::handlers_leaderboard::get_leaderboard),
        )
```

(`get` is already imported at the top of `common.rs`.)

- [ ] **Step 7: Run the unit + integration tests**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades-server parse_period_defaults_and_validates
cargo test -p spades-server --test leaderboard
```
Expected: PASS — the unit test, plus all 4 integration tests.

- [ ] **Step 8: Run the full workspace test suite (regression check)**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --workspace
```
Expected: PASS — everything green, including pre-existing tests.

- [ ] **Step 9: Format and commit**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt
git add crates/spades-server/src/handlers_leaderboard.rs crates/spades-server/src/lib.rs \
        crates/spades-server/src/bin/server/main.rs crates/spades-server/tests/common.rs \
        crates/spades-server/tests/leaderboard.rs
git commit -m "feat(leaderboard): GET /leaderboard handler, route, integration tests"
```

---

## Task 4: Frontend client types

**Files:**
- Modify: `web/src/state/user-types.ts`

- [ ] **Step 1: Add the types**

Append to `web/src/state/user-types.ts`:

```ts
export type LeaderboardPeriod = 'all-time' | 'this-month';

export type LeaderboardEntry = {
  rank: number;
  username: string;
  rating: number;
  rd: number;
  games_played: number;
  score: number;
};

export type Leaderboard = {
  period: string;
  entries: LeaderboardEntry[];
};
```

- [ ] **Step 2: Typecheck**

Run:
```bash
pnpm -C web exec tsc -p tsconfig.json --noEmit
```
Expected: PASS (no type errors). New types are unused for now — that's fine, `tsc` won't error on unused exports.

- [ ] **Step 3: Commit**

```bash
git add web/src/state/user-types.ts
git commit -m "feat(leaderboard): client types for the leaderboard endpoint"
```

---

## Task 5: Frontend leaderboard route + nav + styles

**Files:**
- Create: `web/src/routes/leaderboard.ts`
- Modify: `web/src/main.ts` (import + register route)
- Modify: `web/src/ui/components/header.ts` (nav link)
- Modify: `web/src/ui/design.css` (styles)

- [ ] **Step 1: Create the route module**

Create `web/src/routes/leaderboard.ts`:

```ts
import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { request } from '../api/client';
import type { Leaderboard, LeaderboardPeriod } from '../state/user-types';
import type { RouteModule } from '../router';

export const leaderboard: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    const period = signal<LeaderboardPeriod>('all-time');
    const board = signal<Leaderboard | null>(null);
    const loading = signal(true);
    const error = signal<string | null>(null);

    async function load(p: LeaderboardPeriod): Promise<void> {
      loading.value = true;
      error.value = null;
      try {
        board.value = await request<Leaderboard>(`/leaderboard?period=${p}`, {
          method: 'GET',
        });
      } catch (e) {
        error.value = e instanceof Error ? e.message : 'Failed to load leaderboard.';
      } finally {
        loading.value = false;
      }
    }

    function selectPeriod(p: LeaderboardPeriod): void {
      if (period.value === p) return;
      period.value = p;
      void load(p);
    }

    const dispose = effect(() => {
      // Read signals eagerly before building the template (see profile.ts:
      // happy-dom/lit-html nested-conditional re-render quirk).
      const l = loading.value;
      const err = error.value;
      const b = board.value;
      const cur = period.value;
      const entries = b?.entries ?? [];
      const showEmpty = !l && !err && entries.length === 0;
      const showList = !l && !err && entries.length > 0;

      render(
        appShell(html`
          <section class="leaderboard panel">
            <h2>Leaderboard</h2>
            <div class="leaderboard__tabs" role="tablist">
              <button
                class="leaderboard__tab ${cur === 'all-time' ? 'is-active' : ''}"
                data-testid="tab-all-time"
                @click=${() => selectPeriod('all-time')}
              >
                All-time
              </button>
              <button
                class="leaderboard__tab ${cur === 'this-month' ? 'is-active' : ''}"
                data-testid="tab-this-month"
                @click=${() => selectPeriod('this-month')}
              >
                This month
              </button>
            </div>
            ${l ? html`<p>Loading…</p>` : nothing}
            ${err ? html`<p class="field-error">${err}</p>` : nothing}
            ${showEmpty
              ? html`<div class="empty-state"><p>No ranked players yet.</p></div>`
              : nothing}
            ${showList
              ? html`<ol class="leaderboard__list">
                  ${entries.map(
                    (e) =>
                      html`<li class="leaderboard__row">
                        <span class="leaderboard__rank">${e.rank}</span>
                        <a
                          class="leaderboard__name"
                          href="/u/${encodeURIComponent(e.username)}"
                          data-link
                          >${e.username}</a
                        >
                        <span class="leaderboard__rating">${e.rating}</span>
                      </li>`,
                  )}
                </ol>`
              : nothing}
          </section>
        `),
        root,
      );
    });

    void load(period.value);

    return () => {
      dispose();
      render(nothing, root);
    };
  },
};
```

- [ ] **Step 2: Register the route**

In `web/src/main.ts`:

1. Add the import alongside the other route imports (after the `profile` import):

```ts
import { leaderboard } from './routes/leaderboard';
```

2. Add the route to the `createRouter({ … })` map (after the `'/u/:username': profile,` line):

```ts
    '/leaderboard': leaderboard,
```

- [ ] **Step 3: Add the nav link**

In `web/src/ui/components/header.ts`, inside the `<nav class="site-nav">`, add a Leaderboard link as the first child of the nav (before `${themeToggle()}`):

```ts
    <nav class="site-nav">
      <a class="site-nav__link" href="/leaderboard" data-link data-testid="nav-leaderboard"
        >Leaderboard</a
      >
      ${themeToggle()}
```

(Keep the rest of the nav — `themeToggle()`, the `user ? avatarMenu(user) : …` block — unchanged.)

- [ ] **Step 4: Add styles**

Append to `web/src/ui/design.css`:

```css
/* Leaderboard */
.leaderboard__tabs {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
}
.leaderboard__tab {
  cursor: pointer;
  padding: 0.35rem 0.75rem;
  border: 1px solid currentColor;
  border-radius: 0.375rem;
  background: transparent;
  color: inherit;
  opacity: 0.7;
}
.leaderboard__tab.is-active {
  font-weight: 700;
  opacity: 1;
}
.leaderboard__list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.leaderboard__row {
  display: grid;
  grid-template-columns: 2.5rem 1fr auto;
  align-items: center;
  gap: 0.75rem;
  padding: 0.4rem 0;
  border-bottom: 1px solid rgba(128, 128, 128, 0.2);
}
.leaderboard__rank {
  text-align: right;
  opacity: 0.7;
  font-variant-numeric: tabular-nums;
}
.leaderboard__rating {
  font-variant-numeric: tabular-nums;
}
```

- [ ] **Step 5: Typecheck**

Run:
```bash
pnpm -C web exec tsc -p tsconfig.json --noEmit
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/routes/leaderboard.ts web/src/main.ts web/src/ui/components/header.ts web/src/ui/design.css
git commit -m "feat(leaderboard): /leaderboard page, nav link, styles"
```

---

## Task 6: Frontend component test

**Files:**
- Create: `web/tests/component/leaderboard.spec.ts`

- [ ] **Step 1: Write the test**

Create `web/tests/component/leaderboard.spec.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { leaderboard } from '../../src/routes/leaderboard';

function entry(rank: number, username: string, rating: number) {
  return { rank, username, rating, rd: 50, games_played: 10, score: rating - 100 };
}

describe('leaderboard route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    vi.unstubAllGlobals();
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders ranked rows for the default all-time board', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        expect(url).toContain('period=all-time');
        return new Response(
          JSON.stringify({
            period: 'all-time',
            entries: [entry(1, 'alice', 1700), entry(2, 'bob', 1600)],
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }),
    );
    const cleanup = leaderboard.render(
      {},
      { path: '/leaderboard', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent).toContain('alice');
    expect(document.body.textContent).toContain('bob');
    expect(document.querySelectorAll('.leaderboard__row').length).toBe(2);
    cleanup();
  });

  it('switches to this-month and refetches', async () => {
    const periods: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        periods.push(url.includes('this-month') ? 'this-month' : 'all-time');
        return new Response(JSON.stringify({ period: 'x', entries: [] }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    const cleanup = leaderboard.render(
      {},
      { path: '/leaderboard', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    (document.querySelector('[data-testid="tab-this-month"]') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(periods).toContain('this-month');
    cleanup();
  });

  it('shows an empty state when no players are ranked', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ period: 'all-time', entries: [] }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    const cleanup = leaderboard.render(
      {},
      { path: '/leaderboard', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent?.toLowerCase()).toContain('no ranked players');
    cleanup();
  });
});
```

- [ ] **Step 2: Run the test to verify it passes**

Run:
```bash
pnpm -C web exec vitest run --project=component tests/component/leaderboard.spec.ts
```
Expected: PASS — 3 tests.

- [ ] **Step 3: Run the full frontend test + lint suite (regression check)**

Run:
```bash
pnpm -C web test
pnpm -C web lint
```
Expected: PASS — all unit + component tests green, no lint warnings.

- [ ] **Step 4: Commit**

```bash
git add web/tests/component/leaderboard.spec.ts
git commit -m "test(leaderboard): component tests for the leaderboard page"
```

---

## Final verification

- [ ] **Run the complete suite end to end**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --workspace
cargo fmt --check
pnpm -C web test
pnpm -C web lint
pnpm -C web exec tsc -p tsconfig.json --noEmit
```
Expected: everything green.

- [ ] **Manual smoke (optional)**

Start the stack (`make dev`), open `/leaderboard`, confirm: the page renders, the All-time / This-month tabs switch and refetch, player names link to `/u/:username`, and an empty board shows the empty state. The nav "Leaderboard" link appears in the header.

---

## Spec coverage check

- Conservative ranking `rating − 2·RD` → Task 2 (SQL `ORDER BY`), constant in Task 1.
- Eligibility ≥ `MIN_GAMES` (=5) → Task 1 constant, Task 2 `HAVING`, tested.
- All-time board → Task 2 `AllTime`, Task 3 default period.
- Monthly season = activity filter, monthly cadence → Task 1 `month_bounds`, Task 2 `Month` EXISTS clause, Task 3 `this-month`/`YYYY-MM`.
- Re-ranks current ratings (no history) → inherent to the query (documented).
- Top-10 cap → Task 1 constant, Task 2 `LIMIT`, tested.
- Anon/bot excluded → Task 2 JOIN on `user_id`, tested.
- HTTP endpoint + period parsing + invalid → 422 → Task 3.
- Frontend page, two tabs, top-10 table, player links, nav link → Tasks 4–6.
- Tests at store, handler, and component levels → Tasks 2, 3, 6.
```
