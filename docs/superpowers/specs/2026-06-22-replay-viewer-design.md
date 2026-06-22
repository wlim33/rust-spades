# Replay Viewer — Design

**Date:** 2026-06-22
**Status:** Approved (pending spec review)

> **Reconciled 2026-06-22** with `2026-06-22-trick-notation-design.md`. The replay
> data now comes from the game-agnostic `trick-notation` model rather than spades-only
> STF: the `text/plain` export emits canonical trick-notation, and `replay.json` is the
> JSON projection of that model plus server annotations. See "Section 1" (revised) and
> "Reconciliation note" below. The notation crate must land before this viewer's server
> endpoint.

## Summary

Add a lichess-style **game replay viewer**: a step-through review of any completed
or aborted game, showing all four (now-revealed) hands, bids, tricks, and running
score. The game engine already encodes a full move transcript (STF) and can rebuild
any finished game via `replay()`; the backend exposes only a `text/plain` transcript
today and the frontend has no consumer. This feature adds a structured JSON endpoint
and a dedicated viewer UI.

This is the highest value-to-effort feature available: the risky, stateful half
(deterministic reconstruction, hidden-hand access control) already exists and is
covered by property tests. Replay is also lichess's defining feature and the
prerequisite for downstream work (spades-specific stats, puzzles).

## Reference: how lichess does it

Lichess exposes a game in two parallel formats — a **PGN text export** (interop
standard) and a **JSON export** (`GET /game/export/{id}`). The JSON returns a
**movelist** (`"moves"`), not precomputed board states; the client *replays* the
moves to reconstruct each position. The data the client cannot derive from the
moves — `clocks` (per-ply timing) and `evals` (engine analysis) — the server ships
as **parallel annotation arrays** zipped in at step time. The server never sends
board states, only moves plus annotations.

The governing principle: **client reconstructs what's derivable from the moves; the
server only ships what isn't.**

### Adapting the principle to spades

- **Imperfect information.** Unlike chess, the dealt hands are *not* derivable from
  the move list. So our transcript must carry the hands (STF already does). This is
  the spades equivalent of "ship what the client can't derive."
- **No shared rules library.** Chess clients reconstruct the board with a mature,
  shared library (chessops/chessground). We have no TS spades-rules library — the
  rules live only in Rust. Reimplementing spades scoring (bags, nil, blind-nil, the
  10-bag penalty) in TS would be a second source of truth with no shared test
  vectors.
- **Derive vs annotate split:**
  - Derivable + cheap + unambiguous → **client computes** (trick winner, card
    layout per step).
  - Not derivable, OR derivable but drift-prone/authoritative → **server ships as
    annotation** (cumulative scores, tricks-won per round, viewer seat).

## Architecture

Three units, each independently understandable and testable:

1. **Server** — new JSON endpoint `GET /games/{id}/replay.json`.
2. **`ReplayController`** (frontend) — pure logic, no DOM. Holds the decoded
   response + a cursor; derives a `ViewState` per step.
3. **`ReplayBoard`** (frontend) — rendering. Reuses animation primitives; renders
   four face-up hands; driven imperatively by the controller.

The `text/plain` endpoint remains the "PGN" equivalent (interop / power users) but now
emits **canonical trick-notation** instead of STF (see reconciliation). The new
`replay.json` endpoint is the structured format for the viewer. This mirrors lichess's
two-format design exactly.

### Reconciliation note

The `cumulative` / `tricks_won` / `viewer_seat` annotations are **not** part of the pure
trick-notation model (which is rule-agnostic and records events only). They are computed
server-side during `replay()` and composed into the HTTP DTO alongside the model's JSON
projection. This keeps the notation crate pure while still sparing the client from
reimplementing spades scoring.

## Section 1 — Server: JSON replay endpoint

`GET /games/{id}/replay.json`, registered alongside the existing
`/games/{id}/replay` text route. Same terminal-game guard (403 for in-progress —
encoding mid-game would leak hidden hands), same `GameNotFound` → 404 mapping.

### Response DTO (`GameReplayResponse`)

**Revised per reconciliation:** the body is the **JSON projection of the general
`trick-notation` model** (`deck` / `seats` / `deal` / `events`) plus server-only
annotation fields. **Cards serialize as `{suit, rank}` objects** — matching every
existing endpoint and the frontend `Card` type.

```jsonc
{
  "model": {                                 // pure trick-notation model (game-agnostic)
    "meta": { "game_hint": "spades", "seats": ["N","E","S","W"],
              "dealer": "N", "players": ["Ann","Bo","Cy","Di"] },
    "deck": { "preset": "french52" },
    "events": [
      { "type": "deal", "hands": [["N", [{…}, …]], …] },   // deal is an event; all revealed
      { "type": "call", "start": "E", "values": ["3","4","nil","4"] },
      { "type": "play", "leader": "E",
        "cards": [{…},{…},{…},{…}] },        // rotation order from leader
      …
    ]
  },
  "annotations": {                           // server-computed, NOT part of the pure model
    "tricks_won_by_round": [[4,3,2,4], …],   // per seat, per round
    "cumulative_by_round": [[84,56], …],     // team [A,B] cumulative after each round
    "result": [252, 198],                    // final team scores; null if not terminal
    "termination": "Completed"               // Completed | Aborted
  },
  "viewer_seat": 2                           // seat index if authed caller played; else null
}
```

The client groups the flat `events` stream into rounds/tricks itself (rotation from
each `play.leader`); the server never ships rule-derived state inside `model`. The only
spades-specific logic in TS remains the trick-winner rule.

### Server-computed fields (the "annotations")

- **`tricks_won_by_round` and `cumulative_by_round`** are captured by the server during
  `replay()` at each round boundary (the engine exposes `get_tricks_won` /
  `get_team_scores`). Justified by the derive-vs-annotate principle: keeps spades scoring
  rules single-sourced in Rust. They live in the DTO's `annotations` block, *outside* the
  pure trick-notation `model`.
- **`viewer_seat`** resolved server-side from the auth `Identity` → that game's
  seat, reusing `game_seats_for_game(game_id)` + `seat_matches_identity()`
  (`handlers/games.rs`). Null when the caller didn't play / isn't authenticated.

### OpenAPI

Annotate with `#[oasgen]`, return `Result<Json<GameReplayResponse>, (StatusCode,
Json<ErrorResponse>)>`. All field types derive `OaSchema`. After implementing:
start server → `pnpm -C web openapi:fetch` → `openapi:generate`; commit both
`web/openapi/openapi.json` and `web/src/api/schema.d.ts` (CLAUDE.md gotcha).

## Section 2 — Frontend: `ReplayController` + `ReplayBoard`

### `ReplayController` (pure logic, no DOM)

- Holds the decoded `GameReplayResponse` and a **cursor** `(round, step)`, where a
  step is one move (a bid or a card).
- Interprets the flat `model.events` stream — grouping `play` events into tricks/rounds
  via each event's `leader` + seat rotation — into a linear move list so `next() /
  prev() / seek(i) / jumpRound(r)` are O(1) cursor math. Moves per round = 4 bids + up
  to 52 cards.
- Exposes a derived **`ViewState`** for the current cursor: seat to act, cards on
  the table this trick, each seat's remaining hand, current round's bids,
  `tricks_won` so far, and `cumulative` score (read from `annotations.*_by_round` —
  no scoring math in TS).
- Computes the **trick winner** when a trick completes — the one derivable thing:
  highest spade, else highest card of the led suit.
- Surfaces autoplay / reduced-motion timing as plain state; never touches the DOM.

### `ReplayBoard` (rendering)

- Reuses the animation **primitives**: `animation.ts` (`animateTo`), `card-el.ts`
  (`createFront`), `trick-manager.ts` for the center-trick juice. **Does not**
  instantiate `CardOrchestrator` — it can't show four face-up hands and is welded to
  live-play/WS semantics (documented prod-bug territory in CLAUDE.md). Leaving the
  live orchestrator untouched avoids regressing live play.
- Renders **all four hands face-up** (the new capability), oriented so `viewer_seat`
  (or seat A) is at the bottom, reusing the live table's seat-rotation mapping.
- Two render modes off one path: **animated** (card flies to slot, trick collects to
  winner — same motion as live) and **instant** (under `prefers-reduced-motion`, or
  when scrubbing/stepping fast), mirroring the orchestrator's `skipAnims()` gate.
- Driven imperatively: controller emits a new `ViewState`; board diffs against the
  previous one and animates exactly the moves between them, or **snaps** when the
  jump is more than one step. One rule ("animate the delta, snap on big jumps")
  covers play, pause, step, and scrub.

### Why the controller/board split

The live code tangles state and animation in the orchestrator — which is *why* it
needs the generation/`clearAll` machinery (to survive out-of-order WS events). The
replay path has no network races, so we keep a clean boundary: the controller is
100% testable without a DOM, the board is a thin imperative renderer.

## Section 3 — Routing, entry points, edge cases

### Routing

- New lazy route `/replay/:id` in `web/src/router.ts`, code-split like `/play`.
- `:id` accepts a game UUID or short-id (the endpoint has both lookup paths).
- Public, no auth required (matches the endpoint's terminal-game-public model →
  shareable links).

### Entry points

- **Profile game-history rows** (`/u/:username`, `routes/profile.ts`) currently show
  a `game_id` with no link → link each row to `/replay/:id`. Primary discovery path.
- **End-of-game "Review game" button** on the terminal-state screen
  (`routes/game-view.ts`) so players land in review right after finishing.

### Edge cases & error handling

- **In-progress game** → 403 → route shows "This game is still in progress" with a
  link to the live game (not a dead error).
- **Not found / bad id** → 404 → existing not-found view.
- **Aborted games** → replayable; timeline ends early with an "Aborted" marker at
  the final step (driven by `termination`). Documented STF quirk: a game aborted
  *mid-betting* has lossy bet data — the viewer renders whatever bids are present
  and marks the abort.
- **Nil bids** render as "nil" (bet `0`); the round summary shows made/failed
  against `tricks_won`.

## Testing

Matches the repo split (Vitest unit/component + Playwright e2e):

- **`ReplayController` unit tests** (highest value, no DOM): cursor math,
  `next/prev/seek/jumpRound` bounds, `ViewState` derivation, trick-winner rule
  (spade-trump and led-suit cases), nil/abort handling.
- **`ReplayBoard` component test**: feed states, assert correct cards land in the
  right seats/trick; assert reduced-motion takes the instant path.
- **Server**: unit-test DTO assembly (round annotations + `viewer_seat` resolution)
  and a handler test for the 403/404/terminal guards. The existing transcript
  round-trip property tests already cover the underlying reconstruction.
- **One Playwright e2e**: finish a game → click Review → step through → assert
  score/board update. Reuses the existing e2e harness (auto-starts backend).

## Out of scope (YAGNI)

- Engine evaluation / "best play" analysis (a later feature, enabled by this one).
- Move annotations, comments, branching/variations.
- Scrubber timeline UI beyond step/jump controls and basic autoplay.
- Replay of in-progress games from a participant's own perspective.
- Sharing/embed cards, social features.

## Downstream this unlocks

- Spades-specific stats (nil success rate, bid accuracy, bags) computed from
  transcripts.
- "Best play here" puzzles generated from real game positions.
