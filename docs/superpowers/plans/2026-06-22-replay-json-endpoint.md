# Replay JSON Endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `GET /games/{id}/replay.json` returning the game-agnostic `trick_notation::Model` (built directly from the Game, names preserved) plus server-computed annotations (per-round cumulative score, viewer seat), so the web replay viewer has a structured, type-safe data source.

**Architecture:** spades-core exposes the in-memory `Model` directly (`to_model`) and a per-round cumulative-score snapshot (`round_summaries`) computed by replaying the model through the engine. The server actor gains a `GetReplayData` command (mirroring the existing `GetTranscript`) that returns `Some((Model, Vec<[i32;2]>))` for terminal games and `None` otherwise. A new `#[oasgen]` handler wraps that in a `GameReplayResponse` DTO, adding the caller's `viewer_seat`.

**Tech Stack:** Rust (edition 2024), axum, oasgen (OpenAPI), serde, the `trick-notation` crate (already a workspace member). OpenAPI codegen produces two committed artifacts.

## Global Constraints

- This is the server half of `docs/superpowers/specs/2026-06-22-replay-viewer-design.md`. The web viewer (`ReplayController`/`ReplayBoard`/routes) is a SEPARATE follow-up plan — do not build UI here.
- Run cargo with `~/.cargo/bin` on PATH (`export PATH="$HOME/.cargo/bin:$PATH"`).
- The endpoint is terminal-only: `Some` for Completed/Aborted games, `None` (→ HTTP 403) for in-progress — same guard semantics as the existing `get_transcript`/`get_replay`. Encoding mid-game would leak hidden hands.
- Cards serialize as the `trick_notation` JSON shape (`{"kind":"suited","suit":"S","rank":"A"}`), inherited from the crate — do not re-map.
- The `Model` MUST be built directly from the Game (`to_model`), never via `decode(encode(game))` — the canonical text format is lossy for player names containing spaces/quotes (documented limitation); the direct path preserves them.
- Per the derive-vs-annotate principle: the server ships only what the client can't cheaply derive. `tricks_won`/trick-winner are client-derived; the server annotates only **per-round cumulative score** (needs engine scoring rules) and **viewer_seat** (needs auth↔seat mapping).
- OpenAPI: after adding the endpoint, regenerate BOTH committed artifacts — start the server, `pnpm -C web openapi:fetch`, then `pnpm -C web openapi:generate`; commit `web/openapi/openapi.json` AND `web/src/api/schema.d.ts` (CLAUDE.md gotcha).
- Commit with pathspec (`git commit -- <paths>`); the repo carries unrelated `web/` WIP — do not sweep it in. Working on `master` (no branch) per the prior session's choice unless told otherwise.

---

## File Structure

- `crates/spades-core/src/transcript/mod.rs` — add `pub fn to_model`, `pub struct RoundSummary` (or `[i32;2]`), `pub fn round_summaries`.
- `crates/spades-core/src/transcript/adapter.rs` — `round_summaries` implementation (reuses the round-walk logic from `model_to_game`).
- `crates/spades-server/src/game_actor.rs` — new `GameCmd::GetReplayData` + handler arm.
- `crates/spades-server/src/game_manager.rs` — `get_replay_data` delegating method.
- `crates/spades-server/src/bin/server/dto.rs` (or wherever response DTOs live — confirm; `GameStateResponse` is in `game_manager.rs`, error type `ErrorResponse` in dto.rs) — `GameReplayResponse` DTO.
- `crates/spades-server/src/bin/server/handlers/games.rs` — `get_replay_json` handler.
- `crates/spades-server/src/bin/server/main.rs` — route registration.
- `web/openapi/openapi.json`, `web/src/api/schema.d.ts` — regenerated artifacts.

---

### Task 1: spades-core — expose `to_model`

**Files:**
- Modify: `crates/spades-core/src/transcript/mod.rs`

**Interfaces:**
- Consumes: existing `adapter::game_to_model` (currently `pub(crate)`), `Game`, `trick_notation::Model`.
- Produces: `pub fn to_model(game: &Game) -> trick_notation::Model`.

- [ ] **Step 1: Write the failing test**

In `crates/spades-core/src/transcript/mod.rs` test module (where other transcript tests live), add:

```rust
#[test]
fn to_model_matches_encode_decode_path_for_simple_game() {
    // to_model(game) must equal what decode(encode(game)) yields, for a game
    // whose player names have no spaces (so the text path is lossless here).
    let g = super::adapter::tests_support_played_game(); // see note
    let direct = to_model(&g);
    let via_text = decode(&encode(&g)).unwrap();
    assert_eq!(direct, via_text);
}
```

Note: if no shared test-game helper exists, inline a small played game using the
same pattern other tests in this file use (`Game::new` + `play(Start)` + bets +
a few legal cards). Keep names unset (UUID-only) so the text path is lossless.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p spades --lib transcript::to_model`
Expected: FAIL — `cannot find function to_model`.

- [ ] **Step 3: Implement**

Add to `crates/spades-core/src/transcript/mod.rs` near the other public fns:

```rust
/// Build the game-agnostic notation model directly from a `Game`. Unlike
/// `decode(&encode(game))`, this preserves player names verbatim (the canonical
/// text format is lossy for names with whitespace/quotes — see Known limitations).
pub fn to_model(game: &Game) -> trick_notation::Model {
    adapter::game_to_model(game)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p spades --lib transcript::to_model`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-core/src/transcript/mod.rs
git commit -m "feat(spades-core): expose to_model for direct Model construction"
```

---

### Task 2: spades-core — `round_summaries` (per-round cumulative score)

**Files:**
- Modify: `crates/spades-core/src/transcript/adapter.rs` (implementation)
- Modify: `crates/spades-core/src/transcript/mod.rs` (re-export)

**Interfaces:**
- Consumes: `trick_notation::{Model, Event}`, `Game`, `GameTransition`, the same per-event replay logic used by `model_to_game`, `get_team_a_score`/`get_team_b_score`, `override_hands`, `from_tn_card`.
- Produces: `pub fn round_summaries(model: &trick_notation::Model) -> Result<Vec<[i32; 2]>, ReplayError>` — element `r` is `[team_a, team_b]` cumulative score after round `r` is fully played/scored. Length = number of fully-played rounds.

> **Why this exists:** the running score shown as the viewer steps through changes only at round boundaries (spades scores per round, with bags/nil/penalties). That math lives only in the engine, so the server replays the model and snapshots the cumulative scores. `tricks_won` is intentionally NOT returned — the client derives it from `Play` events.

- [ ] **Step 1: Write the failing test**

In `adapter.rs` test module:

```rust
#[test]
fn round_summaries_are_monotonic_in_round_count() {
    // A completed low-max-points game produces one cumulative pair per round,
    // and the final pair equals the game's final team scores.
    let g = played_completed_game(); // build via the file's existing helpers; low max_points e.g. 50
    let model = game_to_model(&g);
    let sums = round_summaries(&model).expect("summaries");
    assert!(!sums.is_empty());
    let last = *sums.last().unwrap();
    assert_eq!(last, [g.get_team_a_score().unwrap(), g.get_team_b_score().unwrap()]);
}
```

If `played_completed_game` isn't a real helper, inline it: `Game::new(..., 50, None)`,
then loop `play` choosing `Start`/`Bet(3)`/first legal card until `State::Completed`
(mirror the `play_first_legal` helper that existed in the old encode.rs tests).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p spades --lib transcript::adapter::round_summaries`
Expected: FAIL — `cannot find function round_summaries`.

- [ ] **Step 3: Implement**

Add to `adapter.rs`. This mirrors `model_to_game`'s event walk but snapshots
`[get_team_a_score, get_team_b_score]` at each round boundary (the engine has
scored round `r` by the time the next `Deal` arrives, and the final round by
end-of-events):

```rust
/// Cumulative `[team_a, team_b]` score after each fully-played round, computed by
/// replaying the model through the engine and snapshotting at round boundaries.
pub fn round_summaries(model: &Model) -> Result<Vec<[i32; 2]>, ReplayError> {
    let mut game = build_game_from_meta(model)?; // same construction model_to_game uses
    let mut summaries: Vec<[i32; 2]> = Vec::new();
    let mut started = false;
    let mut seen_first_deal = false;

    let snapshot = |g: &Game| -> [i32; 2] {
        [g.get_team_a_score().unwrap_or(0), g.get_team_b_score().unwrap_or(0)]
    };

    for event in &model.events {
        match event {
            Event::Deal { hands } => {
                if seen_first_deal {
                    // Previous round is fully scored by now.
                    summaries.push(snapshot(&game));
                }
                seen_first_deal = true;
                if !started {
                    game.play(GameTransition::Start).map_err(|err| ReplayError::Transition {
                        round: summaries.len(), trick: None, seat: 0, err,
                    })?;
                    started = true;
                }
                apply_deal(&mut game, hands); // override_hands with mapped cards
            }
            Event::Call { values, .. } => apply_bets(&mut game, values, summaries.len())?,
            Event::Play { cards, leader } => apply_play(&mut game, cards, leader, summaries.len())?,
            Event::Exchange { .. } | Event::Reveal { .. } => {} // not produced by spades
        }
    }
    if seen_first_deal {
        summaries.push(snapshot(&game)); // final round
    }
    Ok(summaries)
}
```

> **DRY note:** Tasks 7's `model_to_game` already contains the deal/bets/play
> application logic. Extract the shared steps into small helpers
> (`build_game_from_meta`, `apply_deal`, `apply_bets`, `apply_play`) and have BOTH
> `model_to_game` and `round_summaries` call them, rather than duplicating the
> per-event bodies. The `apply_*` helpers take the current round index for error
> reporting. Do the extraction as part of this task; keep `model_to_game`'s
> behavior identical (its tests must still pass).

- [ ] **Step 4: Re-export and run tests**

In `mod.rs` add `pub use adapter::round_summaries;` (next to other re-exports; `RoundSummary` is just `[i32;2]`, no new type needed).

Run: `cargo test -p spades --lib transcript`
Expected: PASS (new test + all existing transcript tests, including `model_to_game` round-trips, still green after the helper extraction).

- [ ] **Step 5: Commit**

```bash
git add crates/spades-core/src/transcript/adapter.rs crates/spades-core/src/transcript/mod.rs
git commit -m "feat(spades-core): per-round cumulative score via round_summaries"
```

---

### Task 3: server — `GetReplayData` actor command + manager delegation

**Files:**
- Modify: `crates/spades-server/src/game_actor.rs` (GameCmd variant + handler arm + actor method)
- Modify: `crates/spades-server/src/game_manager.rs` (delegating method)

**Interfaces:**
- Consumes: `spades::transcript::{to_model, round_summaries, Model}`, `spades::State`, the existing `GameCmd`/oneshot actor pattern.
- Produces:
  - `GameActorHandle::get_replay_data(&self) -> Result<Option<ReplayData>, GameManagerError>`
  - `GameManager::get_replay_data(&self, game_id: Uuid) -> Result<Option<ReplayData>, GameManagerError>`
  - `pub struct ReplayData { pub model: spades::transcript::Model, pub cumulative_by_round: Vec<[i32; 2]> }` (define in `game_actor.rs` or a shared module; derive nothing special — it's internal).

> **Mirror the existing `GetTranscript` command end-to-end.** Read how
> `GameCmd::GetTranscript` is defined, sent, and handled in `game_actor.rs`
> (the actor arm checks terminal state and returns `Some(text)`/`None`) and how
> `game_manager.rs::get_transcript` delegates. Replicate that exact shape for
> `GetReplayData`, returning `ReplayData` instead of a `String`.

- [ ] **Step 1: Write the failing test**

In `game_manager.rs` tests (mirror an existing get_transcript test if present; otherwise add one):

```rust
#[tokio::test]
async fn get_replay_data_none_for_in_progress_some_for_terminal() {
    let mgr = /* construct a GameManager the way other tests here do */;
    let id = /* create + start a game, place bets, play to completion using the test helpers */;
    // In progress → None
    // (assert at a mid-game point if the helper exposes one)
    // Completed → Some with a non-empty model + cumulative_by_round
    let data = mgr.get_replay_data(id).await.unwrap();
    let data = data.expect("terminal game yields replay data");
    assert!(!data.model.events.is_empty());
    assert_eq!(data.cumulative_by_round.len() >= 1, true);
}
```

Adapt construction/looping to the file's existing test conventions (there are
existing tests that create games and drive them — match them).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p spades-server --lib get_replay_data`
Expected: FAIL — method/variant not found.

- [ ] **Step 3: Implement the actor side**

In `game_actor.rs`:
- Add the struct:
```rust
pub struct ReplayData {
    pub model: spades::transcript::Model,
    pub cumulative_by_round: Vec<[i32; 2]>,
}
```
- Add a `GameCmd::GetReplayData { reply: oneshot::Sender<Option<ReplayData>> }` variant (match the exact field style of `GetTranscript`).
- In the actor's command loop, add an arm mirroring `GetTranscript`'s terminal guard:
```rust
GameCmd::GetReplayData { reply } => {
    let out = match self.game.get_state() {
        spades::State::Completed | spades::State::Aborted => {
            let model = spades::transcript::to_model(&self.game);
            // round_summaries replays the model; it cannot fail for a model we
            // just produced from a valid game, but propagate defensively.
            let cumulative = spades::transcript::round_summaries(&model).unwrap_or_default();
            Some(ReplayData { model, cumulative_by_round: cumulative })
        }
        _ => None,
    };
    let _ = reply.send(out);
}
```
(Use the actor's real field name for the game — confirm whether it's `self.game`.)
- Add the handle method mirroring `get_transcript`:
```rust
pub async fn get_replay_data(&self) -> Result<Option<ReplayData>, GameManagerError> {
    let (tx, rx) = oneshot::channel();
    self.sender
        .send(GameCmd::GetReplayData { reply: tx })
        .map_err(|_| GameManagerError::GameNotFound)?;
    rx.await.map_err(|_| GameManagerError::GameNotFound)
}
```

- [ ] **Step 4: Implement the manager delegation**

In `game_manager.rs`, mirror `get_transcript`:
```rust
pub async fn get_replay_data(
    &self,
    game_id: Uuid,
) -> Result<Option<crate::game_actor::ReplayData>, GameManagerError> {
    self.handle(game_id).await?.get_replay_data().await
}
```
(Match the real module path of `ReplayData` / how the manager refers to actor types.)

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p spades-server --lib get_replay_data`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/spades-server/src/game_actor.rs crates/spades-server/src/game_manager.rs
git commit -m "feat(server): GetReplayData actor command + manager delegation"
```

---

### Task 4: server — `GameReplayResponse` DTO + `get_replay_json` handler + route

**Files:**
- Modify: `crates/spades-server/src/bin/server/handlers/games.rs` (handler + DTO, OR put DTO in dto.rs — match where `GameStateResponse`-like response DTOs that derive `oasgen::OaSchema` live; `GameStateResponse` is in `game_manager.rs`. Put `GameReplayResponse` next to the handler in `games.rs` or in `dto.rs` — follow the codebase's existing choice for handler response DTOs.)
- Modify: `crates/spades-server/src/bin/server/main.rs` (route)

**Interfaces:**
- Consumes: `AppState`, `GameManager::get_replay_data`, `spades::transcript::Model`, the auth `Identity` extractor, `state.auth.store.game_seats_for_game`, `seat_matches_identity` (in `games.rs`).
- Produces:
  - `GameReplayResponse { model: spades::transcript::Model, cumulative_by_round: Vec<[i32;2]>, viewer_seat: Option<usize> }` deriving `Serialize, oasgen::OaSchema`.
  - `pub async fn get_replay_json(...) -> Result<Json<GameReplayResponse>, (StatusCode, Json<ErrorResponse>)>`.

> **OaSchema requirement:** every field type must derive `oasgen::OaSchema`. `spades::transcript::Model` (= `trick_notation::Model`) must derive it. The `trick-notation` crate currently derives only `serde`. ADD an optional `openapi` feature to `trick-notation` (mirroring spades-core's `openapi` feature gating `oasgen::OaSchema` on public types: `Model`, `Meta`, `Event`, `Deck`, `Card`), and have spades-core's `openapi` feature enable `trick-notation/openapi`. If that proves too invasive, the fallback is to NOT embed `Model` directly but project it into a server-local DTO that derives OaSchema — but prefer the feature approach to avoid a second model shape.

- [ ] **Step 1: Write the failing handler test**

Add an axum handler test (match how `games.rs`/the server tests exercise handlers — there are existing handler tests; mirror their `AppState` setup and request flow):

```rust
#[tokio::test]
async fn replay_json_200_for_terminal_403_for_in_progress_404_for_missing() {
    // terminal game → 200 with model.events non-empty and cumulative_by_round present
    // in-progress game → 403
    // unknown id → 404
}
```
Fill in using the file's existing handler-test harness conventions (construct
AppState, create/drive a game, call the handler fn directly or via the router).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p spades-server replay_json`
Expected: FAIL — handler not found.

- [ ] **Step 3: Implement the DTO + handler**

```rust
#[derive(Debug, Serialize, oasgen::OaSchema)]
pub struct GameReplayResponse {
    pub model: spades::transcript::Model,
    /// Cumulative [team_a, team_b] score after each fully-played round.
    pub cumulative_by_round: Vec<[i32; 2]>,
    /// Seat index (0..4) the authenticated caller played, if any; else null.
    pub viewer_seat: Option<usize>,
}

#[oasgen]
pub async fn get_replay_json(
    AxumState(state): AxumState<AppState>,
    Path(game_id): Path<Uuid>,
    identity: spades_server::auth::Identity,
) -> Result<Json<GameReplayResponse>, (StatusCode, Json<ErrorResponse>)> {
    let data = state
        .game_manager
        .get_replay_data(game_id)
        .await
        .map_err(|e| {
            let status = match e {
                GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse { error: format!("{e}") }))
        })?;
    let Some(data) = data else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "replay is only available for completed or aborted games".to_string(),
            }),
        ));
    };

    // Resolve the caller's seat (if they played) from the seat roster.
    let viewer_seat = state
        .auth
        .store
        .game_seats_for_game(game_id)
        .ok()
        .and_then(|seats| {
            seats
                .iter()
                .find(|s| seat_matches_identity(s, &identity))
                .map(|s| s.seat_index as usize)
        });

    Ok(Json(GameReplayResponse {
        model: data.model,
        cumulative_by_round: data.cumulative_by_round,
        viewer_seat,
    }))
}
```
(Confirm `game_seats_for_game`'s real return type — adapt the `.ok()`/iteration to it. Confirm `Identity` import path matches `get_hand`'s.)

- [ ] **Step 4: Register the route**

In `main.rs`, add to the `#[oasgen]` router group (next to `get_game_state`):
```rust
.get("/games/{game_id}/replay.json", get_replay_json)
```
Add `get_replay_json` to the handler `use` list. (The existing text `/games/{game_id}/replay` route stays.)

- [ ] **Step 5: Run to verify it passes + clippy**

Run: `cargo test -p spades-server replay_json` → PASS.
Run: `cargo clippy -p spades-server -p spades -p trick-notation -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/spades-server/src/bin/server/handlers/games.rs crates/spades-server/src/bin/server/main.rs crates/trick-notation/ crates/spades-core/Cargo.toml
git commit -m "feat(server): GET /games/{id}/replay.json endpoint"
```

---

### Task 5: OpenAPI codegen artifacts

**Files:**
- Modify: `web/openapi/openapi.json`, `web/src/api/schema.d.ts` (regenerated, committed)

> No application code here — this regenerates the two committed OpenAPI artifacts
> so the web client is type-aware of the new endpoint. CI's `openapi:check` only
> verifies schema.d.ts ↔ openapi.json consistency; it does NOT catch a stale
> openapi.json vs the live server, so this regen must be done from the running server.

- [ ] **Step 1: Start the server**

Run (background): `cargo run -p spades-server --bin server -- --insecure-cookies --cors-allow-origin http://localhost:5173`
Wait until it logs listening on :3000.

- [ ] **Step 2: Fetch + generate**

Run: `pnpm -C web openapi:fetch`
Run: `pnpm -C web openapi:generate`

- [ ] **Step 3: Verify the new endpoint is present**

Run: `grep -c "replay.json" web/openapi/openapi.json`
Expected: ≥ 1.
Run: `pnpm -C web openapi:check`
Expected: passes (schema.d.ts ↔ openapi.json consistent).

- [ ] **Step 4: Stop the server, commit**

```bash
git add web/openapi/openapi.json web/src/api/schema.d.ts
git commit -m "chore(web): regenerate OpenAPI artifacts for replay.json"
```

---

### Task 6: Full gate + coverage

**Files:**
- Modify: `coverage-baseline.json` (only if coverage moved; regenerate honestly)

- [ ] **Step 1: Full workspace gate**

Run: `cargo test --workspace` → all pass.
Run: `cargo clippy --workspace -- -D warnings` → clean.
Run: `make check` → green (fmt + lint + tests).

- [ ] **Step 2: Refresh coverage baseline**

Run: `bash hooks/update-coverage-baseline.sh`
Confirm spades-core / spades-server / trick-notation numbers did not regress below their committed baselines (the new server code is covered by Task 3/4 tests; add a focused test if a number drops materially). Commit only if the file changed.

- [ ] **Step 3: Commit (if baseline changed)**

```bash
git add coverage-baseline.json
git commit -m "chore: refresh coverage baseline for replay.json endpoint"
```

---

## Self-Review notes (for the implementer)

- **Spec coverage:** implements the server half of the replay-viewer spec's Section 1 — `replay.json` returning the `Model` (built directly via `to_model`, names preserved) + `cumulative_by_round` + `viewer_seat`, terminal-only guard, OpenAPI artifacts. `tricks_won` is intentionally omitted (client-derived per the derive-vs-annotate principle — note this deviates from the spec's earlier `tricks_won_by_round` field; it is a deliberate simplification, flag it in review).
- **Deferred:** the web viewer (`ReplayController`/`ReplayBoard`/routes/entry points) is the next plan. The compact-format endpoint (`?format=compact`) remains deferred per the spec.
- **Reconciliation flag:** the spec's DTO sketch showed `annotations.tricks_won_by_round`; this plan drops it (derivable client-side). If the web-viewer plan finds it genuinely needs server-side `tricks_won`, add it then.
- **Engine-accessor caveat:** as in the trick-notation crate plan, exact spades-core/spades-server accessor and field names (actor's game field, `game_seats_for_game` return type, handler-test harness) must be reconciled against the real code; the plan points at the `GetTranscript` command and `get_hand` handler as the patterns to mirror.
- **Type consistency:** `ReplayData { model, cumulative_by_round }`, `GameReplayResponse { model, cumulative_by_round, viewer_seat }`, and `round_summaries -> Vec<[i32;2]>` are used consistently across Tasks 2–4.
