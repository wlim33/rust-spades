# rust-spades Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land 20 prioritized improvements across correctness, code quality, server organization, and crate architecture in four reviewable phases.

**Architecture:** Single repo, four phases shipped in order. Phase 1 = bugs + local cleanup. Phase 2 = `players: [Player; 4]` refactor + doc refresh. Phase 3 = server module split + typed errors + CORS gate + CI. Phase 4 = workspace split into `spades-core` / `spades-server`, removal of `Suit::Blank`/`Rank::Blank` sentinels (2.0 bump), scoring array → counter.

**Tech Stack:** Rust 2024 edition, `axum 0.8`, `oasgen 0.25`, `tokio`, `rusqlite`, `tower-sessions`, `rustrict`. Tests use the built-in `#[cfg(test)]` framework with `ntest` and `axum-test`.

**Verification commands (run after each task):**
- `cargo build --all-features`
- `cargo test --all-features`
- `cargo clippy --all-features -- -D warnings` (allowed to fail until Phase 3 lands the CI gate; surface warnings inline)

---

## Phase 1 — Bugs & local cleanup

Each task is independently revertible. After all tasks, `cargo test --all-features` must pass.

### Task 1.1: Fix wrong `GetError` variant in `get_current_trick_cards`

**Files:**
- Modify: `src/lib.rs:300-307`
- Test: `src/lib.rs` (inline `#[cfg(test)]` module — or add to `src/tests/spades_game_api_unit.rs`)

- [ ] **Step 1: Add failing test in `src/tests/spades_game_api_unit.rs`**

```rust
#[test]
fn get_current_trick_cards_in_betting_returns_correct_variant() {
    use spades::{Game, GameTransition, GetError};
    use uuid::Uuid;
    let mut g = Game::new(
        Uuid::new_v4(),
        [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
        500,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    // In Betting state, asking for trick cards is a stage mismatch — not GameCompleted.
    assert!(matches!(
        g.get_current_trick_cards(),
        Err(GetError::Unknown)
    ));
}
```

- [ ] **Step 2: Run, expect failure**
  Run: `cargo test --all-features get_current_trick_cards_in_betting`. Expect failure (current code returns `GetError::GameCompleted`).

- [ ] **Step 3: Fix the variant**
  In `src/lib.rs` `get_current_trick_cards`, change the `State::Betting(_)` arm from `Err(GetError::GameCompleted)` to `Err(GetError::Unknown)`.

- [ ] **Step 4: Re-run; expect pass**

- [ ] **Step 5: Commit**
  `git add -A && git commit -m "fix: return GetError::Unknown (not GameCompleted) when asking for trick cards mid-bet"`

---

### Task 1.2: Make bag penalty loop while ≥ 10

**Files:**
- Modify: `src/scoring.rs:38-41`
- Test: `src/scoring.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add failing test**

```rust
#[test]
fn bag_penalty_applies_per_ten_bags() {
    // Construct a TeamState where a single round adds enough bags to cross
    // 20 simultaneously. Expect TWO -100 penalties, not one.
    let mut t = TeamState::new();
    t.bags = 9;
    // Simulate winning 12 tricks against a bid of 1 (11 bags this round).
    // After the round: bags would be 9 + 11 = 20.
    // current_round_tricks_won not used by calculate_round_totals directly —
    // it sums tricks. Build the array to sum to 12.
    t.current_round_tricks_won = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0];
    let starting_pts = t.cumulative_points;
    t.calculate_round_totals(1, false, 0, false);
    // Bid 1, won 12 -> +1*10 + 11 bags = +21 points before bag penalty
    // After loop: bags 20 -> 10 -> 0, two -100 penalties applied
    assert_eq!(t.bags, 0);
    // +10 (bet) + 11 (bag points) + 100 (nil bonus for second_bet=0, second_nil=false)
    //   - 200 (two bag penalties)
    assert_eq!(t.cumulative_points, starting_pts + 10 + 11 + 100 - 200);
}
```

- [ ] **Step 2: Run, expect failure.**
  Run: `cargo test --all-features bag_penalty_applies_per_ten_bags`. The current `if self.bags >= 10` cycles once.

- [ ] **Step 3: Change the `if` to a `while`** in `src/scoring.rs`.

```rust
while self.bags >= 10 {
    self.bags -= 10;
    self.cumulative_points -= 100;
}
```

- [ ] **Step 4: Re-run; expect pass.**

- [ ] **Step 5: Commit**
  `git add -A && git commit -m "fix: apply bag penalty once per 10 bags, not just once per round"`

---

### Task 1.3: Remove redundant double-shuffle

**Files:**
- Modify: `src/cards.rs:153,175-178`
- Modify: `src/lib.rs:557-569` (deal_cards)

- [ ] **Step 1: Read current `deal_four_players` and `new_deck`** to confirm the redundancy:
  - `new_deck()` shuffles the freshly-built deck.
  - `Game::new()` → `deal_cards()` calls `cards::shuffle(&mut self.deck)`.
  - Then `deal_cards()` calls `cards::deal_four_players(&mut self.deck)`, which shuffles again.

- [ ] **Step 2: Drop the shuffle inside `deal_four_players`**
  In `src/cards.rs:175-178`:
```rust
pub fn deal_four_players(cards: &mut Vec<Card>) -> Vec<Vec<Card>> {
    assert_eq!(cards.len(), 52);
    let mut hands = vec![vec![], vec![], vec![], vec![]];
    // ... unchanged
}
```

- [ ] **Step 3: Drop the shuffle at the end of `new_deck`** (the deck will be shuffled before dealing).
  In `src/cards.rs:124-156`, remove `shuffle(&mut cards);` near the end.

- [ ] **Step 4: Verify** `Game::deal_cards` (in `src/lib.rs`) still calls `cards::shuffle(&mut self.deck)`, which is now the *only* shuffle. Leave it in place.

- [ ] **Step 5: Run tests.**
  `cargo test --all-features` — all pass.

- [ ] **Step 6: Commit**
  `git add -A && git commit -m "refactor: remove redundant deck shuffles; one shuffle in deal_cards is enough"`

---

### Task 1.4: Fix misleading tie-handling comment in `get_winner_ids`

**Files:**
- Modify: `src/lib.rs:320-336`

- [ ] **Step 1: Re-read the scoring tie logic** in `src/scoring.rs:122-134`. A tie at max_points keeps `is_over = false`, so `State::Completed` is only reached with a non-tie. The current comment in `get_winner_ids` says "Tie should not happen (is_over prevents it)" — accurate but cryptic.

- [ ] **Step 2: Replace the comment**

```rust
} else {
    // Unreachable: scoring keeps `is_over = false` on a tie at max_points,
    // so the game does not transition to State::Completed with equal scores.
    // Guard returned for safety.
    return Err(GetError::GameNotCompleted);
}
```

- [ ] **Step 3: Commit**
  `git add -A && git commit -m "docs: clarify why the tie branch in get_winner_ids is unreachable"`

---

### Task 1.5: Make `Card` `Copy`

**Files:**
- Modify: `src/cards.rs:68-73`
- Modify: `src/cards.rs:147-151` (`new_deck` constructor)
- Modify: `src/lib.rs:409,414,541,548` (remove `.clone()` on `Card`/`&[Card; 4]` where shadowed)
- Modify: anywhere else clippy flags

- [ ] **Step 1: Add `Copy` to `Card`'s derive** in `src/cards.rs:68`:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(oasgen::OaSchema))]
pub struct Card { pub suit: Suit, pub rank: Rank }
```

- [ ] **Step 2: Build to discover ripple sites**
  Run: `cargo build --all-features`. Compile errors are fine; clippy warnings on `.clone()` are the target.

- [ ] **Step 3: Run clippy and remove unneeded clones**
  Run: `cargo clippy --all-features -- -W clippy::clone_on_copy`.
  Replace `card.clone()` with `card`, `c.clone()` with `*c` (or `c` if already a value), etc.
  Specifically in `src/lib.rs`:
  - line ~409: `self.hands_played.last_mut().unwrap()[self.current_player_index] = card;` already takes ownership — no change needed but verify.
  - line 414: `Some(self.hands_played.last().unwrap().clone())` — that clones a `[Card; 4]`. Array of `Copy` items derives `Copy`, so swap to `Some(*self.hands_played.last().unwrap())`.
  - lines 541, 548 (`get_legal_cards`): `Ok(hand.clone())` clones a `Vec<Card>` — leave it (Vec isn't Copy).
  - In `src/cards.rs:150`: `Card { suit: s.clone(), rank: r.clone() }` — replace clones with `*s`, `*r` (both are Copy).

- [ ] **Step 4: Build, test, clippy clean**
  `cargo test --all-features` and `cargo clippy --all-features` — both clean.

- [ ] **Step 5: Commit**
  `git add -A && git commit -m "refactor: make Card Copy, drop unneeded clones"`

---

### Task 1.6: Cache `Sqids` in a `OnceLock`

**Files:**
- Modify: `src/lib.rs:70-118`

- [ ] **Step 1: Replace the factory with a `OnceLock`**

```rust
use std::sync::OnceLock;

fn sqids_instance() -> &'static Sqids {
    static SQIDS: OnceLock<Sqids> = OnceLock::new();
    SQIDS.get_or_init(|| {
        Sqids::builder()
            .min_length(6)
            .build()
            .expect("valid sqids config")
    })
}
```

- [ ] **Step 2: Update call sites** — every `sqids_instance().encode(...)` / `.decode(...)` already works because `&Sqids` has those methods. No call site changes required.

- [ ] **Step 3: Test**
  `cargo test --all-features` — pass.

- [ ] **Step 4: Commit**
  `git add -A && git commit -m "perf: cache Sqids builder in OnceLock instead of rebuilding per call"`

---

### Task 1.7: Drop dead tuple destructure in score/bags getters

**Files:**
- Modify: `src/lib.rs:219-245`

- [ ] **Step 1: Simplify each of `get_team_a_score`, `get_team_b_score`, `get_team_a_bags`, `get_team_b_bags`**

```rust
pub fn get_team_a_score(&self) -> Result<&i32, GetError> {
    match self.state {
        State::NotStarted => Err(GetError::GameNotStarted),
        _ => Ok(&self.scoring.team_a.cumulative_points),
    }
}
```

Apply the same shape to all four.

- [ ] **Step 2: Test**
  `cargo test --all-features` — pass.

- [ ] **Step 3: Commit**
  `git add -A && git commit -m "refactor: drop dead current_player_index from score/bags getters"`

---

### Task 1.8: Implement "spades broken" rule

**Files:**
- Modify: `src/lib.rs:160-208` (Game fields & `Game::new`)
- Modify: `src/lib.rs:373-447` (`play` → Card case)
- Modify: `src/lib.rs:534-555` (`get_legal_cards`)
- Test: `src/tests/spades_game_api_unit.rs`

**Background:** Standard Spades rules forbid leading with a spade until either (a) someone has played a spade off-suit on a previous trick ("spades broken"), or (b) the leader has only spades remaining. The current code allows leading spades any time.

- [ ] **Step 1: Add failing test**

```rust
#[test]
fn cannot_lead_spade_before_broken_when_non_spades_available() {
    use spades::{Card, Game, GameTransition, Rank, Suit, TransitionError};
    use uuid::Uuid;
    // Construct a game where player 0 leads first trick and has both spades and
    // non-spades in hand. Verify the lead-spade transition is rejected with
    // a SpadesNotBroken error.
    // (Use a deterministic deck or assert behavior on natural shuffle by retrying.)
    // ... see test_helpers below
}
```

(Helper: a `with_hands` test ctor lets us bypass shuffle. If that's too invasive, set `spades_broken: false` and stub `play` to return the new error on lead-spade. See Step 4.)

- [ ] **Step 2: Add a new error variant** in `src/result.rs`:

```rust
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TransitionError {
    AlreadyStarted,
    NotStarted,
    CardInBettingStage,
    BetInTrickStage,
    CompletedGame,
    CardNotInHand,
    CardIncorrectSuit,
    SpadesNotBroken,
}
```

Add the matching `Display` arm: `"Error: Cannot lead a spade until spades are broken."`

- [ ] **Step 3: Add `spades_broken: bool` to `Game`**

In `src/lib.rs` struct `Game`:
```rust
#[serde(default)]
spades_broken: bool,
```

In `Game::new`, initialize `spades_broken: false`.

- [ ] **Step 4: Enforce in `play`**

In the `GameTransition::Card` → `State::Trick(rotation_status)` arm in `src/lib.rs`, after the existing `if rotation_status == 0 { self.leading_suit = card.suit; }`:

```rust
if rotation_status == 0 && card.suit == Suit::Spade && !self.spades_broken {
    let only_spades = player_hand.iter().all(|c| c.suit == Suit::Spade);
    if !only_spades {
        return Err(TransitionError::SpadesNotBroken);
    }
}
```

After successfully playing any spade off-suit (any spade played when `leading_suit != Spade`), set `self.spades_broken = true`. Add this right after the existing card-validity checks:

```rust
if card.suit == Suit::Spade && self.leading_suit != Suit::Spade {
    self.spades_broken = true;
}
```

- [ ] **Step 5: Reflect in `get_legal_cards`**

When `rotation_status == 0`:
```rust
if !self.spades_broken {
    let non_spades: Vec<Card> = hand.iter().filter(|c| c.suit != Suit::Spade).copied().collect();
    if !non_spades.is_empty() {
        return Ok(non_spades);
    }
}
Ok(hand.clone())
```

- [ ] **Step 6: Reset `spades_broken` on new rounds (after a round of 13 tricks)**

In the `rotation_status == 3` + `self.scoring.in_betting_stage` arm of `play` (the round-end branch), add `self.spades_broken = false;` alongside resetting `current_player_index`.

- [ ] **Step 7: Re-run the test from Step 1**
  Now passes.

- [ ] **Step 8: Add a complementary test**

```rust
#[test]
fn lead_spade_allowed_after_spades_broken() { /* play a spade off-suit, then verify leading a spade succeeds */ }

#[test]
fn lead_spade_allowed_when_only_spades_in_hand() { /* arrange a hand of only spades, verify allowed */ }
```

- [ ] **Step 9: Test, clippy**
  All pass.

- [ ] **Step 10: Commit**
  `git add -A && git commit -m "feat: enforce 'spades not broken' lead rule (new TransitionError::SpadesNotBroken)"`

---

### Task 1.9: Tighten Phase 1 outputs

- [ ] Run `cargo test --all-features`, `cargo clippy --all-features -- -D warnings`. If clippy errors persist, address them inline.

- [ ] Tag Phase 1: `git tag phase-1-complete`.

---

## Phase 2 — Player array refactor + IMPROVEMENTS.md refresh

This phase makes the codebase materially easier to reason about. Land it as a single PR.

### Task 2.1: Convert `Game` to use `players: [Player; 4]`

**Files:**
- Modify: `src/lib.rs` (entire `Game` struct + all methods that pattern-match `0..3`)
- Modify: `src/sqlite_store.rs` (serde format change — see Step 5)
- Test: All existing tests must continue to pass.

- [ ] **Step 1: Replace fields**

```rust
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Game {
    id: Uuid,
    state: State,
    scoring: scoring::Scoring,
    current_player_index: usize,
    deck: Vec<cards::Card>,
    hands_played: Vec<[cards::Card; 4]>,
    leading_suit: Suit,
    players: [Player; 4],          // was player_a..player_d
    #[serde(default)] timer_config: Option<TimerConfig>,
    #[serde(default)] player_clocks: Option<PlayerClocks>,
    #[serde(default)] turn_started_at_epoch_ms: Option<u64>,
    #[serde(default)] last_trick_winner: Option<usize>,
    #[serde(default)] last_completed_trick: Option<[cards::Card; 4]>,
    #[serde(default)] spades_broken: bool,
}
```

- [ ] **Step 2: Update `Game::new`**

```rust
players: [
    Player::new(player_ids[0]),
    Player::new(player_ids[1]),
    Player::new(player_ids[2]),
    Player::new(player_ids[3]),
],
```

- [ ] **Step 3: Rewrite player-dispatching getters**

Replace the four-arm match in `get_current_player_id`:

```rust
pub fn get_current_player_id(&self) -> Result<&Uuid, GetError> {
    match self.state {
        State::NotStarted => Err(GetError::GameNotStarted),
        State::Completed | State::Aborted => Err(GetError::GameCompleted),
        State::Betting(_) | State::Trick(_) => Ok(&self.players[self.current_player_index].id),
    }
}
```

Apply same shape to `get_current_hand`. For `get_hand_by_player_id`:

```rust
pub fn get_hand_by_player_id(&self, player_id: Uuid) -> Result<&Vec<Card>, GetError> {
    self.players.iter()
        .find(|p| p.id == player_id)
        .map(|p| &p.hand)
        .ok_or(GetError::InvalidUuid)
}
```

`set_player_name`:
```rust
pub fn set_player_name(&mut self, player_id: Uuid, name: Option<String>) -> Result<(), GetError> {
    let p = self.players.iter_mut()
        .find(|p| p.id == player_id)
        .ok_or(GetError::InvalidUuid)?;
    p.name = name;
    Ok(())
}
```

`get_player_names`:
```rust
pub fn get_player_names(&self) -> [(Uuid, Option<&str>); 4] {
    std::array::from_fn(|i| (self.players[i].id, self.players[i].name.as_deref()))
}
```

`get_last_trick_winner_id`:
```rust
pub fn get_last_trick_winner_id(&self) -> Option<Uuid> {
    self.last_trick_winner.map(|idx| self.players[idx].id)
}
```

For deprecated `get_hand`, replace the silently-aliasing match with bounds-checked access:
```rust
#[deprecated(...)]
pub fn get_hand(&self, player: usize) -> Result<&Vec<Card>, GetError> {
    self.players.get(player).map(|p| &p.hand).ok_or(GetError::InvalidUuid)
}
```

For `play`'s inner card-removal match (~lines 386-392), replace with `&mut self.players[self.current_player_index].hand`.

`deal_cards`:
```rust
fn deal_cards(&mut self) {
    cards::shuffle(&mut self.deck);
    let mut hands = cards::deal_four_players(&mut self.deck);
    for i in (0..4).rev() {
        self.players[i].hand = hands.pop().unwrap();
        self.players[i].hand.sort();
    }
}
```

(Note: `pop()` returns the last element, so iterating in reverse keeps the same player order as before. Verify with a unit test that hand contents match what the prior code produced for a fixed seed — see Step 7.)

- [ ] **Step 4: Find any remaining direct `player_a/b/c/d` refs**
  Run: `grep -n "player_a\|player_b\|player_c\|player_d" src/`
  Replace each.

- [ ] **Step 5: Address serde format break**

The serialized JSON in SQLite changes from `{"player_a": {...}, "player_b": {...}, ...}` to `{"players": [...]}`. Two options:
  (a) Add a `#[serde(rename = "players")]` and accept that existing DBs are wiped (acceptable for pre-1.0 work but project is at 1.2.8 — coordinate with user).
  (b) Implement custom `Deserialize` that accepts both shapes.

For Phase 2, **prefer (a)** — write a tiny one-shot migration script `scripts/migrate_player_fields.sh` that uses `sqlite3` + `jq` to rewrite each game row's JSON. Document in `IMPROVEMENTS.md` Phase 2 entry.

```bash
# scripts/migrate_player_fields.sh
sqlite3 "$1" 'SELECT id, data FROM games' | while IFS='|' read -r id data; do
    new=$(echo "$data" | jq '{id, state, scoring, current_player_index, deck, hands_played, leading_suit, players: [.player_a, .player_b, .player_c, .player_d], timer_config, player_clocks, turn_started_at_epoch_ms, last_trick_winner, last_completed_trick, spades_broken}')
    sqlite3 "$1" "UPDATE games SET data = ? WHERE id = ?" "$new" "$id"
done
```

- [ ] **Step 6: Build incrementally**
  `cargo build --all-features`. Fix compile errors one by one.

- [ ] **Step 7: Add a regression test pinning deal-order**

```rust
#[test]
fn deal_cards_assigns_to_all_four_players() {
    // Each player gets 13 cards, total 52 unique
    use spades::{Game, GameTransition};
    use uuid::Uuid;
    let ids = [Uuid::new_v4(); 4];
    let mut g = Game::new(Uuid::new_v4(), ids, 500, None);
    g.play(GameTransition::Start).unwrap();
    let counts: Vec<usize> = (0..4)
        .map(|i| g.get_hand_by_player_id(ids[i]).unwrap().len())
        .collect();
    // ids are all the same in this test — use a coverage-style assertion instead
    // Better: use distinct ids
    // ...
}
```

(Rewrite with distinct UUIDs and assert 13 cards each, no duplicates across the union.)

- [ ] **Step 8: Run full test suite**
  `cargo test --all-features` — pass. `cargo clippy --all-features -- -D warnings` — clean.

- [ ] **Step 9: Commit**
  `git add -A && git commit -m "refactor: replace player_a/b/c/d with players: [Player; 4]"`

---

### Task 2.2: Refresh `IMPROVEMENTS.md`

**Files:**
- Modify: `IMPROVEMENTS.md`

- [ ] **Step 1: Remove stale notes**

The line: `` `result.rs` uses deprecated `Error::description` and `Error::cause` — should migrate to `Display` and `Error::source` `` is wrong — neither method appears in `result.rs`. Remove it.

The note "Some long methods (notably `play`) could be broken up" — annotate as "addressed in Phase 4 via state-pattern split" or remove if not planned.

- [ ] **Step 2: Add new implemented items** for everything Phase 1 just landed:
  - Spades-broken rule
  - Bag penalty per-10-bags
  - `Card: Copy`
  - `Sqids` cached
  - Trick-cards stage-error variant fixed

- [ ] **Step 3: Add a "Code Quality Notes" entry**
  - `GetError` does not implement `std::error::Error` — will land in Phase 3 with the typed-error pass.

- [ ] **Step 4: Commit**
  `git add IMPROVEMENTS.md && git commit -m "docs: refresh IMPROVEMENTS.md to reflect Phase 1 + 2"`

---

### Task 2.3: Phase 2 close-out

- [ ] All tests pass; clippy clean.
- [ ] Tag: `git tag phase-2-complete`.

---

## Phase 3 — Server module split, typed errors, CORS gate, CI

### Task 3.1: Implement `Display` and `Error` for `GetError`, `GameManagerError`, `ChallengeError`

**Files:**
- Modify: `src/result.rs`
- Modify: `src/game_manager.rs`
- Modify: `src/challenges.rs`

- [ ] **Step 1: `GetError`**

In `src/result.rs`, below the existing `impl fmt::Display for GetError`:
```rust
impl std::error::Error for GetError {}
```

(Display is already present; we're just adding the Error impl.)

- [ ] **Step 2: `GameManagerError`**

Currently `enum GameManagerError { GameNotFound, GameError(String), LockError }`. Add `Display` + `Error`:

```rust
impl std::fmt::Display for GameManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GameNotFound => write!(f, "Game not found"),
            Self::GameError(s) => write!(f, "Game error: {s}"),
            Self::LockError => write!(f, "Internal lock error"),
        }
    }
}
impl std::error::Error for GameManagerError {}
```

- [ ] **Step 3: `ChallengeError`**

Same shape. Read the existing variants in `src/challenges.rs` and write a `Display` arm for each, then `impl Error`.

- [ ] **Step 4: Build, test**
  `cargo test --all-features` — pass.

- [ ] **Step 5: Commit**
  `git add -A && git commit -m "feat: implement Display + Error for GetError/GameManagerError/ChallengeError"`

---

### Task 3.2: Replace `format!("{:?}", e)` with `format!("{}", e)` everywhere

**Files:**
- Modify: `src/bin/server.rs` (many sites)
- Modify: `src/game_manager.rs` (any error-formatting sites)

- [ ] **Step 1: Find all sites**
  Run: `grep -n '"{:?}"' src/bin/server.rs src/game_manager.rs src/challenges.rs src/matchmaking.rs`

- [ ] **Step 2: For each formatter of a known-Display error, switch to `{}`**

```rust
Json(ErrorResponse { error: format!("{}", e) }),
```

Leave `Debug` in `eprintln!` log lines unless they're user-facing.

- [ ] **Step 3: Build, test**
  `cargo build --all-features`, `cargo test --all-features` — pass.

- [ ] **Step 4: Commit**
  `git add -A && git commit -m "fix: use Display (not Debug) for error response bodies"`

---

### Task 3.3: Convert `GameManagerError::GameError(String)` to carry typed `TransitionError`

**Files:**
- Modify: `src/game_manager.rs`
- Modify: `src/bin/server.rs` (error mapping)

- [ ] **Step 1: Add a typed variant**

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum GameManagerError {
    GameNotFound,
    Transition(TransitionError),
    Get(GetError),
    LockError,
    Other(String),
}
```

Adjust `Display`/`Error`.

- [ ] **Step 2: Update production sites**

In `src/game_manager.rs`, find `GameError(format!(...))` calls and replace with the structured variant.

- [ ] **Step 3: Update server status-code mapping**

In `src/bin/server.rs`, the various map_err closures should branch on the typed variant:
```rust
let status = match &e {
    GameManagerError::GameNotFound => StatusCode::NOT_FOUND,
    GameManagerError::Transition(TransitionError::CardNotInHand)
    | GameManagerError::Transition(TransitionError::CardIncorrectSuit)
    | GameManagerError::Transition(TransitionError::SpadesNotBroken) => StatusCode::BAD_REQUEST,
    GameManagerError::Transition(_) => StatusCode::CONFLICT,
    GameManagerError::Get(_) => StatusCode::BAD_REQUEST,
    GameManagerError::LockError => StatusCode::INTERNAL_SERVER_ERROR,
    GameManagerError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
};
```

- [ ] **Step 4: Add a test that the JSON body for a known transition error is structured**

Use `axum-test` to exercise `POST /games/:id/transition` with a deliberately invalid move and assert `response.json()["error"].contains("CardNotInHand")` (or whatever Display string we settled on).

- [ ] **Step 5: Build, test**

- [ ] **Step 6: Commit**
  `git add -A && git commit -m "refactor: carry typed TransitionError through GameManagerError"`

---

### Task 3.4: Gate CORS behind a runtime config

**Files:**
- Modify: `src/bin/server.rs:194-244` (main + build_router)

- [ ] **Step 1: Parse a `--cors-allow-origin` flag** (repeatable) and default to *no* CORS layer.

```rust
let cors_origins: Vec<String> = std::env::args()
    .enumerate()
    .filter_map(|(i, a)| if a == "--cors-allow-origin" { std::env::args().nth(i + 1) } else { None })
    .collect();
let cors = if cors_origins.is_empty() {
    None
} else if cors_origins.iter().any(|s| s == "*") {
    Some(CorsLayer::permissive())
} else {
    let mut layer = CorsLayer::new();
    for o in &cors_origins {
        layer = layer.allow_origin(o.parse::<axum::http::HeaderValue>().unwrap());
    }
    Some(layer)
};
```

Apply `cors` to the router only if `Some`.

- [ ] **Step 2: Update `build_router` signature** if needed (or do this in `main`).

- [ ] **Step 3: Document in `SERVER.md`** under a new "CORS" subsection that you must pass `--cors-allow-origin <origin>` (or `*` for dev).

- [ ] **Step 4: Smoke test**
  `cargo run --features server -- --port 0 &` (in CI), curl with/without `Origin:` header — verify the layer responds appropriately.

- [ ] **Step 5: Commit**
  `git add -A && git commit -m "feat: gate CORS behind --cors-allow-origin (default: no CORS layer)"`

---

### Task 3.5: Split `bin/server.rs` into modules

**Files:**
- Create: `src/bin/server/main.rs` (move from `src/bin/server.rs`)
- Create: `src/bin/server/handlers/mod.rs`
- Create: `src/bin/server/handlers/games.rs`
- Create: `src/bin/server/handlers/matchmaking.rs`
- Create: `src/bin/server/handlers/challenges.rs`
- Create: `src/bin/server/handlers/players.rs`
- Create: `src/bin/server/presence.rs`
- Create: `src/bin/server/ws.rs`
- Create: `src/bin/server/sse.rs`
- Create: `src/bin/server/dto.rs` (request/response structs)
- Modify: `Cargo.toml` (set `[[bin]] path = "src/bin/server/main.rs"`)

- [ ] **Step 1: Pre-flight** — `cargo test --all-features` green.

- [ ] **Step 2: Create `src/bin/server/` directory** and move `src/bin/server.rs` to `src/bin/server/main.rs`. (Cargo accepts binary at `src/bin/<name>/main.rs`.)

- [ ] **Step 3: Update `Cargo.toml`**:
```toml
[[bin]]
name = "spades-server"
path = "src/bin/server/main.rs"
required-features = ["server"]
```

- [ ] **Step 4: Confirm `cargo build --features server` still works** (no functional changes yet).

- [ ] **Step 5: Carve out `dto.rs`**
  Move every `#[derive(... Serialize, Deserialize ...)]` request/response struct (`CreateGameRequest`, `TransitionRequest`, `TransitionType`, `TransitionResponse`, `ErrorResponse`, `PlayerUrlResponse`, `SeekRequest`, `SetNameRequest`, `JoinChallengeRequest`, `CancelChallengeRequest`, `WsQuery`, `PlayerPresenceEntry`, `PresenceSnapshot`, `ServerEvent`, `UserSession`, `SessionPlayerResponse`, `SetDisplayNameRequest`) to `dto.rs`. Each should be `pub`. Re-import in `main.rs`.

- [ ] **Step 6: Carve out `presence.rs`**
  Move `PresenceTracker` impl + `PresenceSnapshot` usage there. (PresenceSnapshot was just moved to dto.rs; the tracker can import it.)

- [ ] **Step 7: Carve out `ws.rs`**
  Move the WebSocket handler `game_ws` + helpers + the `WsQuery`-driven session-validation logic.

- [ ] **Step 8: Carve out `sse.rs`**
  Move the SSE handlers for seek/challenges.

- [ ] **Step 9: Carve out handler files**
  `handlers/games.rs` — `create_game`, `list_games`, `get_game_state`, `get_game_by_short_id_handler`, `get_game_by_player_url`, `delete_game`, `make_transition`, `get_hand`.
  `handlers/players.rs` — `set_player_name`, `get_player`, `set_display_name`, `get_presence`.
  `handlers/matchmaking.rs` — `seek`, `list_seeks_handler`, `queue_sizes_handler`.
  `handlers/challenges.rs` — `create_challenge_handler`, `list_challenges_handler`, `get_challenge_handler`, `get_challenge_by_short_id_handler`, `join_challenge_handler`, `cancel_challenge_handler`.

- [ ] **Step 10: `main.rs` shrinks** to: imports, `AppState`, `build_router`, `main`, plus startup banner.

- [ ] **Step 11: After each move, run `cargo build --features server`** to catch missing imports immediately.

- [ ] **Step 12: Verify**
  `cargo test --all-features` — pass. `cargo clippy --features server -- -D warnings` — clean.

- [ ] **Step 13: Commit** (one commit per logical move is fine; final commit when done):
  `git add -A && git commit -m "refactor: split bin/server into modules (handlers, presence, ws, sse, dto)"`

---

### Task 3.6: Drop the dead Travis badge & add GitHub Actions CI

**Files:**
- Modify: `Cargo.toml` (remove `[badges]` block)
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Cargo.toml — delete**

```toml
[badges]
travis-ci = { repository = "wlim33/rust-spades", branch = "master" }
```

- [ ] **Step 2: Add `.github/workflows/ci.yml`**

```yaml
name: CI
on:
  push: { branches: [master] }
  pull_request:
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-features -- -D warnings
      - run: cargo test --all-features
      - run: cargo build --all-features --release
```

- [ ] **Step 3: Commit**
  `git add -A && git commit -m "ci: add GitHub Actions workflow; drop dead Travis badge"`

---

### Task 3.7: Phase 3 close-out

- [ ] All tests pass; clippy clean; new CI workflow green on push.
- [ ] Tag: `git tag phase-3-complete`.

---

## Phase 4 — Workspace split, sentinel removal, scoring counter

This phase is intentionally aggressive. `Suit::Blank`/`Rank::Blank` removal is a public-API break; bump to **2.0.0**.

### Task 4.1: Replace `[i32; 13]` round-trick counter with `i32`

**Files:**
- Modify: `src/scoring.rs`

This is the smallest swing in Phase 4 — land first.

- [ ] **Step 1: In `TeamState`** replace `current_round_tricks_won: [i32; 13]` with `current_round_tricks_won: i32`.

- [ ] **Step 2: In `Scoring::trick`** replace `self.team_*.current_round_tricks_won[self.trick] += 1;` with `self.team_*.current_round_tricks_won += 1;`.

- [ ] **Step 3: In `calculate_round_totals`** the line `let team_tricks: i32 = self.current_round_tricks_won.iter().sum();` becomes `let team_tricks: i32 = self.current_round_tricks_won;`.

- [ ] **Step 4: Reset on round end** — replace `current_round_tricks_won = [0; 13]` with `= 0`.

- [ ] **Step 5: Build, test**
  `cargo test --all-features` — pass.

- [ ] **Step 6: Commit**
  `git add -A && git commit -m "refactor: collapse current_round_tricks_won array to single counter"`

---

### Task 4.2: Remove `Suit::Blank` / `Rank::Blank` sentinels

**Files:**
- Modify: `src/cards.rs`
- Modify: `src/lib.rs` (Game.hands_played, leading_suit, get_current_trick_cards, get_last_completed_trick, etc.)
- Modify: `src/scoring.rs` (trick → only meaningful cards)
- Modify: `src/game_manager.rs` (GameStateResponse.table_cards, last_completed_trick)
- Test: Add coverage for `table_cards = None` mid-trick.

**Strategy:** Replace `[Card; 4]` "trick pot" everywhere with `[Option<Card>; 4]`. Replace `leading_suit: Suit` (with Blank as "no lead") with `leading_suit: Option<Suit>`.

- [ ] **Step 1: Card.rs**
  Delete `Blank` arms from `Suit` and `Rank` enums. Remove `Blank => write!(f, " ")` arms from `Debug` impls.
  Delete `new_pot()` (no longer needed — use `[None, None, None, None]`).

- [ ] **Step 2: Game struct**
  `hands_played: Vec<[Option<Card>; 4]>` (note: `Option<Card>` is `Copy` once `Card` is `Copy`).
  `leading_suit: Option<Suit>`
  `last_completed_trick: Option<[Option<Card>; 4]>` → at end of trick all four are `Some(_)`, simplify to `Option<[Card; 4]>` by unwrapping at completion.

- [ ] **Step 3: Game::new** initialize `hands_played: vec![[None; 4]]`, `leading_suit: None`.

- [ ] **Step 4: play()** — wherever it pushes a card into `hands_played`:
```rust
self.hands_played.last_mut().unwrap()[self.current_player_index] = Some(card);
```
At trick-end, unwrap all four to build `[Card; 4]` for `last_completed_trick`:
```rust
let trick = self.hands_played.last().unwrap();
let completed: [Card; 4] = [
    trick[0].unwrap(), trick[1].unwrap(),
    trick[2].unwrap(), trick[3].unwrap(),
];
self.last_completed_trick = Some(completed);
```
Pass `&completed` to `self.scoring.trick(...)`.

When starting a new trick: `self.hands_played.push([None; 4])` and `self.leading_suit = None`. When setting leading suit: `self.leading_suit = Some(card.suit)`.

Leading-suit checks elsewhere: `if let Some(ls) = self.leading_suit { if ls != card.suit && ... }`.

- [ ] **Step 5: get_current_trick_cards** returns `Option<[Option<Card>; 4]>` *or* keep the existing `Result<&[Card; 4], GetError>` but filter to only fully-played tricks. Simpler: change return type to `Result<&[Option<Card>; 4], GetError>`. Update `GameStateResponse.table_cards` accordingly: `Option<[Option<Card>; 4]>`.

- [ ] **Step 6: scoring.rs** — `trick(starting_player_index, cards: &[Card; 4])` already takes fully-played cards; no change required.

- [ ] **Step 7: get_trick_winner** in `cards.rs` — no change.

- [ ] **Step 8: oasgen schemas**
  `Option<Suit>`, `Option<Card>` are `OaSchema`-compatible if the inner is. Build, observe errors, fix.

- [ ] **Step 9: Frontend/API consumers**
  Document the breaking change in `SERVER.md`: `table_cards` is now `[Card|null, Card|null, Card|null, Card|null]` (or `null` if no trick in progress). Old: empty/Blank cards in slots; new: explicit nulls.

- [ ] **Step 10: Build, test, clippy**
  Likely many ripple sites — fix until green.

- [ ] **Step 11: Add a serialization test**
  Confirm `serde_json::to_string(&Some(card))` round-trips, that `[None; 4]` serializes to `[null, null, null, null]`.

- [ ] **Step 12: Commit**
  `git add -A && git commit -m "feat!: remove Suit::Blank/Rank::Blank sentinels; trick slots are Option<Card>"`

---

### Task 4.3: Bump version to 2.0.0

**Files:**
- Modify: `Cargo.toml:3`
- Modify: `IMPROVEMENTS.md` (note 2.0 break)
- Modify: `readme.md` (`spades = "2.0"`)

- [ ] **Step 1**: `version = "2.0.0"` in `Cargo.toml`.

- [ ] **Step 2**: Add `## Breaking Changes (2.0)` to `IMPROVEMENTS.md`.

- [ ] **Step 3**: Update install snippet in `readme.md`.

- [ ] **Step 4: Commit**
  `git add -A && git commit -m "chore: bump to 2.0.0 (Suit/Rank Blank sentinel removal)"`

---

### Task 4.4: Split into `spades-core` + `spades-server` crates

**Files:**
- Create: `Cargo.toml` (root workspace manifest)
- Create: `crates/spades-core/Cargo.toml`
- Create: `crates/spades-core/src/` (move all library `.rs` here)
- Create: `crates/spades-server/Cargo.toml`
- Create: `crates/spades-server/src/` (move server bin + game_manager, challenges, matchmaking, sqlite_store, validation here)
- Move: `tests/` → `crates/spades-core/tests/` (or split)
- Move: `examples/` → `crates/spades-server/examples/` (server_demo.sh references the server)

**Strategy:** core has zero server-only deps; server depends on core. Public API of core is `Game`, `GameTransition`, `Card`, `Suit`, `Rank`, `State`, `Player` (if pub), `TimerConfig`, `PlayerClocks`, errors, `uuid_to_short_id`/`short_id_to_uuid`/`encode_player_url`/`decode_player_url`, `ai`.

- [ ] **Step 1: Make root a virtual workspace**

`Cargo.toml`:
```toml
[workspace]
members = ["crates/spades-core", "crates/spades-server"]
resolver = "3"

[workspace.package]
edition = "2024"
authors = ["William <limwilliam23@gmail.com>"]
repository = "https://github.com/wlim33/rust-spades"
license = "MIT"

[workspace.dependencies]
uuid = { version = "1.18", features = ["v4", "serde"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

- [ ] **Step 2: Create `crates/spades-core/Cargo.toml`**

```toml
[package]
name = "spades-core"
version = "2.0.0"
description = "Core 4-player Spades card-game logic."
edition.workspace = true
authors.workspace = true
repository.workspace = true
license.workspace = true
categories = ["games", "game-engines", "simulation"]
keywords = ["spades", "cards", "state", "machine"]
readme = "../../readme.md"

[lib]
name = "spades"

[dependencies]
rand = "0.8"
uuid.workspace = true
serde.workspace = true
serde_json.workspace = true
sqids = "0.4"
```

- [ ] **Step 3: Move core files** to `crates/spades-core/src/`:
  `lib.rs`, `cards.rs`, `game_state.rs`, `result.rs`, `scoring.rs`, `ai.rs`, plus `tests/` module.
  Remove the `#[cfg(feature = "server")]` gating on `game_manager`, `matchmaking`, `sqlite_store`, `validation`, `challenges`, `oasgen_impls` from this `lib.rs` — they move to the server crate.

- [ ] **Step 4: Create `crates/spades-server/Cargo.toml`**

```toml
[package]
name = "spades-server"
version = "2.0.0"
description = "Spades game server (axum HTTP/WebSocket)."
edition.workspace = true
authors.workspace = true
repository.workspace = true
license.workspace = true

[[bin]]
name = "spades-server"
path = "src/bin/server/main.rs"

[lib]
path = "src/lib.rs"

[dependencies]
spades-core = { path = "../spades-core", version = "2.0.0" }
tokio = { version = "1.0", features = ["full"] }
axum = { version = "0.8", features = ["ws"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors"] }
tokio-stream = "0.1"
futures-util = "0.3"
async-stream = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }
rustrict = "0.7"
tower-sessions = "0.14"
tower-sessions-sqlx-store = { version = "0.15", features = ["sqlite"] }
oasgen = { version = "0.25", features = ["axum", "uuid", "swagger-ui"] }
time = "0.3"
uuid.workspace = true
serde.workspace = true
serde_json.workspace = true

[dev-dependencies]
axum-test = "18"
ntest = "0.9"
```

- [ ] **Step 5: Move server files** to `crates/spades-server/src/`:
  `game_manager.rs`, `matchmaking.rs`, `sqlite_store.rs`, `validation.rs`, `challenges.rs`, `oasgen_impls.rs`, `bin/server/...`.

- [ ] **Step 6: Create `crates/spades-server/src/lib.rs`** that re-exports the server's public surface so tests can drive it:

```rust
pub mod game_manager;
pub mod matchmaking;
pub mod sqlite_store;
pub mod validation;
pub mod challenges;
pub use spades as spades_core;
```

- [ ] **Step 7: Rewrite imports** across moved files:
  - In server-crate files, replace `use crate::{Game, ...}` with `use spades::{Game, ...}` (the core crate re-exports as `spades`).
  - `use crate::ai::AiStrategy` → `use spades::ai::AiStrategy`.

- [ ] **Step 8: Build the workspace**
  `cargo build --workspace` — fix imports until clean.

- [ ] **Step 9: Tests**
  `cargo test --workspace` — pass.

- [ ] **Step 10: Update `IMPROVEMENTS.md`, `readme.md`, `SERVER.md`** to reflect the workspace layout.

  - `readme.md` install snippet stays `spades = "2.0"` (refers to the core crate; the published name is `spades` per `[lib] name = "spades"`).
  - `SERVER.md` install snippet should mention `cargo run -p spades-server`.

- [ ] **Step 11: Cargo.lock**
  Workspace produces one `Cargo.lock`. Commit it.

- [ ] **Step 12: Commit**
  `git add -A && git commit -m "refactor: split crate into spades-core (lib) and spades-server (binary)"`

---

### Task 4.5: Phase 4 close-out

- [ ] `cargo test --workspace --all-features` clean. `cargo clippy --workspace --all-features -- -D warnings` clean. CI green.
- [ ] Tag: `git tag phase-4-complete v2.0.0`.

---

## Final self-review checklist

- [ ] Phase 1 tag exists, all 8 commits land.
- [ ] Phase 2 player array touches every former `player_a/b/c/d` reference (grep returns no matches).
- [ ] Phase 3 server module split: `wc -l src/bin/server/main.rs` is reasonable (< 300 lines).
- [ ] Phase 4 workspace: `cargo metadata --format-version 1 | jq '.workspace_members'` lists both crates.
- [ ] Version bumped to 2.0.0; readme + IMPROVEMENTS reflect breaking change.
- [ ] `IMPROVEMENTS.md` accurately reflects state at end of Phase 4 (no stale entries from earlier review).
