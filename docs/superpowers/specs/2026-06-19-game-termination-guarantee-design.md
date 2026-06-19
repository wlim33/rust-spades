# Game Termination Guarantee — Design

**Date:** 2026-06-19
**Status:** Approved (design)
**Crate:** `spades-core` (published as `spades`)

## Problem

A game ends only when a team reaches `cumulative_points >= max_points` (default
500). There is no lower bound and no round cap. Two teams that both keep losing
points (failed bids, bag penalties) diverge negative and the game never
terminates. `hands_played: Vec<[Option<Card>; 4]>` then grows without bound.

This caused a production outage on 2026-06-19: 30 such runaway games reached
5,000–27,558 rounds (scores ~ −44,000 against a 500 target), bloating
`games.sqlite` to 87 MB. Startup eager-loads every game into memory
(`GameManager::with_db` → `load_all_games`), expanding to ~2–3 GB and OOM-killing
the backend on the 3.7 GB VPS. The runaway rows were deleted to restore service;
this change prevents recurrence at the source.

(The eager-load-at-startup scaling problem is a separate follow-up — out of scope
here.)

## Goal

Guarantee that every game terminates, so `hands_played` and the persisted blob
stay bounded.

## Change Locus

`crates/spades-core/src/scoring.rs`, in the round-completion block (currently
lines 151–163, where `is_over` is set after a round's scores are tallied).

No changes to `spades-server` (actor / manager / API). Setting `is_over = true`
already flows through the existing `State::Completed` transition, which fires the
Glicko-2 rating update and lets the sweeper evict the game.

## New Terminal Conditions

Two module-level constants in `scoring.rs`:

```rust
const MIN_POINTS: i32 = -200;   // a team at or below this loses
const MAX_ROUNDS: usize = 100;  // hard backstop on game length
```

Terminal rules, evaluated at round completion:

| Rule | Condition | Tie behavior |
|---|---|---|
| Max points (existing) | a team `>= max_points` | both-equal → game **continues** (unchanged) |
| Loss floor (new) | a team `<= MIN_POINTS` | ends unconditionally |
| Round cap (new) | completed round index reaches `MAX_ROUNDS` | ends unconditionally |

The existing max-points "tie continues" escape is preserved for normal games. The
new floor and cap conditions end the game unconditionally (no tie escape) — that
is what guarantees termination.

Round indexing: `self.round` is the just-completed round's 0-based index, and is
incremented at the end of the block. The cap fires when the 100th round completes
(`self.round + 1 >= MAX_ROUNDS`, i.e. index 99). Exact boundary to be pinned by a
test.

## Winner Determination

Higher `cumulative_points` wins, via the existing `Game::get_winner_ids` and the
server's score-comparison rating path (`fire_rating_update` uses
`get_team_a_score`/`get_team_b_score`, treating a tie as team B).

Exact score ties at the floor/cap are astronomically unlikely and harmless:
- No production code calls `get_winner_ids` (definition + tests only); it returns
  `Err(GameNotCompleted)` on a tie, which is semantically fine (no single winner).
- The rating update never errors on a tie.

The stale comment in `get_winner_ids` ("Unreachable: Scoring keeps is_over = false
on a tie at max_points") will be updated: a tie at the floor/cap is now reachable.

## Testing (TDD)

New `scoring.rs` unit tests:
1. **Floor:** drive a team to `<= -200` → `is_over == true`; the higher-scoring
   team is the winner.
2. **Round cap:** play `MAX_ROUNDS` low-scoring rounds with neither team reaching
   `max_points` or the floor → `is_over == true` at the cap; pin the exact round.
3. **Floor precedence:** floor ends the game even when nobody approached
   `max_points`.

Regression: existing max-points and tie-continues tests must still pass
(`cargo test -p spades`).

## Non-Goals

- Not changing the eager-load-at-startup architecture (separate follow-up).
- No new config fields — the floor is a fixed constant.
- No data cleanup (already done during the incident).
- No bot-strategy changes.

## Compatibility

Behavior change (when games end) in the published `spades` crate, not an
API-signature break. In-flight persisted games adopt the new rules on their next
completed round; any already past the floor will end on their next round, which is
the intended correction.
