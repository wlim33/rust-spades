# Game Termination Guarantee Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Guarantee every Spades game terminates by adding a loss-score floor and a round-count backstop, so games can no longer run forever and bloat the DB.

**Architecture:** All logic lives in `crates/spades-core/src/scoring.rs` in the round-completion block of `Scoring::trick`. Setting `is_over = true` already flows through the existing `State::Completed` transition in the server (rating update + sweeper eviction), so no server-side changes are needed. One stale doc comment in `lib.rs` is refreshed.

**Tech Stack:** Rust, `cargo test`. Package name is `spades` (the crate `spades-core` publishes as `spades`).

## Global Constraints

- Package name for cargo commands: `spades` (e.g. `cargo test -p spades`).
- Run cargo with `~/.cargo/bin` on PATH: `export PATH="$HOME/.cargo/bin:$PATH"`.
- TDD: write the failing test first, watch it fail, then implement.
- The existing max-points "tie continues" behavior MUST be preserved unchanged.
- The new floor and round-cap conditions end the game UNCONDITIONALLY (no tie escape) — that is what guarantees termination.
- Tests go in the existing in-file `#[cfg(test)] mod tests` in `scoring.rs` (line ~173), which can read the module-private consts.
- Use the existing `play_round(scoring, team_a_wins)` test helper where realistic play is needed.

---

### Task 1: Loss-score floor terminates the game

**Files:**
- Modify: `crates/spades-core/src/scoring.rs` (add `MIN_POINTS` const near the top of the file, after the `use` line; add the floor check in the round-completion block at lines ~151-163)
- Test: `crates/spades-core/src/scoring.rs` (in `mod tests`)

**Interfaces:**
- Consumes: existing `Scoring`, `Scoring::new`, `Scoring::add_bet`, `Scoring::bet`, `Scoring::trick`, pub fields `team_a`/`team_b.cumulative_points`, `is_over`; test helper `play_round`.
- Produces: module-private `const MIN_POINTS: i32 = -200;`. New terminal behavior: `is_over` becomes `true` at round end when either team's `cumulative_points <= MIN_POINTS`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/spades-core/src/scoring.rs`:

```rust
#[test]
fn test_is_over_team_below_loss_floor() {
    // A team that keeps getting set eventually drops to the loss floor and
    // loses, even though neither team ever reached max_points.
    let mut s = Scoring::new(500);
    s.team_b.cumulative_points = -100; // just above the floor going in

    // Team A bets 6 (3+3) and wins all 13 -> makes bid.
    // Team B bets 13 (7+6) and wins 0 -> set -> -130 -> ends at -230.
    s.add_bet(0, 3);
    s.add_bet(1, 7);
    s.add_bet(2, 3);
    s.add_bet(3, 6);
    s.bet();

    play_round(&mut s, 13); // team A wins all 13, team B wins 0

    assert!(s.is_over, "game must end when a team hits the loss floor");
    assert!(s.team_b.cumulative_points <= MIN_POINTS);
    assert!(s.team_a.cumulative_points < s.config.max_points); // not a max-points win
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p spades test_is_over_team_below_loss_floor`
Expected: FAIL — either `cannot find value MIN_POINTS in this scope` (const not yet added) or assertion failure on `s.is_over`.

- [ ] **Step 3: Add the constant**

Near the top of `crates/spades-core/src/scoring.rs`, immediately after the `use crate::cards::...;` line, add:

```rust
/// A team at or below this cumulative score loses — the standard Spades
/// "minimum score" rule. Without it, two teams that both keep losing points
/// never reach `max_points` and the game runs forever (unbounded `hands_played`,
/// which OOM'd the server on 2026-06-19). See
/// docs/superpowers/specs/2026-06-19-game-termination-guarantee-design.md.
const MIN_POINTS: i32 = -200;
```

- [ ] **Step 4: Add the floor check**

In `Scoring::trick`, in the round-completion block, immediately AFTER the existing
`if a_reached || b_reached { ... }` block and BEFORE `self.round += 1;`
(currently line ~162), add:

```rust
            // Loss floor: a team at or below MIN_POINTS loses. Unconditional —
            // no tie escape — so a perpetually-losing game still terminates.
            if self.team_a.cumulative_points <= MIN_POINTS
                || self.team_b.cumulative_points <= MIN_POINTS
            {
                self.is_over = true;
            }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p spades test_is_over_team_below_loss_floor`
Expected: PASS

- [ ] **Step 6: Run the surrounding suite to confirm no regression**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p spades`
Expected: PASS (all existing scoring/is_over tests still green)

- [ ] **Step 7: Commit**

```bash
git add crates/spades-core/src/scoring.rs
git commit -m "feat(core): end the game when a team hits the loss floor (-200)" -- crates/spades-core/src/scoring.rs
```

---

### Task 2: Round-count backstop terminates the game

**Files:**
- Modify: `crates/spades-core/src/scoring.rs` (add `MAX_ROUNDS` const next to `MIN_POINTS`; add the round-cap check in the round-completion block, after the floor check from Task 1, before `self.round += 1;`)
- Test: `crates/spades-core/src/scoring.rs` (in `mod tests`)

**Interfaces:**
- Consumes: `MIN_POINTS` and the floor check from Task 1; pub fields `round`, `bets_placed`, `trick`, `team_a`/`team_b.cumulative_points`, `is_over`; helpers `make_trick`, `Suit`, `Rank`.
- Produces: module-private `const MAX_ROUNDS: usize = 100;`. New terminal behavior: `is_over` becomes `true` at round end when the completed round index reaches `MAX_ROUNDS` (i.e. `self.round + 1 >= MAX_ROUNDS`, evaluated before the post-round `self.round += 1`).

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/spades-core/src/scoring.rs`. This test drives the
state directly to the last allowed round (avoiding the need to simulate 100
realistic rounds) and verifies the cap ends the game with scores still inside the
(floor, max) band — proving the cap, not a score threshold, fired:

```rust
#[test]
fn test_is_over_round_cap() {
    let mut s = Scoring::new(500);
    // Jump to the final allowed round with scores comfortably inside the band.
    s.round = MAX_ROUNDS - 1;
    s.team_a.cumulative_points = 50;
    s.team_b.cumulative_points = 40;
    // bets_placed is normally grown one entry per round; pre-fill so the
    // round-end read of bets_placed[self.round] is in bounds.
    s.bets_placed = vec![[1, 1, 1, 1]; MAX_ROUNDS];

    // Play 13 tricks directly (team A wins 7, team B wins 6) — modest deltas
    // that cross neither max_points nor the loss floor.
    for t in 0..13 {
        let cards = if t < 7 {
            make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
        } else {
            make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
        };
        s.trick(0, &cards);
    }

    assert!(s.is_over, "game must end at the round cap");
    assert!(s.round >= MAX_ROUNDS);
    // Ended by the cap, not by a score threshold:
    assert!(s.team_a.cumulative_points < s.config.max_points);
    assert!(s.team_b.cumulative_points < s.config.max_points);
    assert!(s.team_a.cumulative_points > MIN_POINTS);
    assert!(s.team_b.cumulative_points > MIN_POINTS);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p spades test_is_over_round_cap`
Expected: FAIL — `cannot find value MAX_ROUNDS in this scope` (const not yet added), or assertion failure on `s.is_over`.

- [ ] **Step 3: Add the constant**

Next to `MIN_POINTS` near the top of `crates/spades-core/src/scoring.rs`, add:

```rust
/// Hard cap on rounds per game — a final backstop against any non-terminating
/// edge case beyond the loss floor. Far above any realistic game (a game to 500
/// is well under ~30 rounds).
const MAX_ROUNDS: usize = 100;
```

- [ ] **Step 4: Add the round-cap check**

In `Scoring::trick`, immediately AFTER the loss-floor check added in Task 1 and
BEFORE `self.round += 1;`, add:

```rust
            // Round cap: a final backstop. self.round is the just-completed
            // round's 0-based index and is incremented just below, so the cap
            // fires as the MAX_ROUNDS-th round completes.
            if self.round + 1 >= MAX_ROUNDS {
                self.is_over = true;
            }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p spades test_is_over_round_cap`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/spades-core/src/scoring.rs
git commit -m "feat(core): cap games at 100 rounds as a termination backstop" -- crates/spades-core/src/scoring.rs
```

---

### Task 3: Refresh the stale tie comment and run the full gate

**Files:**
- Modify: `crates/spades-core/src/lib.rs:314` (the comment inside `get_winner_ids`)

**Interfaces:**
- Consumes: nothing new.
- Produces: no behavior change — documentation only. `get_winner_ids` still returns `Err(GetError::GameNotCompleted)` on a tie (the existing `test_get_winner_ids_tie_returns_error` stays green).

- [ ] **Step 1: Update the comment**

In `crates/spades-core/src/lib.rs`, replace the comment in the tie branch of
`get_winner_ids` (currently):

```rust
                    // Unreachable: Scoring keeps is_over = false on a tie at max_points,
                    // so the game never transitions to State::Completed with equal scores.
```

with:

```rust
                    // A tie at State::Completed is reachable only when the game
                    // ends via the loss floor or round cap (max_points keeps
                    // playing on a tie). No production code calls this on a tied
                    // game; the server's rating path compares scores directly.
```

- [ ] **Step 2: Confirm the tie test still passes**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p spades test_get_winner_ids_tie_returns_error`
Expected: PASS

- [ ] **Step 3: Run fmt + clippy + the full workspace tests**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p spades
```
Expected: fmt makes no/minimal changes, clippy clean, all `spades` tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-core/src/lib.rs crates/spades-core/src/scoring.rs
git commit -m "docs(core): correct get_winner_ids tie comment for new terminal rules" -- crates/spades-core/src/lib.rs crates/spades-core/src/scoring.rs
```

---

## Post-implementation

- This is an engine behavior change. Before deploying, run the full gate: `make check`.
- Deploy via the manual path (no GitHub credits): see the team-termination notes —
  `cd ansible && DEPLOY_HOST=5.161.99.196 ANSIBLE_VAULT_PASSWORD_FILE="$PWD/.vault-pass" ansible-playbook deploy.yml -e image_tag=$(git rev-parse --short=12 HEAD)`, then `docker compose restart caddy`.
- Out of scope (separate follow-up): the eager-load-all-games-at-startup memory architecture (fix #2), and sweeping idle non-terminal games.
```
