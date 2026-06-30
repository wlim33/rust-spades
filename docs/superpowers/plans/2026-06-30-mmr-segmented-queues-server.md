# MMR Segmented Queues — Server Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the server matchmaker rating-segmented — seeks match only within the same rating band — so the lobby can be stocked across skill tiers without bots self-matching.

**Architecture:** Add a `band_of(rating)` helper with config boundaries `[1400, 1600]` (3 bands). Thread the seeker's Glicko rating into `PendingSeek`; the `seek` handler reads it from the user's stored rating (anonymous → default 1500). `try_match` and `notify_seekers` key on `(band, max_points, timer_config)` instead of `(max_points, timer_config)`. `list_seeks` reports per-band counts for the lobby.

**Tech Stack:** Rust, axum, the existing `spades-server` crate; Glicko-2 ratings in `crates/spades-server/src/ratings.rs`; matchmaker in `crates/spades-server/src/matchmaking.rs`.

## Global Constraints

- Band boundaries: `BAND_BOUNDARIES: [f64; 2] = [1400.0, 1600.0]` → bands `0` (`<1400`), `1` (`1400–1600`), `2` (`≥1600`). Lower boundary inclusive to the upper band (`band_of(1400.0) == 1`, `band_of(1600.0) == 2`).
- New/anonymous/unrated seeker rating = `crates::ratings::DEFAULT_RATING.rating` (`1500.0`) → band `1` (Mid).
- This slice is the foundation only: with all current bots at ~1500 they land in Mid — correct, single-tier until the bots slice seeds ratings. No behavior regression for existing single-bucket play.
- After changing `SeekSummary` (Task 4), regenerate BOTH OpenAPI artifacts (`web/openapi/openapi.json` + `web/src/api/schema.d.ts`) per the repo gotcha, or CI `openapi:check` fails.
- Run cargo from a shell with `~/.cargo/bin` on PATH.

---

### Task 1: `band_of` rating-band helper

**Files:**
- Create: `crates/spades-server/src/bands.rs`
- Modify: `crates/spades-server/src/lib.rs` (add `pub mod bands;`)

**Interfaces:**
- Produces: `pub const BAND_BOUNDARIES: [f64; 2]`; `pub fn band_of(rating: f64) -> u8` (returns `0`, `1`, or `2`).

- [ ] **Step 1: Write the failing test**

Create `crates/spades-server/src/bands.rs`:

```rust
//! Rating-band segmentation for matchmaking. A seeker's Glicko rating maps to
//! one of three broad skill tiers; the matchmaker only pairs seeks within the
//! same band (see `matchmaking::try_match`).

/// Upper-exclusive boundaries between the three bands, in the visible
/// 1500-scale. `< 1400` = band 0 (Low), `1400..1600` = band 1 (Mid),
/// `>= 1600` = band 2 (High). Server config — tune without a protocol change.
pub const BAND_BOUNDARIES: [f64; 2] = [1400.0, 1600.0];

/// Map a rating to its band index (0 = Low, 1 = Mid, 2 = High).
pub fn band_of(rating: f64) -> u8 {
    BAND_BOUNDARIES.iter().filter(|&&b| rating >= b).count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_boundaries_map_to_expected_tiers() {
        assert_eq!(band_of(0.0), 0);
        assert_eq!(band_of(1399.9), 0);
        assert_eq!(band_of(1400.0), 1); // lower boundary -> Mid
        assert_eq!(band_of(1500.0), 1);
        assert_eq!(band_of(1599.9), 1);
        assert_eq!(band_of(1600.0), 2); // upper boundary -> High
        assert_eq!(band_of(2000.0), 2);
    }
}
```

Add to `crates/spades-server/src/lib.rs` alongside the other `pub mod` lines:

```rust
pub mod bands;
```

- [ ] **Step 2: Run test to verify it fails (then passes — pure function)**

Run: `cargo test -p spades-server bands::tests::band_boundaries_map_to_expected_tiers`
Expected: compiles and PASSES (the helper and test are written together; this task has no separate red phase because the function is trivial and defined inline).

- [ ] **Step 3: Commit**

```bash
git add crates/spades-server/src/bands.rs crates/spades-server/src/lib.rs
git commit -m "feat(matchmaking): add band_of rating-band helper"
```

---

### Task 2: Thread rating into the seek queue and key matching on band

**Files:**
- Modify: `crates/spades-server/src/matchmaking.rs` (PendingSeek, add_seek, try_match, notify_seekers, existing tests)

**Interfaces:**
- Consumes: `crate::bands::band_of` (Task 1).
- Produces: `PendingSeek { rating: f64, .. }`; `add_seek(&self, max_points: i32, timer_config: TimerConfig, name: Option<String>, rating: f64) -> (Uuid, mpsc::UnboundedReceiver<SeekEvent>)`; `try_match(&self, band: u8, max_points: i32, timer_config: TimerConfig)`; `notify_seekers(&self, band: u8, max_points: i32, timer_config: TimerConfig)`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/spades-server/src/matchmaking.rs`:

```rust
#[tokio::test]
async fn test_same_band_four_seekers_match() {
    let mm = make_matchmaker();
    let mut receivers = Vec::new();
    // Four Mid-band ratings (1400..1600) -> one game.
    for r in [1450.0, 1500.0, 1520.0, 1580.0] {
        let (_pid, rx) = mm.add_seek(500, default_timer(), None, r).await;
        receivers.push(rx);
    }
    let mut game_id = None;
    for mut rx in receivers {
        while let Some(event) = rx.recv().await {
            if let SeekEvent::GameStart(result) = event {
                game_id = Some(result.game_id);
                break;
            }
        }
    }
    assert!(game_id.is_some(), "four same-band seekers should match");
}

#[tokio::test]
async fn test_cross_band_does_not_match() {
    let mm = make_matchmaker();
    // Three Mid + one High: different bands, so no game forms.
    for r in [1500.0, 1510.0, 1520.0] {
        let _ = mm.add_seek(500, default_timer(), None, r).await;
    }
    let _ = mm.add_seek(500, default_timer(), None, 1700.0).await; // High band
    // 4 seekers total but split 3 (Mid) + 1 (High): no match.
    let total: usize = mm.list_seeks().iter().map(|s| s.waiting).sum();
    assert_eq!(total, 4, "cross-band seeks must not be matched into a game");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p spades-server matchmaking::tests::test_cross_band_does_not_match`
Expected: FAIL to compile — `add_seek` takes 3 args, not 4.

- [ ] **Step 3: Implement — add `rating` to PendingSeek and thread it through**

In `crates/spades-server/src/matchmaking.rs`, add the field to `PendingSeek` (after `timer_config`):

```rust
struct PendingSeek {
    player_id: Uuid,
    max_points: i32,
    timer_config: TimerConfig,
    rating: f64,
    name: Option<String>,
    sender: mpsc::UnboundedSender<SeekEvent>,
}
```

Change `add_seek` to accept and store `rating`, and key the match/notify on band:

```rust
pub async fn add_seek(
    &self,
    max_points: i32,
    timer_config: TimerConfig,
    name: Option<String>,
    rating: f64,
) -> (Uuid, mpsc::UnboundedReceiver<SeekEvent>) {
    let player_id = Uuid::new_v4();
    let (tx, rx) = mpsc::unbounded_channel();

    {
        let mut queue = self.seek_queue.lock_or_recover();
        queue.push(PendingSeek {
            player_id,
            max_points,
            timer_config,
            rating,
            name,
            sender: tx,
        });
    }

    let band = crate::bands::band_of(rating);
    self.try_match(band, max_points, timer_config).await;
    self.notify_seekers(band, max_points, timer_config);
    (player_id, rx)
}
```

Update `cancel_seek` to recompute and notify by band — change the captured tuple and the `notify_seekers` call:

```rust
pub fn cancel_seek(&self, player_id: Uuid) {
    let seek_info;
    {
        let mut queue = self.seek_queue.lock_or_recover();
        seek_info = queue
            .iter()
            .find(|s| s.player_id == player_id)
            .map(|s| (s.rating, s.max_points, s.timer_config));
        queue.retain(|s| s.player_id != player_id);
    }
    if let Some((rating, mp, tc)) = seek_info {
        self.notify_seekers(crate::bands::band_of(rating), mp, tc);
    }
}
```

Change `try_match`'s signature and its filter to include band (only the signature line and the `matching` filter change; the rest of the body is unchanged):

```rust
async fn try_match(&self, band: u8, max_points: i32, timer_config: TimerConfig) {
    let seeks: Vec<PendingSeek> = {
        let mut queue = self.seek_queue.lock_or_recover();

        let matching: Vec<usize> = queue
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                crate::bands::band_of(s.rating) == band
                    && s.max_points == max_points
                    && s.timer_config == timer_config
            })
            .map(|(i, _)| i)
            .collect();

        if matching.len() < 4 {
            return;
        }
        // ... unchanged: take first 4, remove from back, reverse ...
```

Change `notify_seekers` to be band-aware:

```rust
fn notify_seekers(&self, band: u8, max_points: i32, timer_config: TimerConfig) {
    let queue = self.seek_queue.lock_or_recover();
    let matches = |s: &&PendingSeek| {
        crate::bands::band_of(s.rating) == band
            && s.max_points == max_points
            && s.timer_config == timer_config
    };
    let waiting = queue.iter().filter(matches).count();
    for seek in queue.iter().filter(matches) {
        let _ = seek.sender.send(SeekEvent::QueueUpdate { waiting });
    }
}
```

- [ ] **Step 4: Fix existing tests to pass a rating**

In the same `tests` module, update the three existing `add_seek(...)` call sites to pass a Mid rating so their behavior is unchanged. In `test_seek_match_4_players`, `test_seek_no_match_with_3_players`, and `test_seek_different_max_points_no_match`, change each `mm.add_seek(500, default_timer(), None)` to `mm.add_seek(500, default_timer(), None, 1500.0)` and the `300` one to `mm.add_seek(300, default_timer(), None, 1500.0)`.

- [ ] **Step 5: Run to verify all matchmaking tests pass**

Run: `cargo test -p spades-server matchmaking::`
Expected: PASS (existing 3 + new 2).

- [ ] **Step 6: Commit**

```bash
git add crates/spades-server/src/matchmaking.rs
git commit -m "feat(matchmaking): key seek matching on rating band"
```

---

### Task 3: Seek handler supplies the seeker's rating

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/matchmaking.rs` (the `seek` handler)

**Interfaces:**
- Consumes: `add_seek(.., rating: f64)` (Task 2); `state.auth.store.get_user_rating(user_id) -> Result<Option<Rating>, _>`; `crate::ratings::DEFAULT_RATING`.

- [ ] **Step 1: Implement — look up the rating before `add_seek`**

In `crates/spades-server/src/bin/server/handlers/matchmaking.rs`, just before the `add_seek` call, resolve the rating: logged-in users use their stored Glicko rating, everyone else uses the default. `identity_user` (the `Option<Uuid>`) is already computed in this handler.

```rust
let rating = match identity_user {
    Some(uid) => store
        .get_user_rating(uid)
        .ok()
        .flatten()
        .unwrap_or(spades_server::ratings::DEFAULT_RATING)
        .rating,
    None => spades_server::ratings::DEFAULT_RATING.rating,
};

let (player_id, mut rx) = state
    .matchmaker
    .add_seek(request.max_points, request.timer_config, validated_name, rating)
    .await;
```

(Note: `store` is already cloned in this handler as `let store = state.auth.store.clone();`. If `get_user_rating` is not in scope via `store`, confirm the method name on the store type — it is the same one `handlers_users.rs::get_profile` calls.)

- [ ] **Step 2: Build to verify the handler compiles**

Run: `cargo build -p spades-server`
Expected: builds clean.

- [ ] **Step 3: Run the server test suite (no regressions)**

Run: `cargo test -p spades-server`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-server/src/bin/server/handlers/matchmaking.rs
git commit -m "feat(matchmaking): seek uses the seeker's Glicko rating for banding"
```

---

### Task 4: Per-band lobby counts + OpenAPI regen

**Files:**
- Modify: `crates/spades-server/src/matchmaking.rs` (`SeekSummary`, `list_seeks`, tests)
- Modify (generated): `web/openapi/openapi.json`, `web/src/api/schema.d.ts`

**Interfaces:**
- Produces: `SeekSummary { band: u8, max_points: i32, waiting: usize }`; `list_seeks` grouped by `(band, max_points)`.

- [ ] **Step 1: Write the failing test**

Replace `test_seek_no_match_with_3_players`'s assertions and add a banded-count test in the `tests` module:

```rust
#[tokio::test]
async fn test_list_seeks_groups_by_band() {
    let mm = make_matchmaker();
    // 2 Mid + 1 Low waiting, none enough to match.
    let _ = mm.add_seek(500, default_timer(), None, 1500.0).await;
    let _ = mm.add_seek(500, default_timer(), None, 1550.0).await;
    let _ = mm.add_seek(500, default_timer(), None, 1200.0).await;

    let mut summary = mm.list_seeks();
    summary.sort_by_key(|s| s.band);
    assert_eq!(summary.len(), 2, "two bands occupied");
    assert_eq!(summary[0].band, 0); // Low
    assert_eq!(summary[0].waiting, 1);
    assert_eq!(summary[1].band, 1); // Mid
    assert_eq!(summary[1].waiting, 2);
}
```

Also update the existing `test_seek_no_match_with_3_players` to assert `summary[0].band == 1` (all default 1500 → Mid).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p spades-server matchmaking::tests::test_list_seeks_groups_by_band`
Expected: FAIL — `SeekSummary` has no `band` field.

- [ ] **Step 3: Implement — add `band` to SeekSummary and group by it**

Change the struct:

```rust
/// Summary of seeks waiting in a given band + max_points.
#[derive(Debug, Serialize, Deserialize, oasgen::OaSchema)]
pub struct SeekSummary {
    pub band: u8,
    pub max_points: i32,
    pub waiting: usize,
}
```

Change `list_seeks` to group by `(band, max_points)`:

```rust
pub fn list_seeks(&self) -> Vec<SeekSummary> {
    let queue = self.seek_queue.lock_or_recover();
    let mut counts: HashMap<(u8, i32), usize> = HashMap::new();
    for seek in queue.iter() {
        *counts
            .entry((crate::bands::band_of(seek.rating), seek.max_points))
            .or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|((band, max_points), waiting)| SeekSummary {
            band,
            max_points,
            waiting,
        })
        .collect()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p spades-server matchmaking::`
Expected: PASS.

- [ ] **Step 5: Regenerate OpenAPI artifacts**

Start the server, refetch + regenerate, stop the server:

```bash
cargo run -p spades-server -- --port 3000 &
SRV=$!; sleep 3
pnpm -C web openapi:fetch
pnpm -C web openapi:generate
kill $SRV
```

Expected: `web/openapi/openapi.json` and `web/src/api/schema.d.ts` show `SeekSummary` gaining `band`.

- [ ] **Step 6: Commit**

```bash
git add crates/spades-server/src/matchmaking.rs web/openapi/openapi.json web/src/api/schema.d.ts
git commit -m "feat(matchmaking): expose per-band waiting counts on /matchmaking/seeks"
```

---

### Task 5: Gate check

**Files:** none (verification only)

- [ ] **Step 1: Run the full pre-push gate**

Run: `make check`
Expected: fmt-check + clippy `-D warnings` + all tests + `openapi:check` PASS.

- [ ] **Step 2: Commit any fmt/clippy fixes if the gate required them**

```bash
git add -A
git commit -m "chore(matchmaking): fmt + clippy for band segmentation"
```

---

## Self-Review

- **Spec coverage:** band definition (Task 1) ✓; rating on seek + handler lookup (Tasks 2–3) ✓; banded `try_match` (Task 2) ✓; per-band lobby data (Task 4) ✓; quickplay-on-500 is a web-slice concern (out of this server slice — the server already accepts `max_points` and bots will only seek 500). Bots seeding/stocking/strength and the web tiered lobby are **separate follow-on plans** (see the design's build order).
- **Placeholder scan:** none — every step has concrete code/commands.
- **Type consistency:** `add_seek(.., rating: f64)`, `try_match(band: u8, ..)`, `notify_seekers(band: u8, ..)`, `SeekSummary { band: u8, .. }`, `band_of(f64) -> u8` are used consistently across Tasks 1–4.
- **Known follow-ups (next plans):** `bots` slice — seed bot ratings across bands, per-band stocking (per-band `cap=3`), per-band strength; `web` slice — tiered lobby render + standardize quickplay on 500.
