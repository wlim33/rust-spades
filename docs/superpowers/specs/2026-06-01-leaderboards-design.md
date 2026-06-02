# Leaderboards — Design

**Date:** 2026-06-01
**Status:** Approved (pending implementation plan)

## Summary

Add a leaderboard feature that ranks players by a conservative Glicko-2
score. Two views: an **all-time** board and a **monthly season** board. Both
re-rank the *current* stored ratings; seasons are an activity filter, not a
separate rating track. Each board shows the **top 10** players only.

This builds entirely on data the server already stores (`users.rating/rd`,
`game_seats`). No new tables, no migration, no background jobs.

## Decisions

| Question | Decision |
| --- | --- |
| Ranking metric | Conservative score = `rating − 2·RD` (descending) |
| Eligibility | Player has played ≥ `MIN_GAMES` games (default **5**) |
| Scope | All-time **and** monthly seasons |
| Season model | No per-season rating; a season board re-ranks current all-time ratings, filtered to players active that month |
| Cadence | Fixed monthly (UTC calendar month) |
| Board size | Top **10** only |

## Background (existing system)

- Glicko-2 ratings live on the `users` table (`rating REAL`, `rd REAL`,
  `volatility REAL`), defaulting to `1500 / 350 / 0.06`
  (`spades-server/src/ratings.rs`).
- Ratings update in the background after each finished game
  (`game_actor.rs::apply_glicko_update`).
- `game_seats(game_id, seat_index, player_id, user_id, anon_user_id,
  is_bot, created_at)` links users to games. `user_id` is null for anon/bot
  seats.
- The public profile (`GET /users/{username}`,
  `handlers_users.rs::get_profile`) already exposes `rating`, `rd`, and
  `games_played`, where `games_played` is the user's `game_seats` row count
  (`count_game_seats_for_user`).
- The frontend is lit-html + `@preact/signals-core` with a `navaid` router
  (`web/src/main.ts`), an `appShell` template, and a `header.ts` nav.
  Profile-style endpoints use hand-written client types rather than the
  generated OpenAPI schema.

## Approach

Pure SQL ranking over the existing tables (chosen over snapshot/history
tables and over in-Rust sorting). Rationale: smallest footprint, no
migration or rollover job, and it matches the accepted season semantics
("filter by activity", not historical accuracy).

### Eligibility & activity definitions

- **Games played** reuses the existing seat-count semantics
  (`count_game_seats_for_user`) — the same number shown on profiles. This
  deliberately counts joined games rather than strictly finished ones;
  conservative scoring already keeps unproven players (`1500 − 2·350 = 800`)
  at the bottom, so the imprecision is immaterial to ranking. Reusing it
  keeps the whole app internally consistent.
- **Active this month** = the user has at least one `game_seats` row whose
  `created_at` falls within the target UTC calendar month.
- The `≥ MIN_GAMES` gate applies to **both** boards (all-time game count).
  The monthly board *additionally* requires activity in the month.

### Backend

**Store method** (`sqlite_store.rs`):

```rust
pub enum LeaderboardWindow {
    AllTime,
    Month { year: i32, month: u32 }, // UTC calendar month
}

pub struct LeaderboardRow {
    pub username: String,
    pub rating: f64,
    pub rd: f64,
    pub games_played: i64,
    pub score: f64, // rating - RD_CONSERVATISM * rd
}

impl SqliteStore {
    pub fn leaderboard(
        &self,
        window: LeaderboardWindow,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, String>;
}
```

Query shape:

- Join `users` to a per-user seat count from `game_seats`
  (`GROUP BY user_id`).
- `WHERE games_played >= min_games`.
- For `Month`, additionally require `EXISTS` a seat with `created_at`
  within `[month_start, next_month_start)`.
- `ORDER BY (rating - RD_CONSERVATISM * rd) DESC, rating DESC` (rating as a
  deterministic tiebreak), `LIMIT limit`.

**Handler** (`handlers_users.rs` or a new `handlers_leaderboard.rs`,
following the `State(AuthState)` + hand-written DTO pattern):

- `GET /leaderboard?period=<all-time|this-month|YYYY-MM>`
  - `period` omitted → defaults to `all-time`.
  - `this-month` → current UTC month.
  - `YYYY-MM` → that month (lets past months be addressed even though the
    v1 UI only exposes all-time + this-month).
  - Invalid `period` → `400`.
- Response DTO:

```json
{
  "period": "all-time",
  "entries": [
    { "rank": 1, "username": "alice", "rating": 1712, "rd": 84,
      "games_played": 37, "score": 1544 }
  ]
}
```

`rating`, `rd`, and `score` are rounded to integers for display (matching
the profile endpoint). `rank` is 1-based position in the returned slice.

**Constants** (alongside `ratings.rs` or the handler):

- `MIN_GAMES: i64 = 5`
- `LEADERBOARD_SIZE: i64 = 10`
- `RD_CONSERVATISM: f64 = 2.0`

**Route registration**: plain axum route (mirrors `get_profile`), wired in
`bin/server/main.rs`.

### Frontend

- **New route** `/leaderboard` → `web/src/routes/leaderboard.ts`, structured
  like `profile.ts` (signals + `effect` + lit-html, wrapped in `appShell`).
- **Two tabs**: *All-time* and *This month*. Switching tabs refetches with
  the matching `period`. No past-month browser in v1 (the API supports it
  for later).
- **Table** of up to 10 rows: rank · player name (links to `/u/:username`) ·
  rating. Loading / empty / error states mirror `profile.ts`.
- **Nav**: add a *Leaderboard* link to `components/header.ts`.
- **Client types + fetch**: hand-written, mirroring how profile types are
  defined (these endpoints are not in the generated OpenAPI schema).
- **Router**: register `'/leaderboard'` in `web/src/main.ts`.

## Error handling

- Store errors propagate as `AuthError::Storage` → 500 (same as profile).
- Invalid `period` → `400` with the standard error body.
- Empty board (no one meets the gate) → `200` with `entries: []`; the UI
  renders an empty state.

## Testing

**Backend (store unit tests):**
- Ranking order follows `rating − 2·RD`, not raw rating (a high-rating /
  high-RD player ranks below a slightly-lower-rating / low-RD player).
- `MIN_GAMES` gate excludes players below the threshold.
- Monthly window includes only players with a seat in that month and
  excludes players active only in other months.
- Top-10 cap: with >10 eligible players, exactly 10 returned, in order.
- Anon/bot seats (null `user_id`) never appear.

**Backend (handler test):**
- `period` parsing: omitted → all-time; `this-month`; `YYYY-MM`; invalid →
  400.

**Frontend:**
- Render test mirroring the profile test: renders rows from a mocked
  response, tab switch triggers refetch, empty state renders.

## Limitations (accepted)

- The monthly board ranks by *current* all-time rating, so a past month
  reflects where those players stand now, not their end-of-month standing.
  No rating history is stored.
- "Games played" counts joined games (seat rows), not strictly finished
  games — consistent with the existing profile.

## Out of scope (future)

- Per-season rating tracks / soft resets.
- A past-month browser UI / month picker.
- Partnership (team) leaderboards.
- Pagination beyond the top 10.
