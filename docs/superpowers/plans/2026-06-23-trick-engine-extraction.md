# Trick-Engine Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract a generic, game-agnostic trick-taking engine (`trick-engine`) out of `spades-core`, with spades re-expressed as the first `Ruleset` implementation behind an unchanged public `spades` API.

**Architecture:** Three-layer stack — `trick-notation` (data) → `trick-engine` (generic round/trick/rotation state machine + `Ruleset` trait, operating on notation `Card`s) → `spades-core` (the `Spades` ruleset + a delegating facade that preserves the published `spades` API). The engine treats cards as opaque tokens; all trump/follow-suit/scoring logic lives in the ruleset, dispatched through `Box<dyn Ruleset>` and serialized via `typetag`.

**Tech Stack:** Rust 2024, serde, `typetag` 0.2, `trick-notation` (workspace), `rand` 0.10, `thiserror` 2, `oasgen` 0.25 (openapi feature).

## Global Constraints

- Workspace version is `3.0.0` (`workspace.package.version`); `trick-engine` joins at the same version. Path deps pin `version = "3.0.0"`.
- `trick-engine` lib name is `trick_engine`; `spades-core` lib name stays `spades`; `trick-notation` lib name is `trick_notation`.
- The published `spades` public API must keep compiling for `spades-server` and `web/` unchanged: `Game`, `GameTransition`, `State` (with a `Betting` alias), `TransitionSuccess`, `TransitionError`, `GetError`, and every existing `Game` accessor.
- `State::Betting` is the **facade alias**; the engine uses `State::Bidding` internally.
- Persisted in-flight games are NOT migrated: the serde shape change is accepted as a one-time reset (see spec risk #2). Completed games persist as transcripts (unaffected).
- Gate for every task: `cargo test --workspace` green. Final gate: `make check`.
- `cargo`/`clippy` require `export PATH="$HOME/.cargo/bin:$PATH"` in the Bash tool.
- Styling/web untouched by this plan; no `web/` source edits except the OpenAPI codegen regen in the final task.

---

## File Structure

**New crate `crates/trick-engine/`:**
- `Cargo.toml` — manifest; deps on `trick-notation`, `serde`, `serde_json`, `typetag`, `rand`, `uuid`, `thiserror`; optional `oasgen` behind `openapi`.
- `src/lib.rs` — crate root; re-exports; `Action`, `StepError`, `StepOutcome`.
- `src/types.rs` — `Seat`, `TeamId`, `BidSpec`, `PlayContext`, `RoundOutcome`, `State`, `Player`.
- `src/ruleset.rs` — the `#[typetag::serde]` `Ruleset` trait.
- `src/game.rs` — the generic `Game` state machine.
- `src/testkit.rs` — `#[cfg(test)]` toy `HighCard` ruleset used as the engine's independent test oracle.

**Modified `crates/spades-core/`:**
- `Cargo.toml` — add `trick-engine` path dep.
- `src/rules.rs` (Create) — `Spades` struct implementing `Ruleset`; absorbs `scoring.rs`.
- `src/scoring.rs` (Modify) — becomes a submodule of the spades ruleset (moved, not rewritten).
- `src/cards.rs` (Modify) — keep spades `Card`/`Suit`/`Rank` + `get_trick_winner`; add conversions to/from `trick_notation::Card`.
- `src/lib.rs` (Modify) — `Game` becomes a newtype over `trick_engine::Game`; accessors delegate.
- `src/game_state.rs` (Modify) — re-export engine `State` with `Betting` alias.
- `src/transcript/adapter.rs` (Modify) — read engine state directly; drop redundant card mapping.

---

## Task 1: Scaffold the `trick-engine` crate

**Files:**
- Create: `crates/trick-engine/Cargo.toml`
- Create: `crates/trick-engine/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces: an empty compiling `trick_engine` crate registered in the workspace.

- [ ] **Step 1: Add the crate to the workspace members**

Modify root `Cargo.toml`:

```toml
[workspace]
members = ["crates/spades-core", "crates/spades-server", "crates/trick-notation", "crates/trick-engine"]
resolver = "3"
```

- [ ] **Step 2: Write the crate manifest**

Create `crates/trick-engine/Cargo.toml`:

```toml
[package]
name = "trick-engine"
version.workspace = true
edition.workspace = true
authors.workspace = true
repository.workspace = true
license.workspace = true
description = "Game-agnostic trick-taking card-game state machine driven by a pluggable Ruleset."
categories = ["games", "game-engines"]
keywords = ["cards", "trick-taking", "engine", "state", "machine"]

[lib]
name = "trick_engine"
path = "src/lib.rs"

[features]
default = []
openapi = ["dep:oasgen", "trick-notation/openapi"]

[dependencies]
rand = "0.10"
uuid.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror = "2"
typetag = "0.2"
trick-notation = { path = "../trick-notation", version = "3.0.0" }
oasgen = { version = "0.25", features = ["uuid"], optional = true }
```

- [ ] **Step 3: Write a placeholder lib root**

Create `crates/trick-engine/src/lib.rs`:

```rust
//! Game-agnostic trick-taking card-game engine. A generic [`Game`] state machine
//! drives deal/bid/trick/score rounds, deferring every rule-specific decision to
//! a [`Ruleset`] trait object. Card identity is reused from `trick_notation`.

mod types;
pub use types::*;
```

Create `crates/trick-engine/src/types.rs` with a single line so the module resolves:

```rust
//! Engine value types. Populated in Task 2.
```

- [ ] **Step 4: Verify the workspace builds**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo build -p trick-engine`
Expected: compiles with no errors (unused-module warnings acceptable).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/trick-engine/Cargo.toml crates/trick-engine/src/lib.rs crates/trick-engine/src/types.rs
git commit -m "feat(trick-engine): scaffold game-agnostic engine crate"
```

---

## Task 2: Engine value types

**Files:**
- Modify: `crates/trick-engine/src/types.rs`

**Interfaces:**
- Produces: `Seat` (= `usize`), `TeamId(usize)`, `BidSpec { min, max }`, `PlayContext<'a> { hand, table, leader, round }`, `RoundOutcome { tricks_won, bids }`, `State` (`NotStarted`/`Bidding(usize)`/`Trick(usize)`/`Completed`/`Aborted`), `Player { id, hand, name }`. Re-exports `trick_notation::{Card, Deck}` from the crate root.

- [ ] **Step 1: Write the failing test**

Append to `crates/trick-engine/src/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_serde_round_trips() {
        for s in [
            State::NotStarted,
            State::Bidding(2),
            State::Trick(0),
            State::Completed,
            State::Aborted,
        ] {
            let j = serde_json::to_string(&s).unwrap();
            let back: State = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn player_new_starts_empty() {
        let p = Player::new(uuid::Uuid::from_u128(7));
        assert!(p.hand.is_empty());
        assert_eq!(p.name, None);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine state_serde_round_trips`
Expected: FAIL to compile — `State`/`Player` not defined.

- [ ] **Step 3: Write the types**

Replace the body of `crates/trick-engine/src/types.rs` (above the `tests` module) with:

```rust
//! Engine value types: seats, teams, bid descriptors, the per-play context the
//! ruleset reads, the per-round outcome it scores, the phase enum, and players.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use trick_notation::{Card, Deck};

/// A seat index in `0..seat_count`.
pub type Seat = usize;

/// A scoring group. Games with partnerships map several seats to one `TeamId`;
/// games without partners map every seat to its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct TeamId(pub usize);

/// Describes a game's bidding phase for generic readers (inclusive bounds).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BidSpec {
    pub min: i32,
    pub max: i32,
}

/// Everything a ruleset needs to decide a legal play: the actor's `hand`, the
/// cards on the `table` this trick (index = seat, `None` = not yet played), the
/// `leader` seat, and the 0-based `round`.
pub struct PlayContext<'a> {
    pub hand: &'a [Card],
    pub table: &'a [Option<Card>],
    pub leader: Seat,
    pub round: usize,
}

/// The result of a completed round, handed to `Ruleset::score_round`.
/// `tricks_won[seat]` and `bids[seat]` are seat-indexed; `bids` is all-zero when
/// the game has no bidding phase.
pub struct RoundOutcome {
    pub tricks_won: Vec<i32>,
    pub bids: Vec<i32>,
}

/// Current engine phase. The inner `usize` of `Bidding`/`Trick` is the count of
/// actors who have acted in the current rotation (matches the legacy spades
/// `State` shape so the facade can alias it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum State {
    NotStarted,
    Bidding(usize),
    Trick(usize),
    Completed,
    Aborted,
}

/// A seated player: stable `id`, current `hand`, optional display `name`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: Uuid,
    pub hand: Vec<Card>,
    #[serde(default)]
    pub name: Option<String>,
}

impl Player {
    pub fn new(id: Uuid) -> Player {
        Player {
            id,
            hand: vec![],
            name: None,
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-engine/src/types.rs
git commit -m "feat(trick-engine): engine value types and State enum"
```

---

## Task 3: The `Ruleset` trait

**Files:**
- Create: `crates/trick-engine/src/ruleset.rs`
- Create: `crates/trick-engine/src/testkit.rs`
- Modify: `crates/trick-engine/src/lib.rs`

**Interfaces:**
- Consumes: `types::{Seat, TeamId, BidSpec, PlayContext, RoundOutcome, Card}`.
- Produces: `pub trait Ruleset` (typetag-serialized, object-safe) with methods `seat_count`, `team_of`, `build_deck`, `hand_size`, `first_leader`, `bid_phase`, `bid_is_legal`, `legal_plays`, `trick_winner`, `score_round`, `is_over`, `scores`. Also `testkit::HighCard`, a 4-seat no-bid ruleset where the highest rank of the led suit wins, used by later engine tests.

- [ ] **Step 1: Write the failing test**

Create `crates/trick-engine/src/testkit.rs`:

```rust
//! A minimal `Ruleset` used only to exercise the generic engine independently of
//! any real game. `HighCard`: 4 seats, every seat its own team, french-52 deck,
//! 13-card hands, no bidding, must-follow-led-suit, highest rank of the led suit
//! wins, fixed 1-round game.

use serde::{Deserialize, Serialize};
use trick_notation::{Card, Deck};

use crate::ruleset::Ruleset;
use crate::types::{BidSpec, PlayContext, RoundOutcome, Seat, TeamId};

#[derive(Default, Serialize, Deserialize)]
pub struct HighCard {
    #[serde(default)]
    pub rounds_played: usize,
}

const RANK_ORDER: [&str; 13] = [
    "2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A",
];

fn rank_value(rank: &str) -> usize {
    RANK_ORDER.iter().position(|r| *r == rank).unwrap_or(0)
}

fn suit_of(card: &Card) -> Option<&str> {
    match card {
        Card::Suited { suit, .. } => Some(suit),
        Card::Special { .. } => None,
    }
}

#[typetag::serde]
impl Ruleset for HighCard {
    fn seat_count(&self) -> usize {
        4
    }
    fn team_of(&self, seat: Seat) -> TeamId {
        TeamId(seat)
    }
    fn build_deck(&self) -> Vec<Card> {
        Deck::french52().cards()
    }
    fn hand_size(&self, _round: usize) -> usize {
        13
    }
    fn first_leader(&self, _round: usize) -> Seat {
        0
    }
    fn bid_phase(&self) -> Option<BidSpec> {
        None
    }
    fn bid_is_legal(&self, _seat: Seat, _bid: i32) -> bool {
        false
    }
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<Card> {
        let led = ctx.table[ctx.leader].as_ref().and_then(suit_of);
        match led {
            Some(led_suit) => {
                let following: Vec<Card> = ctx
                    .hand
                    .iter()
                    .filter(|c| suit_of(c) == Some(led_suit))
                    .cloned()
                    .collect();
                if following.is_empty() {
                    ctx.hand.to_vec()
                } else {
                    following
                }
            }
            None => ctx.hand.to_vec(),
        }
    }
    fn trick_winner(&self, leader: Seat, played: &[Card]) -> Seat {
        let led_suit = suit_of(&played[leader]);
        let mut best = leader;
        for (i, card) in played.iter().enumerate() {
            if suit_of(card) == led_suit
                && rank_value(rank_of(card)) > rank_value(rank_of(&played[best]))
            {
                best = i;
            }
        }
        best
    }
    fn score_round(&mut self, _outcome: &RoundOutcome) {
        self.rounds_played += 1;
    }
    fn is_over(&self) -> bool {
        self.rounds_played >= 1
    }
    fn scores(&self) -> Vec<i32> {
        vec![0; 4]
    }
}

fn rank_of(card: &Card) -> &str {
    match card {
        Card::Suited { rank, .. } => rank,
        Card::Special { .. } => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highcard_serializes_with_tag() {
        let r: Box<dyn Ruleset> = Box::new(HighCard::default());
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("HighCard"), "tagged json: {j}");
        let back: Box<dyn Ruleset> = serde_json::from_str(&j).unwrap();
        assert_eq!(back.seat_count(), 4);
    }
}
```

> Note: `Deck::french52().cards()` is used here. If `trick_notation::Deck` does not expose a `cards()` accessor yet, add a thin `pub fn cards(&self) -> Vec<Card>` to `crates/trick-notation/src/deck.rs` returning the deck's cards in canonical order, with its own unit test, as Step 3a below. Inspect `deck.rs` first.

- [ ] **Step 2: Run the test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine highcard_serializes_with_tag`
Expected: FAIL to compile — `Ruleset` not defined.

- [ ] **Step 3: Write the trait**

Create `crates/trick-engine/src/ruleset.rs`:

```rust
//! The pluggable rule surface. The engine owns the round/trick/rotation
//! skeleton; everything game-specific is one of these twelve questions. The
//! trait is object-safe and `#[typetag::serde]`-tagged so `Box<dyn Ruleset>`
//! serializes (and rebuilds) with a `"type"` discriminant.

use crate::types::{BidSpec, Card, PlayContext, RoundOutcome, Seat, TeamId};

#[typetag::serde(tag = "type")]
pub trait Ruleset {
    /// Number of seats at the table.
    fn seat_count(&self) -> usize;
    /// The scoring group a seat belongs to.
    fn team_of(&self, seat: Seat) -> TeamId;

    /// The full deck for a deal, in any order (the engine shuffles).
    fn build_deck(&self) -> Vec<Card>;
    /// Cards dealt to each seat for `round`.
    fn hand_size(&self, round: usize) -> usize;
    /// The seat that leads the first trick of `round`.
    fn first_leader(&self, round: usize) -> Seat;

    /// `Some(spec)` if `round` opens with a bidding phase, else `None`.
    fn bid_phase(&self) -> Option<BidSpec>;
    /// Whether `bid` from `seat` is legal.
    fn bid_is_legal(&self, seat: Seat, bid: i32) -> bool;

    /// The subset of the actor's hand that is legal to play now.
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<Card>;
    /// The winning seat of a completed trick, given the `leader` and the cards
    /// each seat played (index = seat).
    fn trick_winner(&self, leader: Seat, played: &[Card]) -> Seat;

    /// Fold a completed round's outcome into the ruleset's own score state.
    fn score_round(&mut self, outcome: &RoundOutcome);
    /// Whether the game has reached a terminal score.
    fn is_over(&self) -> bool;
    /// Current cumulative score per `TeamId` index, for generic readers.
    fn scores(&self) -> Vec<i32>;
}
```

- [ ] **Step 3a (conditional): add `Deck::cards()` if missing**

If Step 4 fails because `Deck` has no `cards()`, add to `crates/trick-notation/src/deck.rs`:

```rust
impl Deck {
    /// The deck's cards in canonical order.
    pub fn cards(&self) -> Vec<crate::card::Card> {
        // Mirror the existing internal representation; see french52().
        self.iter_cards().collect()
    }
}
```

Adjust `iter_cards()` to the actual internal accessor after reading `deck.rs`. Add a unit test asserting `Deck::french52().cards().len() == 52`.

- [ ] **Step 4: Wire modules and run the test**

Modify `crates/trick-engine/src/lib.rs`:

```rust
//! Game-agnostic trick-taking card-game engine. A generic [`Game`] state machine
//! drives deal/bid/trick/score rounds, deferring every rule-specific decision to
//! a [`Ruleset`] trait object. Card identity is reused from `trick_notation`.

mod ruleset;
mod types;

pub use ruleset::Ruleset;
pub use types::*;

#[cfg(test)]
mod testkit;
```

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-engine/src crates/trick-notation/src/deck.rs
git commit -m "feat(trick-engine): Ruleset trait + HighCard test ruleset"
```

---

## Task 4: Generic `Game` — construction and `Start` (deal)

**Files:**
- Create: `crates/trick-engine/src/game.rs`
- Modify: `crates/trick-engine/src/lib.rs`

**Interfaces:**
- Consumes: `Ruleset`, `types::*`.
- Produces: `Action` (`Start`/`Bid(i32)`/`Play(Card)`/`Abort`), `StepError` (`thiserror`: `NotStarted`/`AlreadyStarted`/`WrongPhase`/`Completed`/`IllegalBid`/`CardNotInHand`/`IllegalPlay`), `StepOutcome` (`Started`/`Bid`/`BidComplete`/`PlayCard`/`TrickComplete`/`RoundComplete`/`GameOver`/`Aborted`), and `Game` with `new(id, player_ids: Vec<Uuid>, rules: Box<dyn Ruleset>) -> Game`, `state()`, `current_seat()`, `step(Action)`. After `Start`, the deck is shuffled and dealt and state becomes `Bidding(0)` (or `Trick(0)` when `bid_phase()` is `None`).

- [ ] **Step 1: Write the failing test**

Create `crates/trick-engine/src/game.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::HighCard;
    use uuid::Uuid;

    fn ids(n: usize) -> Vec<Uuid> {
        (0..n).map(|i| Uuid::from_u128(i as u128 + 1)).collect()
    }

    #[test]
    fn start_deals_and_enters_trick_when_no_bidding() {
        let mut g = Game::new(Uuid::from_u128(99), ids(4), Box::new(HighCard::default()));
        assert_eq!(*g.state(), State::NotStarted);
        let out = g.step(Action::Start).unwrap();
        assert_eq!(out, StepOutcome::Started);
        // HighCard has no bid phase, so we go straight to Trick(0).
        assert_eq!(*g.state(), State::Trick(0));
        // Each of the 4 seats holds 13 cards.
        for seat in 0..4 {
            assert_eq!(g.hand(seat).len(), 13);
        }
        assert_eq!(g.current_seat(), 0); // first_leader
    }

    #[test]
    fn double_start_is_rejected() {
        let mut g = Game::new(Uuid::from_u128(1), ids(4), Box::new(HighCard::default()));
        g.step(Action::Start).unwrap();
        assert_eq!(g.step(Action::Start), Err(StepError::AlreadyStarted));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine start_deals`
Expected: FAIL to compile — `Game` not defined.

- [ ] **Step 3: Write `Game` construction + `Start`**

Prepend to `crates/trick-engine/src/game.rs` (above the test module):

```rust
//! The generic state machine. Holds a boxed `Ruleset` and drives rounds of
//! deal → (bid) → trick* → score. Cards are opaque; legality and winners come
//! from the ruleset.

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ruleset::Ruleset;
use crate::types::{Card, Player, PlayContext, RoundOutcome, Seat, State};

/// A caller-supplied transition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Start,
    Bid(i32),
    Play(Card),
    Abort,
}

/// A successful transition's classification.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepOutcome {
    Started,
    Bid,
    BidComplete,
    PlayCard,
    TrickComplete,
    RoundComplete,
    GameOver,
    Aborted,
}

/// A rejected transition. The facade maps `IllegalPlay`/`IllegalBid` onto its
/// own game-specific error variants.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum StepError {
    #[error("game not started")]
    NotStarted,
    #[error("game already started")]
    AlreadyStarted,
    #[error("action not valid in this phase")]
    WrongPhase,
    #[error("game already completed")]
    Completed,
    #[error("bid rejected by ruleset")]
    IllegalBid,
    #[error("card not in hand")]
    CardNotInHand,
    #[error("card is not a legal play")]
    IllegalPlay,
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    id: Uuid,
    rules: Box<dyn Ruleset>,
    state: State,
    players: Vec<Player>,
    current_seat: Seat,
    trick_leader: Seat,
    deck: Vec<Card>,
    trick: Vec<Option<Card>>,
    history: Vec<Vec<Option<Card>>>,
    bids: Vec<i32>,
    round: usize,
}

impl Game {
    pub fn new(id: Uuid, player_ids: Vec<Uuid>, rules: Box<dyn Ruleset>) -> Game {
        let n = rules.seat_count();
        assert_eq!(player_ids.len(), n, "player count must equal seat_count");
        Game {
            id,
            rules,
            state: State::NotStarted,
            players: player_ids.into_iter().map(Player::new).collect(),
            current_seat: 0,
            trick_leader: 0,
            deck: vec![],
            trick: vec![None; n],
            history: vec![],
            bids: vec![0; n],
            round: 0,
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn current_seat(&self) -> Seat {
        self.current_seat
    }
    pub fn seat_count(&self) -> usize {
        self.rules.seat_count()
    }
    pub fn rules(&self) -> &dyn Ruleset {
        self.rules.as_ref()
    }
    pub fn hand(&self, seat: Seat) -> &[Card] {
        &self.players[seat].hand
    }
    pub fn player_id(&self, seat: Seat) -> Uuid {
        self.players[seat].id
    }
    pub fn current_trick(&self) -> &[Option<Card>] {
        &self.trick
    }
    pub fn trick_leader(&self) -> Seat {
        self.trick_leader
    }
    pub fn round(&self) -> usize {
        self.round
    }
    pub fn bids(&self) -> &[i32] {
        &self.bids
    }
    pub fn history(&self) -> &[Vec<Option<Card>>] {
        &self.history
    }
    pub fn player_mut(&mut self, seat: Seat) -> &mut Player {
        &mut self.players[seat]
    }

    fn deal(&mut self) {
        let n = self.rules.seat_count();
        let mut deck = self.rules.build_deck();
        deck.shuffle(&mut rand::rng());
        let hand_size = self.rules.hand_size(self.round);
        for seat in 0..n {
            let start = seat * hand_size;
            self.players[seat].hand = deck[start..start + hand_size].to_vec();
        }
        self.deck = deck;
        self.trick = vec![None; n];
        self.trick_leader = self.rules.first_leader(self.round);
        self.current_seat = self.trick_leader;
    }

    /// Drive the machine. Spades-specific meaning is entirely in `self.rules`.
    pub fn step(&mut self, action: Action) -> Result<StepOutcome, StepError> {
        match action {
            Action::Start => {
                if self.state != State::NotStarted {
                    return Err(StepError::AlreadyStarted);
                }
                self.deal();
                self.state = if self.rules.bid_phase().is_some() {
                    State::Bidding(0)
                } else {
                    State::Trick(0)
                };
                Ok(StepOutcome::Started)
            }
            Action::Abort => match self.state {
                State::Completed | State::Aborted => Err(StepError::Completed),
                _ => {
                    self.state = State::Aborted;
                    Ok(StepOutcome::Aborted)
                }
            },
            // Bid/Play implemented in Tasks 5 and 6.
            Action::Bid(_) | Action::Play(_) => Err(StepError::WrongPhase),
        }
    }
}
```

Add `mod game; pub use game::{Action, Game, StepError, StepOutcome};` to `lib.rs` (place `pub use types::*;` after so `Game` from `game` wins; types has no `Game`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-engine/src/game.rs crates/trick-engine/src/lib.rs
git commit -m "feat(trick-engine): Game construction, deal, and Start transition"
```

---

## Task 5: Bidding phase

**Files:**
- Modify: `crates/trick-engine/src/game.rs`
- Modify: `crates/trick-engine/src/testkit.rs` (add a bidding test ruleset)

**Interfaces:**
- Consumes: `Game`, `Action::Bid`, `Ruleset::{bid_phase, bid_is_legal}`.
- Produces: `Game::step(Action::Bid(i32))` handling — validates via `bid_is_legal`, records into `self.bids[seat]`, rotates; the `seat_count`-th bid transitions to `State::Trick(0)` with `current_seat = trick_leader` and returns `StepOutcome::BidComplete`. Adds `testkit::SimpleBid` (4 seats, bids `0..=13`, otherwise like `HighCard`).

- [ ] **Step 1: Write the failing test**

Add `SimpleBid` to `crates/trick-engine/src/testkit.rs`:

```rust
/// Like `HighCard` but with a 0..=13 bidding phase, for engine bid tests.
#[derive(Default, Serialize, Deserialize)]
pub struct SimpleBid {
    #[serde(default)]
    inner: HighCard,
}

#[typetag::serde]
impl Ruleset for SimpleBid {
    fn seat_count(&self) -> usize {
        4
    }
    fn team_of(&self, seat: Seat) -> TeamId {
        TeamId(seat)
    }
    fn build_deck(&self) -> Vec<Card> {
        self.inner.build_deck()
    }
    fn hand_size(&self, round: usize) -> usize {
        self.inner.hand_size(round)
    }
    fn first_leader(&self, round: usize) -> Seat {
        self.inner.first_leader(round)
    }
    fn bid_phase(&self) -> Option<BidSpec> {
        Some(BidSpec { min: 0, max: 13 })
    }
    fn bid_is_legal(&self, _seat: Seat, bid: i32) -> bool {
        (0..=13).contains(&bid)
    }
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<Card> {
        self.inner.legal_plays(ctx)
    }
    fn trick_winner(&self, leader: Seat, played: &[Card]) -> Seat {
        self.inner.trick_winner(leader, played)
    }
    fn score_round(&mut self, outcome: &RoundOutcome) {
        self.inner.score_round(outcome)
    }
    fn is_over(&self) -> bool {
        self.inner.is_over()
    }
    fn scores(&self) -> Vec<i32> {
        self.inner.scores()
    }
}
```

Add to `crates/trick-engine/src/game.rs` test module:

```rust
    use crate::testkit::SimpleBid;

    #[test]
    fn four_bids_complete_to_trick() {
        let mut g = Game::new(Uuid::from_u128(2), ids(4), Box::new(SimpleBid::default()));
        g.step(Action::Start).unwrap();
        assert_eq!(*g.state(), State::Bidding(0));
        for i in 0..3 {
            assert_eq!(g.step(Action::Bid(3)).unwrap(), StepOutcome::Bid);
            assert_eq!(*g.state(), State::Bidding(i + 1));
        }
        assert_eq!(g.step(Action::Bid(3)).unwrap(), StepOutcome::BidComplete);
        assert_eq!(*g.state(), State::Trick(0));
        assert_eq!(g.bids(), &[3, 3, 3, 3]);
    }

    #[test]
    fn illegal_bid_rejected() {
        let mut g = Game::new(Uuid::from_u128(3), ids(4), Box::new(SimpleBid::default()));
        g.step(Action::Start).unwrap();
        assert_eq!(g.step(Action::Bid(99)), Err(StepError::IllegalBid));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine four_bids_complete`
Expected: FAIL — `Bid` returns `WrongPhase`.

- [ ] **Step 3: Implement the bid branch**

In `game.rs` `step`, replace the `Action::Bid(_) | Action::Play(_) => Err(StepError::WrongPhase),` arm with:

```rust
            Action::Bid(bid) => match self.state {
                State::Bidding(rot) => {
                    if !self.rules.bid_is_legal(self.current_seat, bid) {
                        return Err(StepError::IllegalBid);
                    }
                    self.bids[self.current_seat] = bid;
                    let n = self.rules.seat_count();
                    if rot + 1 == n {
                        self.state = State::Trick(0);
                        self.current_seat = self.trick_leader;
                        Ok(StepOutcome::BidComplete)
                    } else {
                        self.current_seat = (self.current_seat + 1) % n;
                        self.state = State::Bidding(rot + 1);
                        Ok(StepOutcome::Bid)
                    }
                }
                State::NotStarted => Err(StepError::NotStarted),
                State::Completed | State::Aborted => Err(StepError::Completed),
                State::Trick(_) => Err(StepError::WrongPhase),
            },
            Action::Play(_) => Err(StepError::WrongPhase),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-engine/src/game.rs crates/trick-engine/src/testkit.rs
git commit -m "feat(trick-engine): bidding phase via Ruleset::bid_is_legal"
```

---

## Task 6: Trick play — legality, rotation, trick winner

**Files:**
- Modify: `crates/trick-engine/src/game.rs`

**Interfaces:**
- Consumes: `Game`, `Action::Play`, `Ruleset::{legal_plays, trick_winner}`, `PlayContext`.
- Produces: `Game::step(Action::Play(Card))` — rejects `CardNotInHand` / `IllegalPlay`, places the card, rotates; on the `seat_count`-th card it calls `trick_winner`, pushes the trick to `history`, sets the winner as the next `trick_leader`/`current_seat`, clears `trick`, and returns `StepOutcome::TrickComplete` (round-boundary handling added in Task 7 — for now, start a fresh trick). Adds `Game::legal_plays(&self) -> Vec<Card>` reading the current `PlayContext`.

- [ ] **Step 1: Write the failing test**

Add to `game.rs` test module:

```rust
    fn play_one_full_trick(g: &mut Game) -> StepOutcome {
        let mut last = StepOutcome::PlayCard;
        for _ in 0..4 {
            let legal = g.legal_plays();
            last = g.step(Action::Play(legal[0].clone())).unwrap();
        }
        last
    }

    #[test]
    fn full_trick_records_history_and_sets_winner_leader() {
        let mut g = Game::new(Uuid::from_u128(4), ids(4), Box::new(HighCard::default()));
        g.step(Action::Start).unwrap();
        let out = play_one_full_trick(&mut g);
        assert_eq!(out, StepOutcome::TrickComplete);
        assert_eq!(g.history().len(), 1);
        assert!(g.history()[0].iter().all(|c| c.is_some()));
        // Winner now leads and the table is cleared.
        assert_eq!(g.current_seat(), g.trick_leader());
        assert!(g.current_trick().iter().all(|c| c.is_none()));
    }

    #[test]
    fn playing_card_not_in_hand_is_rejected() {
        let mut g = Game::new(Uuid::from_u128(5), ids(4), Box::new(HighCard::default()));
        g.step(Action::Start).unwrap();
        // Build a card guaranteed not to match a real french-52 token shape after
        // the seat's hand is dealt: a Special card is never dealt.
        let bogus = Card::Special { name: "Joker".into() };
        assert_eq!(g.step(Action::Play(bogus)), Err(StepError::CardNotInHand));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine full_trick_records`
Expected: FAIL — `legal_plays` method / `Play` handling missing.

- [ ] **Step 3: Implement play + legal_plays**

Add to `impl Game` (above `step`):

```rust
    /// The legal plays for the seat on turn. Empty unless in a trick phase.
    pub fn legal_plays(&self) -> Vec<Card> {
        if let State::Trick(_) = self.state {
            let ctx = PlayContext {
                hand: &self.players[self.current_seat].hand,
                table: &self.trick,
                leader: self.trick_leader,
                round: self.round,
            };
            self.rules.legal_plays(&ctx)
        } else {
            vec![]
        }
    }
```

Replace `Action::Play(_) => Err(StepError::WrongPhase),` with:

```rust
            Action::Play(card) => match self.state {
                State::Trick(rot) => {
                    let hand = &self.players[self.current_seat].hand;
                    if !hand.contains(&card) {
                        return Err(StepError::CardNotInHand);
                    }
                    let legal = {
                        let ctx = PlayContext {
                            hand,
                            table: &self.trick,
                            leader: self.trick_leader,
                            round: self.round,
                        };
                        self.rules.legal_plays(&ctx)
                    };
                    if !legal.contains(&card) {
                        return Err(StepError::IllegalPlay);
                    }
                    // Remove from hand, place on table.
                    let h = &mut self.players[self.current_seat].hand;
                    let idx = h.iter().position(|c| c == &card).unwrap();
                    h.remove(idx);
                    self.trick[self.current_seat] = Some(card);

                    let n = self.rules.seat_count();
                    if rot + 1 == n {
                        let played: Vec<Card> =
                            self.trick.iter().map(|c| c.clone().unwrap()).collect();
                        let winner = self.rules.trick_winner(self.trick_leader, &played);
                        self.history.push(self.trick.clone());
                        // Round-boundary handling lands in Task 7; for now always
                        // start a fresh trick led by the winner.
                        self.trick = vec![None; n];
                        self.trick_leader = winner;
                        self.current_seat = winner;
                        self.state = State::Trick(0);
                        Ok(StepOutcome::TrickComplete)
                    } else {
                        self.current_seat = (self.current_seat + 1) % n;
                        self.state = State::Trick(rot + 1);
                        Ok(StepOutcome::PlayCard)
                    }
                }
                State::NotStarted => Err(StepError::NotStarted),
                State::Completed | State::Aborted => Err(StepError::Completed),
                State::Bidding(_) => Err(StepError::WrongPhase),
            },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine`
Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-engine/src/game.rs
git commit -m "feat(trick-engine): trick play, legality delegation, winner-leads rotation"
```

---

## Task 7: Round completion, scoring, termination

**Files:**
- Modify: `crates/trick-engine/src/game.rs`

**Interfaces:**
- Consumes: `Ruleset::{score_round, is_over, bid_phase}`, `RoundOutcome`.
- Produces: when the last trick of a round completes (`hand_size` tricks played this round), `step` builds a `RoundOutcome { tricks_won, bids }`, calls `score_round`, then `is_over`: if over → `State::Completed`, `StepOutcome::GameOver`; else re-deal, reset `bids`, enter `Bidding(0)` (or `Trick(0)`), `StepOutcome::RoundComplete`. Adds a per-round trick counter and per-seat `tricks_won` tally.

- [ ] **Step 1: Write the failing test**

Add to `game.rs` test module:

```rust
    #[test]
    fn one_round_completes_and_game_over_for_highcard() {
        let mut g = Game::new(Uuid::from_u128(6), ids(4), Box::new(HighCard::default()));
        g.step(Action::Start).unwrap();
        // 13 tricks; HighCard is_over() after 1 round.
        let mut last = StepOutcome::Started;
        for _ in 0..13 {
            last = play_one_full_trick(&mut g);
        }
        assert_eq!(last, StepOutcome::GameOver);
        assert_eq!(*g.state(), State::Completed);
        assert_eq!(g.history().len(), 13);
    }
```

To make this deterministic, add a `tricks_this_round` counter; the round ends when it reaches `hand_size(round)`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine one_round_completes`
Expected: FAIL — game never reaches `Completed` (Task 6 always starts a fresh trick).

- [ ] **Step 3: Implement round boundary**

Add fields to `Game` struct: `tricks_this_round: usize` and `tricks_won: Vec<i32>`. Initialize both in `new` (`0` and `vec![0; n]`) and reset `tricks_this_round = 0`, `tricks_won = vec![0; n]` at the end of `deal`.

In the trick-complete branch of `Action::Play`, replace the block from `let winner = …` through `Ok(StepOutcome::TrickComplete)` with:

```rust
                        let winner = self.rules.trick_winner(self.trick_leader, &played);
                        self.history.push(self.trick.clone());
                        self.tricks_won[winner] += 1;
                        self.tricks_this_round += 1;
                        self.trick = vec![None; n];
                        self.trick_leader = winner;

                        if self.tricks_this_round == self.rules.hand_size(self.round) {
                            let outcome = RoundOutcome {
                                tricks_won: self.tricks_won.clone(),
                                bids: self.bids.clone(),
                            };
                            self.rules.score_round(&outcome);
                            if self.rules.is_over() {
                                self.state = State::Completed;
                                return Ok(StepOutcome::GameOver);
                            }
                            self.round += 1;
                            self.deal();
                            self.bids = vec![0; n];
                            self.state = if self.rules.bid_phase().is_some() {
                                State::Bidding(0)
                            } else {
                                State::Trick(0)
                            };
                            Ok(StepOutcome::RoundComplete)
                        } else {
                            self.current_seat = winner;
                            self.state = State::Trick(0);
                            Ok(StepOutcome::TrickComplete)
                        }
```

> Note: `deal()` resets `current_seat` to the new `trick_leader`, so the re-deal branch needs no explicit `current_seat` assignment.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine`
Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-engine/src/game.rs
git commit -m "feat(trick-engine): round completion, scoring, and termination"
```

---

## Task 8: Whole-`Game` serialization round-trip

**Files:**
- Modify: `crates/trick-engine/src/game.rs`

**Interfaces:**
- Consumes: `Game` (derives `Serialize`/`Deserialize`), `typetag` on `Ruleset`.
- Produces: a test proving a mid-game `Game` survives a JSON round-trip including the boxed ruleset and its score state.

- [ ] **Step 1: Write the failing test**

Add to `game.rs` test module:

```rust
    #[test]
    fn game_json_round_trips_mid_play() {
        let mut g = Game::new(Uuid::from_u128(8), ids(4), Box::new(SimpleBid::default()));
        g.step(Action::Start).unwrap();
        for _ in 0..4 {
            g.step(Action::Bid(2)).unwrap();
        }
        play_one_full_trick(&mut g);
        let json = serde_json::to_string(&g).unwrap();
        let back: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state(), g.state());
        assert_eq!(back.history().len(), g.history().len());
        assert_eq!(back.bids(), g.bids());
        assert_eq!(back.current_seat(), g.current_seat());
    }
```

- [ ] **Step 2: Run the test to verify it passes (or fix derives)**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p trick-engine game_json_round_trips`
Expected: PASS. If it fails to compile, ensure `Game` derives `Serialize, Deserialize` and all fields are serializable (they are: `Box<dyn Ruleset>` via typetag, `Card` via trick-notation).

- [ ] **Step 3: Commit**

```bash
git add crates/trick-engine/src/game.rs
git commit -m "test(trick-engine): whole-Game JSON round-trip through typetag ruleset"
```

---

## Task 9: Implement `Spades: Ruleset` in spades-core

**Files:**
- Modify: `crates/spades-core/Cargo.toml`
- Modify: `crates/spades-core/src/cards.rs`
- Create: `crates/spades-core/src/rules.rs`
- Modify: `crates/spades-core/src/scoring.rs`
- Modify: `crates/spades-core/src/lib.rs` (declare `mod rules;`)

**Interfaces:**
- Consumes: `trick_engine::{Ruleset, BidSpec, PlayContext, RoundOutcome, Seat, TeamId, Card as TnCard}`; spades `Card`/`Suit`/`Rank`, `scoring::Scoring`.
- Produces: `pub(crate) struct Spades { scoring: Scoring }` implementing `Ruleset`; `cards.rs` gains `to_tn(Card) -> TnCard` and `from_tn(&TnCard) -> Option<Card>` plus a spades-suit/rank symbol map. Spades trump + spades-broken live in `legal_plays`/`trick_winner`; `0..=13` in `bid_is_legal`; existing scoring in `score_round`/`is_over`/`scores`.

- [ ] **Step 1: Add the dependency**

In `crates/spades-core/Cargo.toml` `[dependencies]`, add:

```toml
trick-engine = { path = "../trick-engine", version = "3.0.0" }
```

And extend the `openapi` feature: `openapi = ["dep:oasgen", "trick-notation/openapi", "trick-engine/openapi"]`.

- [ ] **Step 2: Write the failing test (spades trick winner via ruleset)**

Create `crates/spades-core/src/rules.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Card, Rank, Suit};
    use trick_engine::Ruleset;

    fn tn(c: Card) -> trick_notation::Card {
        crate::cards::to_tn(c)
    }

    #[test]
    fn spade_trumps_led_suit() {
        let rules = Spades::new(500);
        // Leader (seat 0) leads a high heart; seat 2 plays a low spade and wins.
        let played = vec![
            tn(Card { suit: Suit::Heart, rank: Rank::Ace }),
            tn(Card { suit: Suit::Heart, rank: Rank::Two }),
            tn(Card { suit: Suit::Spade, rank: Rank::Two }),
            tn(Card { suit: Suit::Heart, rank: Rank::King }),
        ];
        assert_eq!(rules.trick_winner(0, &played), 2);
    }

    #[test]
    fn bid_range_enforced() {
        let rules = Spades::new(500);
        assert!(rules.bid_is_legal(0, 0));
        assert!(rules.bid_is_legal(0, 13));
        assert!(!rules.bid_is_legal(0, 14));
        assert!(!rules.bid_is_legal(0, -1));
    }
}
```

- [ ] **Step 3: Add card conversions**

Append to `crates/spades-core/src/cards.rs` (reuse the symbol maps already present in `transcript/adapter.rs` — copy the `rank_sym`/`suit_sym`/`rank_from_sym`/`suit_from_sym` bodies here so the adapter can later delegate):

```rust
/// Spades suit/rank ↔ trick-notation symbol mapping. Spades uses french-52
/// single-character tokens, identical to the notation deck.
impl Suit {
    pub(crate) fn sym(self) -> &'static str {
        match self {
            Suit::Club => "C",
            Suit::Diamond => "D",
            Suit::Heart => "H",
            Suit::Spade => "S",
        }
    }
    pub(crate) fn from_sym(s: &str) -> Option<Suit> {
        Some(match s {
            "C" => Suit::Club,
            "D" => Suit::Diamond,
            "H" => Suit::Heart,
            "S" => Suit::Spade,
            _ => return None,
        })
    }
}

impl Rank {
    pub(crate) fn sym(self) -> &'static str {
        match self {
            Rank::Two => "2", Rank::Three => "3", Rank::Four => "4", Rank::Five => "5",
            Rank::Six => "6", Rank::Seven => "7", Rank::Eight => "8", Rank::Nine => "9",
            Rank::Ten => "T", Rank::Jack => "J", Rank::Queen => "Q", Rank::King => "K",
            Rank::Ace => "A",
        }
    }
    pub(crate) fn from_sym(s: &str) -> Option<Rank> {
        Some(match s {
            "2" => Rank::Two, "3" => Rank::Three, "4" => Rank::Four, "5" => Rank::Five,
            "6" => Rank::Six, "7" => Rank::Seven, "8" => Rank::Eight, "9" => Rank::Nine,
            "T" => Rank::Ten, "J" => Rank::Jack, "Q" => Rank::Queen, "K" => Rank::King,
            "A" => Rank::Ace, _ => return None,
        })
    }
}

/// Convert a spades card to its trick-notation representation.
pub(crate) fn to_tn(c: Card) -> trick_notation::Card {
    trick_notation::Card::Suited {
        suit: c.suit.sym().to_string(),
        rank: c.rank.sym().to_string(),
    }
}

/// Convert a trick-notation card back to a spades card, or `None` for any
/// token that is not a french-52 suited card (e.g. a `Special`).
pub(crate) fn from_tn(c: &trick_notation::Card) -> Option<Card> {
    match c {
        trick_notation::Card::Suited { suit, rank } => Some(Card {
            suit: Suit::from_sym(suit)?,
            rank: Rank::from_sym(rank)?,
        }),
        trick_notation::Card::Special { .. } => None,
    }
}
```

- [ ] **Step 4: Move `scoring.rs` into the ruleset and write `Spades`**

`scoring.rs` keeps its file and contents; just make it a child of `rules` by adding `mod scoring;` inside `rules.rs` and removing `mod scoring;` from `lib.rs` (the scoring tests move with it untouched). Prepend to `crates/spades-core/src/rules.rs`:

```rust
//! Spades as a `trick_engine::Ruleset`. Trump (spades) and the spades-broken
//! lead rule live in `legal_plays`/`trick_winner`; bags/nil/termination live in
//! the moved `scoring` module, owned here as serialized state.

mod scoring;

use serde::{Deserialize, Serialize};
use trick_engine::{BidSpec, PlayContext, RoundOutcome, Ruleset, Seat, TeamId};
use trick_notation::Card as TnCard;

use crate::cards::{from_tn, to_tn, Card, Rank, Suit, get_trick_winner, new_deck};
use scoring::Scoring;

#[derive(Serialize, Deserialize)]
pub(crate) struct Spades {
    scoring: Scoring,
    #[serde(default)]
    spades_broken: bool,
}

impl Spades {
    pub(crate) fn new(max_points: i32) -> Spades {
        Spades { scoring: Scoring::new(max_points), spades_broken: false }
    }
    pub(crate) fn scoring(&self) -> &Scoring {
        &self.scoring
    }
}

#[typetag::serde]
impl Ruleset for Spades {
    fn seat_count(&self) -> usize {
        4
    }
    fn team_of(&self, seat: Seat) -> TeamId {
        TeamId(seat % 2)
    }
    fn build_deck(&self) -> Vec<TnCard> {
        new_deck().into_iter().map(to_tn).collect()
    }
    fn hand_size(&self, _round: usize) -> usize {
        13
    }
    fn first_leader(&self, _round: usize) -> Seat {
        0
    }
    fn bid_phase(&self) -> Option<BidSpec> {
        Some(BidSpec { min: 0, max: 13 })
    }
    fn bid_is_legal(&self, _seat: Seat, bid: i32) -> bool {
        (0..=13).contains(&bid)
    }
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<TnCard> {
        let hand: Vec<Card> = ctx.hand.iter().filter_map(from_tn).collect();
        let leading: Option<Suit> = ctx.table[ctx.leader].as_ref().and_then(from_tn).map(|c| c.suit);
        let legal: Vec<Card> = match leading {
            None => {
                // Leading the trick: can't lead a spade until broken, unless
                // only spades remain.
                if !self.spades_broken && hand.iter().any(|c| c.suit != Suit::Spade) {
                    hand.iter().filter(|c| c.suit != Suit::Spade).copied().collect()
                } else {
                    hand.clone()
                }
            }
            Some(ls) => {
                let following: Vec<Card> = hand.iter().filter(|c| c.suit == ls).copied().collect();
                if following.is_empty() { hand.clone() } else { following }
            }
        };
        legal.into_iter().map(to_tn).collect()
    }
    fn trick_winner(&self, leader: Seat, played: &[TnCard]) -> Seat {
        let cards: [Card; 4] = std::array::from_fn(|i| from_tn(&played[i]).expect("spades card"));
        get_trick_winner(leader, &cards)
    }
    fn score_round(&mut self, outcome: &RoundOutcome) {
        // Drive the existing Scoring per-trick API is not needed here: Scoring
        // already accumulated per-trick during play via `note_trick` (added in
        // Task 10's wiring). At round end we finalize from the tallies.
        self.scoring.finalize_round(&outcome.tricks_won, &outcome.bids);
        self.spades_broken = false;
    }
    fn is_over(&self) -> bool {
        self.scoring.is_over
    }
    fn scores(&self) -> Vec<i32> {
        vec![
            self.scoring.team_a.cumulative_points,
            self.scoring.team_b.cumulative_points,
        ]
    }
}
```

> Important: this introduces two `Scoring` API needs not in the current code:
> `finalize_round(tricks_won: &[i32], bids: &[i32])` and the `spades_broken`
> tracking. The current `Scoring::trick` couples winner-finding, per-trick
> tallying, and round finalization. **Refactor `Scoring`** (Step 5) to split
> these: keep round finalization (the bag/nil/penalty/termination math in
> `calculate_round_totals` + the `trick==12` block) behind `finalize_round`,
> driven by the engine's `tricks_won`/`bids` rather than its own trick loop.

- [ ] **Step 5: Refactor `Scoring` to `finalize_round`**

In `crates/spades-core/src/rules/scoring.rs` (the moved file), add:

```rust
impl Scoring {
    /// Finalize a completed round from the engine's per-seat tallies. Replaces
    /// the old self-counted `trick()` round-end path; the bag/nil/penalty/
    /// termination math is unchanged.
    pub(crate) fn finalize_round(&mut self, tricks_won: &[i32], bids: &[i32]) {
        self.team_a.current_round_tricks_won = tricks_won[0] + tricks_won[2];
        self.team_b.current_round_tricks_won = tricks_won[1] + tricks_won[3];
        let won_a_trick: [bool; 4] = std::array::from_fn(|i| tricks_won[i] > 0);
        self.bets_placed[self.round] = [bids[0], bids[1], bids[2], bids[3]];

        self.team_a.calculate_round_totals(
            PartnerOutcome { bet: bids[0], took_trick: won_a_trick[0] },
            PartnerOutcome { bet: bids[2], took_trick: won_a_trick[2] },
        );
        self.team_b.calculate_round_totals(
            PartnerOutcome { bet: bids[1], took_trick: won_a_trick[1] },
            PartnerOutcome { bet: bids[3], took_trick: won_a_trick[3] },
        );

        // Termination: copy the existing max_points / MIN_POINTS / MAX_ROUNDS
        // block verbatim from the old `trick()` `self.trick == 12` arm.
        let a_reached = self.team_a.cumulative_points >= self.config.max_points;
        let b_reached = self.team_b.cumulative_points >= self.config.max_points;
        if a_reached || b_reached {
            if a_reached && b_reached {
                if self.team_a.cumulative_points != self.team_b.cumulative_points {
                    self.is_over = true;
                }
            } else {
                self.is_over = true;
            }
        }
        if self.team_a.cumulative_points <= MIN_POINTS || self.team_b.cumulative_points <= MIN_POINTS {
            self.is_over = true;
        }
        if self.round + 1 >= MAX_ROUNDS {
            self.is_over = true;
        }

        self.team_a.current_round_tricks_won = 0;
        self.team_b.current_round_tricks_won = 0;
        self.round += 1;
        self.in_betting_stage = true;
        self.bets_placed.push([0; 4]);
    }
}
```

Make `PartnerOutcome`, `MIN_POINTS`, `MAX_ROUNDS` visible to this `impl` (they are in the same file). Keep the existing `trick()`/`bet()`/`add_bet()` methods for now; the existing `scoring.rs` unit tests still exercise `trick()` and must stay green. (A later cleanup task may remove `trick()` once nothing calls it.)

- [ ] **Step 6: Declare the module and run the new tests**

Add `mod rules;` to `crates/spades-core/src/lib.rs`. Remove the now-moved `mod scoring;` line.

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p spades rules::tests`
Expected: PASS (spade_trumps_led_suit, bid_range_enforced) plus the moved scoring tests still pass.

- [ ] **Step 7: Commit**

```bash
git add crates/spades-core/Cargo.toml crates/spades-core/src/cards.rs crates/spades-core/src/rules.rs crates/spades-core/src/rules/scoring.rs crates/spades-core/src/lib.rs
git commit -m "feat(spades): Spades ruleset implementing trick_engine::Ruleset"
```

---

## Task 10: Rewrite `spades-core::Game` as a delegating facade

**Files:**
- Modify: `crates/spades-core/src/lib.rs`
- Modify: `crates/spades-core/src/game_state.rs`

**Interfaces:**
- Consumes: `trick_engine::{Game as EngineGame, Action, State as EngineState, StepOutcome, StepError}`, `rules::Spades`, `cards::{to_tn, from_tn}`.
- Produces: `pub struct Game` wrapping `EngineGame` + timer fields; preserves the entire existing public API: `new(id, [Uuid;4], max_points, Option<TimerConfig>)`, `play(GameTransition)`, `get_state`, `get_legal_cards`, `get_current_player_id`, `get_hand_by_player_id`, `get_team_a_score`/`b`, `get_team_a_bags`/`b`, `get_player_bets`, `get_player_tricks_won`, `get_current_trick_cards`, `get_leading_suit`, `get_winner_ids`, `get_last_trick_winner_id`, `get_last_completed_trick`, names, clocks, `get_history`, `get_all_bets`, `get_round_index`, `is_in_betting_stage`, `is_first_round_betting`, `get_max_points`, `override_hands`, `set_state`. `State` re-exported from the engine with a `Betting` alias.

- [ ] **Step 1: Map `State` with the `Betting` alias**

Replace `crates/spades-core/src/game_state.rs` with a re-export plus alias:

```rust
//! Spades surfaces the engine's `State`, but keeps the historical `Betting`
//! name (the engine calls the same phase `Bidding`). Both name the same value.

pub use trick_engine::State;

/// Back-compat alias: the engine's bidding phase, named `Betting` for spades.
/// Construct with `betting(rotation)`; match with `State::Bidding(n)`.
pub fn betting(rotation: usize) -> State {
    State::Bidding(rotation)
}
```

> Note: `State::Betting(_)` was a variant; callers that pattern-match `State::Betting(n)` must change to `State::Bidding(n)`. Audit `spades-server` and `web` consumers. In `spades-core` itself, update all `State::Betting` matches to `State::Bidding`. The web reads serialized state strings — confirm the WS/DTO layer maps `Bidding` to whatever the web expects (Task 12 handles the OpenAPI/DTO regen and any web state-string check).

- [ ] **Step 2: Write the failing test (facade parity)**

Add to `crates/spades-core/src/lib.rs` test section (or a new `#[cfg(test)] mod facade_tests`):

```rust
#[cfg(test)]
mod facade_tests {
    use super::*;
    use uuid::Uuid;

    fn ids() -> [Uuid; 4] {
        [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3), Uuid::from_u128(4)]
    }

    #[test]
    fn full_game_drives_to_completion_through_facade() {
        let mut g = Game::new(Uuid::from_u128(9), ids(), 50, None);
        g.play(GameTransition::Start).unwrap();
        while *g.get_state() != State::Completed {
            match g.get_state() {
                State::Bidding(_) => { g.play(GameTransition::Bet(3)).unwrap(); }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    g.play(GameTransition::Card(legal[0])).unwrap();
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(*g.get_state(), State::Completed);
        assert!(g.get_team_a_score().is_ok());
    }

    #[test]
    fn spades_not_broken_error_is_preserved() {
        // Re-derives the precise spades error from the engine's IllegalPlay.
        // Construct a hand where leading a spade is illegal, assert the variant.
        // (Use override_hands to set a deterministic hand; see helper below.)
        // Detailed setup uses the same approach as the existing lib tests.
    }
}
```

> The second test's body should mirror an existing spades-not-broken test in `src/tests/` — port that test's setup to the facade. Find it before writing.

- [ ] **Step 3: Replace the `Game` struct and impl**

Rewrite the `Game` struct and its `impl` in `crates/spades-core/src/lib.rs`. Replace the struct (`lib.rs:149-171`) with:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Game {
    inner: trick_engine::Game,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timer_config: Option<TimerConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    player_clocks: Option<PlayerClocks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    turn_started_at_epoch_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_trick_winner: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_completed_trick: Option<[Card; 4]>,
}
```

`new` builds the engine with a boxed `Spades`:

```rust
impl Game {
    pub fn new(id: Uuid, player_ids: [Uuid; 4], max_points: i32, timer_config: Option<TimerConfig>) -> Game {
        let player_clocks = timer_config.map(|tc| PlayerClocks {
            remaining_ms: [tc.initial_time_secs * 1000; 4],
        });
        let rules = Box::new(crate::rules::Spades::new(max_points));
        Game {
            inner: trick_engine::Game::new(id, player_ids.to_vec(), rules),
            timer_config,
            player_clocks,
            turn_started_at_epoch_ms: None,
            last_trick_winner: None,
            last_completed_trick: None,
        }
    }
```

Then re-implement each accessor by delegating to `self.inner` and converting cards via `from_tn`/`to_tn`. Key mappings (implement each; these are the non-obvious ones):

```rust
    pub fn get_state(&self) -> &State {
        self.inner.state()
    }

    pub fn play(&mut self, entry: GameTransition) -> Result<TransitionSuccess, TransitionError> {
        self.last_completed_trick = None;
        let action = match entry {
            GameTransition::Start => trick_engine::Action::Start,
            GameTransition::Bet(b) => trick_engine::Action::Bid(b),
            GameTransition::Card(c) => trick_engine::Action::Play(crate::cards::to_tn(c)),
            GameTransition::Abort => trick_engine::Action::Abort,
        };
        match self.inner.step(action) {
            Ok(outcome) => {
                // Capture last completed trick / winner for the getters.
                if matches!(outcome, trick_engine::StepOutcome::TrickComplete
                    | trick_engine::StepOutcome::RoundComplete
                    | trick_engine::StepOutcome::GameOver)
                {
                    if let Some(last) = self.inner.history().last() {
                        if last.iter().all(|c| c.is_some()) {
                            let arr: [Card; 4] = std::array::from_fn(|i| {
                                crate::cards::from_tn(last[i].as_ref().unwrap()).unwrap()
                            });
                            self.last_completed_trick = Some(arr);
                            self.last_trick_winner = Some(self.inner.trick_leader());
                        }
                    }
                }
                Ok(map_outcome(outcome))
            }
            Err(e) => Err(self.map_step_error(e, entry)),
        }
    }
```

Add free fn `map_outcome(StepOutcome) -> TransitionSuccess` and method `map_step_error`:

```rust
fn map_outcome(o: trick_engine::StepOutcome) -> TransitionSuccess {
    use trick_engine::StepOutcome as O;
    match o {
        O::Started => TransitionSuccess::Start,
        O::Bid => TransitionSuccess::Bet,
        O::BidComplete => TransitionSuccess::BetComplete,
        O::PlayCard => TransitionSuccess::PlayCard,
        O::TrickComplete | O::RoundComplete => TransitionSuccess::Trick,
        O::GameOver => TransitionSuccess::GameOver,
        O::Aborted => TransitionSuccess::Aborted,
    }
}

impl Game {
    /// Translate the engine's coarse error into the precise spades variant the
    /// public API has always returned. The engine says "no"; spades explains why.
    fn map_step_error(&self, e: trick_engine::StepError, entry: GameTransition) -> TransitionError {
        use trick_engine::StepError as E;
        match e {
            E::NotStarted => TransitionError::NotStarted,
            E::AlreadyStarted => TransitionError::AlreadyStarted,
            E::Completed => TransitionError::CompletedGame,
            E::IllegalBid => TransitionError::InvalidBet,
            E::CardNotInHand => TransitionError::CardNotInHand,
            E::WrongPhase => match entry {
                GameTransition::Bet(_) => TransitionError::BetInTrickStage,
                GameTransition::Card(_) => TransitionError::CardInBettingStage,
                _ => TransitionError::CompletedGame,
            },
            E::IllegalPlay => self.explain_illegal_play(entry),
        }
    }

    /// Re-derive SpadesNotBroken vs CardIncorrectSuit for an in-hand-but-illegal
    /// card, matching the historical engine behavior.
    fn explain_illegal_play(&self, entry: GameTransition) -> TransitionError {
        let GameTransition::Card(card) = entry else {
            return TransitionError::CardIncorrectSuit;
        };
        let seat = self.inner.current_seat();
        let hand: Vec<Card> = self.inner.hand(seat).iter().filter_map(crate::cards::from_tn).collect();
        let leader = self.inner.trick_leader();
        let table = self.inner.current_trick();
        let leading = table[leader].as_ref().and_then(crate::cards::from_tn).map(|c| c.suit);
        match leading {
            None => TransitionError::SpadesNotBroken, // leading a spade while unbroken
            Some(ls) if card.suit != ls && hand.iter().any(|c| c.suit == ls) => {
                TransitionError::CardIncorrectSuit
            }
            _ => TransitionError::CardIncorrectSuit,
        }
    }
}
```

Implement the remaining accessors straightforwardly (each is 1–4 lines):
- `get_team_a_score`/`b` → `require_started`, read `self.inner.rules()` downcast? No downcast: add accessors on `Spades` and reach them through a `spades_rules()` helper. Implement `fn spades_rules(&self) -> &crate::rules::Spades` by storing nothing extra — instead expose the needed scalars through new `Ruleset`-independent methods on the facade by reading `self.inner.rules().scores()` for scores, and add `bags`/`bets`/`tricks` reads. Since `scores()` returns `[team_a, team_b]`, `get_team_a_score = scores()[0]`. For bags/bets/tricks (not in the generic trait), add **typed accessors** by having the engine expose `rules_as<T>()`:

```rust
// in trick-engine game.rs:
impl Game {
    /// Downcast the boxed ruleset to a concrete type, if it matches.
    pub fn rules_as<T: Ruleset + 'static>(&self) -> Option<&T> {
        // typetag/Any: require Ruleset: Any. Add `fn as_any(&self) -> &dyn Any`
        // to the trait, default-implemented per impl.
        self.rules().as_any().downcast_ref::<T>()
    }
}
```

> This requires adding `fn as_any(&self) -> &dyn std::any::Any;` to the `Ruleset` trait and implementing it (`fn as_any(&self) -> &dyn Any { self }`) in `HighCard`, `SimpleBid`, and `Spades`. Add that to the trait in Task 3's file as part of this task, with a one-line test that `rules_as::<Spades>()` returns `Some`. Then facade bag/bet/trick getters read `self.inner.rules_as::<Spades>().unwrap().scoring()`.

- `get_player_bets` → `Some(self.inner.bids().try_into().unwrap())` when started.
- `get_player_tricks_won` → expose `self.inner.tricks_won()` (add a getter in the engine returning `&[i32]`), map to `[i32;4]`.
- `get_history` → convert `self.inner.history()` (`Vec<Vec<Option<TnCard>>>`) into the legacy `&[[Option<Card>;4]]` shape. Since types differ, change `get_history` to return an owned `Vec<[Option<Card>;4]>` OR keep a converted cache. **Decision:** change return type to `Vec<[Option<Card>; 4]>` (owned). Audit callers (`transcript/adapter.rs`, server DTO, web) — Task 11/12 update them.
- `get_legal_cards` → `self.inner.legal_plays()` mapped through `from_tn`.
- `get_current_trick_cards` → convert `self.inner.current_trick()` to `[Option<Card>;4]` (owned).
- `get_leading_suit` → `self.inner.current_trick()[leader]` → `from_tn` → `.suit`.
- timer/name/clock accessors → delegate to `self.inner.player_mut`/`player_id` for names; timer fields stay local.
- `override_hands` → for each seat, `self.inner.player_mut(seat).hand = hands[seat].into_iter().map(to_tn).collect()`.
- `set_state` → add `pub fn set_state(&mut self, s: State)` to the engine; delegate. (Used by transcript replay/tests.)

> Some accessors change return types from borrowed to owned (`get_history`, `get_current_trick_cards`). This is an internal-API change; the spec permits it (owner treats internal breaks liberally). Callers are updated in Tasks 11–12.

- [ ] **Step 4: Run spades-core tests**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p spades`
Expected: the facade tests pass; pre-existing `src/tests/` may fail where they match `State::Betting` or use changed return types — fix those call sites to `State::Bidding` and the new owned return types. Iterate until green.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-core/src crates/trick-engine/src
git commit -m "feat(spades): Game facade delegating to trick-engine; preserve public API"
```

---

## Task 11: Update the transcript adapter

**Files:**
- Modify: `crates/spades-core/src/transcript/adapter.rs`

**Interfaces:**
- Consumes: the facade `Game`'s updated accessors (owned `get_history`, etc.), `cards::{to_tn, from_tn}`.
- Produces: an adapter that reads engine-derived state. The bespoke `card_to_tn`/`tn_to_card`/`rank_sym`/`suit_sym` helpers (`adapter.rs:21-100`) are replaced by `cards::to_tn`/`from_tn`; per-round bet/trick reconstruction now reads `get_all_bets`/`get_history` from the facade.

- [ ] **Step 1: Run the existing transcript tests to see breakage**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p spades transcript`
Expected: compile errors where the adapter used removed internal fields / borrowed return types.

- [ ] **Step 2: Replace the local card maps with `cards::{to_tn, from_tn}`**

Delete `rank_sym`, `suit_sym`, `rank_from_sym`, `suit_from_sym`, `card_to_tn`, `tn_to_card` from `adapter.rs`; replace call sites with `crate::cards::to_tn(card)` and `crate::cards::from_tn(&tn).ok_or(ReplayError::BadCard { token: trick_notation::format_card(&tn) })`.

- [ ] **Step 3: Adapt to owned `get_history` / state accessors**

Update `game_to_model` to consume the facade's owned `get_history()` `Vec<[Option<Card>;4]>` and `get_all_bets()`/`get_player_names()`/`get_state()` as before (these accessor names are unchanged). Update any `State::Betting` match to `State::Bidding`.

- [ ] **Step 4: Run transcript + property tests**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo test -p spades transcript`
Expected: PASS, including `property_tests::round_trip_is_idempotent_on_many_random_games`.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-core/src/transcript/adapter.rs
git commit -m "refactor(spades): transcript adapter reuses cards::{to_tn,from_tn}, reads engine state"
```

---

## Task 12: Integrate server + web; regen OpenAPI; full gate

**Files:**
- Modify: `crates/spades-server/` (only where it matched `State::Betting` or used changed return types)
- Modify: `web/openapi/openapi.json`, `web/src/api/schema.d.ts` (regen)
- Modify: `web/src/state/*` only if a `State` string changed

**Interfaces:**
- Consumes: the facade `Game` API.
- Produces: a fully building workspace + web, with OpenAPI artifacts regenerated and `make check` green.

- [ ] **Step 1: Build the server, fix `State` matches**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; cargo build -p spades-server`
Fix any `State::Betting` → `State::Bidding` matches and any use of now-owned return types (`get_history`, `get_current_trick_cards`). Search: `grep -rn "State::Betting\|get_history\|get_current_trick_cards" crates/spades-server/src`.

- [ ] **Step 2: Check the WS/DTO state serialization**

Inspect `crates/spades-server/src/bin/server/dto.rs` and `ws.rs` for how `State` serializes to the web. If the DTO sends the serde representation of `State`, the variant rename `Betting`→`Bidding` changes the wire string. Decide: either (a) keep the DTO mapping emitting `"betting"` for `State::Bidding` (add an explicit map in the DTO), or (b) update `web/src/state/` to read `"bidding"`. Prefer (a) to avoid web churn. Implement the chosen mapping.

- [ ] **Step 3: Regenerate OpenAPI artifacts**

Per CLAUDE.md: start the server, fetch, generate, commit both files.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo run -p spades-server --features openapi --bin server -- --insecure-cookies &
sleep 3
pnpm -C web openapi:fetch
pnpm -C web openapi:generate
kill %1
git diff --stat web/openapi/openapi.json web/src/api/schema.d.ts
```

Review the diff: confirm no unintended schema changes (the facade preserves DTO shapes; only intended changes should appear).

- [ ] **Step 4: Run the full gate**

Run: `export PATH="$HOME/.cargo/bin:$PATH"; make check`
Expected: fmt-check, clippy `-D warnings`, `cargo test --workspace`, web unit/component tests, and e2e all green.

- [ ] **Step 5: Update coverage baseline for the new crate**

Per CLAUDE.md / docs/coverage.md, the coverage ratchet compares per-crate line coverage to `coverage-baseline.json`. Add a `trick-engine` entry by running `hooks/update-coverage-baseline.sh` (intentional baseline change) and commit.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: integrate trick-engine across server/web; regen OpenAPI; coverage baseline"
```

---

## Self-Review

**Spec coverage:**
- Three-layer stack → Tasks 1–3 (engine crate on trick-notation), 9–10 (facade). ✓
- Trait-object `Box<dyn Ruleset>` + object-safety → Task 3 (no generic/`Self` methods). ✓
- typetag serialization → Tasks 3, 8 (round-trip), and Task 1 dep. ✓
- Variable seat count → Task 2/4 (`Vec<Player>`, `seat_count`, `% n`). ✓
- Preserve spades facade API → Task 10 (every accessor enumerated). ✓
- `State::Betting` alias → Tasks 10, 12. ✓
- Stateful ruleset owns scoring → Task 9 (`Spades { scoring }`), finalize_round. ✓
- Spades trump / spades-broken in ruleset → Task 9 `legal_plays`/`trick_winner`. ✓
- Precise error mapping (SpadesNotBroken etc.) → Task 10 `explain_illegal_play`. ✓
- Transcript adapter simplification → Task 11. ✓
- typetag dep pre-flight → confirmed at plan time (v0.2.22 resolves); Task 12 `make check` is the integration gate. ✓
- Persisted-game reset (no shim) → no migration task by design; note carried in Task 12 deploy step. ✓ (Add reset note to SERVER.md runbook during Task 12 Step 6 if a `--db` is in use.)
- OpenAPI regen → Task 12. ✓

**Placeholder scan:** Task 10 Step 2's second test and the `explain_illegal_play` setup reference "port the existing test" — these point to concrete existing tests in `src/tests/`, not invented behavior; the implementer reads the named source. The conditional `Deck::cards()` (Task 3 Step 3a) is gated on inspection. No `TODO`/`TBD` requirements remain.

**Type consistency:** `to_tn`/`from_tn` (cards.rs), `Spades::new`/`scoring()` (rules.rs), `finalize_round(&[i32], &[i32])` (scoring.rs), `Action`/`StepOutcome`/`StepError`/`rules_as`/`as_any`/`tricks_won`/`set_state` (engine) — names are used identically across Tasks 4–12. `map_outcome`/`map_step_error`/`explain_illegal_play` defined and used only in Task 10.

**Known risk to flag at execution:** Task 10 is the largest task and may warrant splitting (struct+play vs accessor delegation) if a reviewer wants a tighter gate; left whole because the facade doesn't compile until every accessor is delegated.
