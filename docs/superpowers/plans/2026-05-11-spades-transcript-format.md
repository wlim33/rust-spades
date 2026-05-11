# Spades Transcript Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a PGN-inspired `transcript` module in `spades-core` that encodes any `Game` to a deterministic text format, decodes it back, and replays it through the engine — with vigorous unit testing per the design spec at `docs/superpowers/specs/2026-05-11-spades-transcript-format-design.md`.

**Architecture:** New `crates/spades-core/src/transcript/` module with five files (`mod.rs`, `format.rs`, `encode.rs`, `decode.rs`, `replay.rs`). Each file owns one concern and has inline `#[cfg(test)]` tests. A round-trip property test lives in `transcript/mod.rs`. The engine itself gets a small history-retention fix (task 1) and two narrow accessors so the transcript can reach the data it needs without breaking encapsulation.

**Tech Stack:** Rust 2024 edition, `serde`, `uuid`, `rand`, `ntest` (test attribute macros). No new dependencies.

---

## Background: engine bug discovered during planning

`Game::hands_played` accumulates trick arrays but is **not extended at round boundaries**. After round 0's 13 tricks, `hands_played` has 13 entries. The lib.rs round-end branch (around line 456 in `crates/spades-core/src/lib.rs`) does not push a new empty slot. When round 1's first card is played, the engine writes to `hands_played.last_mut()`, which is round 0's trick 12 — silently overwriting it.

The engine still functions correctly (game logic doesn't depend on past-round history), but the data is lost. Task 1 adds one `push([None; 4])` call to fix this. No existing tests cover multi-round history retention; the fix is behaviorally invisible to current callers.

---

## Task 1: Engine history retention + minimal accessors

**Files:**
- Modify: `crates/spades-core/src/lib.rs`

- [ ] **Step 1: Write the failing test for history retention**

Append to `crates/spades-core/src/tests/spades_game_api_unit.rs` (after the last existing test):

```rust
#[test]
fn history_preserved_across_round_boundary() {
    use crate::{Game, GameTransition, State};
    use uuid::Uuid;

    let mut g = Game::new(Uuid::new_v4(), [Uuid::new_v4(); 4], 50, None);
    g.play(GameTransition::Start).unwrap();

    // Bet round 0
    for _ in 0..4 {
        g.play(GameTransition::Bet(3)).unwrap();
    }

    // Play all 13 tricks of round 0 by always picking the first legal card.
    for _ in 0..13 {
        for _ in 0..4 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
    }

    // We should now be in betting state for round 1 (game isn't over with max_points=50 after 1 round of low bets).
    assert!(matches!(g.get_state(), State::Betting(_)));

    // History must already contain a slot for round 1's first trick (14 total entries),
    // so round 0's tricks 0..12 remain intact.
    assert_eq!(g.get_history().len(), 14);

    // The slot pushed for round 1 must be empty.
    let last = g.get_history().last().unwrap();
    assert!(last.iter().all(|c| c.is_none()));

    // All 13 round-0 trick slots must be fully populated.
    for (i, trick) in g.get_history()[..13].iter().enumerate() {
        for (s, c) in trick.iter().enumerate() {
            assert!(c.is_some(), "round 0 trick {} seat {} should be Some", i, s);
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades history_preserved_across_round_boundary
```

Expected: compile error first (`get_history` doesn't exist) — fix by adding the accessor in Step 3. Then expected: PASS once history retention is fixed.

- [ ] **Step 3: Apply the engine fix**

In `crates/spades-core/src/lib.rs`, locate the round-end branch inside the `GameTransition::Card` match (around line 456). Find:

```rust
                            if self.scoring.in_betting_stage {
                                self.last_trick_winner = None;
                                self.current_player_index = 0;
                                self.state = State::Betting((rotation_status + 1) % 4);
                                self.spades_broken = false;
                                self.leading_suit = None;
                                self.deal_cards();
                            } else {
```

Change it to:

```rust
                            if self.scoring.in_betting_stage {
                                self.last_trick_winner = None;
                                self.current_player_index = 0;
                                self.state = State::Betting((rotation_status + 1) % 4);
                                self.spades_broken = false;
                                self.leading_suit = None;
                                self.deal_cards();
                                self.hands_played.push([None; 4]);
                            } else {
```

- [ ] **Step 4: Add the three transcript accessors**

In `crates/spades-core/src/lib.rs`, inside `impl Game { ... }` (after `get_legal_cards`, before `fn deal_cards`), add:

```rust
    /// Max points configured at game creation.
    pub fn get_max_points(&self) -> i32 {
        self.scoring.config.max_points
    }

    /// All trick slots, one per trick. For round R the slots live at indices
    /// 13*R .. 13*(R+1). The final slot may be partially filled (current trick).
    /// Empty trailing slot during betting between rounds is intentional.
    pub fn get_history(&self) -> &[[Option<cards::Card>; 4]] {
        &self.hands_played
    }

    /// All bets per round in seat order. `bets_placed[R][s]` is seat `s`'s bet
    /// for round `R`. The trailing entry is a write target for the next round's
    /// bets and may be all zeros even when no bets have been placed.
    pub fn get_all_bets(&self) -> &[[i32; 4]] {
        &self.scoring.bets_placed
    }

    /// Current 0-based round index (`scoring.round`).
    pub fn get_round_index(&self) -> usize {
        self.scoring.round
    }

    /// True when the game is in (or just finished) a betting phase rather than
    /// a trick phase. Combined with `get_state()` this disambiguates Aborted
    /// games.
    pub fn get_in_betting_stage(&self) -> bool {
        self.scoring.in_betting_stage
    }
```

- [ ] **Step 5: Run all spades-core tests to confirm no regressions**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades
```

Expected: all existing tests pass; the new `history_preserved_across_round_boundary` passes.

- [ ] **Step 6: Commit**

```bash
git add crates/spades-core/src/lib.rs crates/spades-core/src/tests/spades_game_api_unit.rs
git commit -m "core: preserve hands_played across round boundary + accessors for transcript"
```

---

## Task 2: Module skeleton and types

**Files:**
- Create: `crates/spades-core/src/transcript/mod.rs`
- Create: `crates/spades-core/src/transcript/format.rs` (empty stub)
- Create: `crates/spades-core/src/transcript/encode.rs` (empty stub)
- Create: `crates/spades-core/src/transcript/decode.rs` (empty stub)
- Create: `crates/spades-core/src/transcript/replay.rs` (empty stub)
- Modify: `crates/spades-core/src/lib.rs`

- [ ] **Step 1: Wire the module in lib.rs**

In `crates/spades-core/src/lib.rs`, find:

```rust
mod scoring;
mod game_state;
mod cards;
mod result;
pub mod ai;
```

Add immediately after:

```rust
pub mod transcript;
```

- [ ] **Step 2: Create the format.rs / encode.rs / decode.rs / replay.rs stubs**

Each of these four files should be created with this exact content (one line each):

`crates/spades-core/src/transcript/format.rs`:
```rust
// Card notation, tag-value escaping, format constants. Implemented in task 3.
```

`crates/spades-core/src/transcript/encode.rs`:
```rust
// Game -> String encoder. Implemented in task 5.
```

`crates/spades-core/src/transcript/decode.rs`:
```rust
// &str -> Transcript decoder. Implemented in task 8.
```

`crates/spades-core/src/transcript/replay.rs`:
```rust
// Transcript -> Game replay. Implemented in task 11.
```

- [ ] **Step 3: Create transcript/mod.rs with types**

Create `crates/spades-core/src/transcript/mod.rs`:

```rust
//! Spades Transcript Format (STF) — PGN-inspired serialization of full game history.
//!
//! See `docs/superpowers/specs/2026-05-11-spades-transcript-format-design.md`.

use std::fmt;
use uuid::Uuid;

use crate::cards::Card;
use crate::result::TransitionError;
use crate::TimerConfig;

mod format;
mod encode;
mod decode;
mod replay;

pub use encode::encode;
pub use decode::decode;
pub use replay::replay;

/// Parsed transcript. Constructed by `decode`, consumed by `replay`, produced
/// alongside `encode`'s String for round-trip testing helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub headers: Headers,
    pub rounds: Vec<Round>,
    pub termination: Termination,
    /// Final cumulative team scores, `(team_a, team_b)`. `None` when `termination == InProgress`.
    pub result: Option<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headers {
    pub game_id: Uuid,
    pub max_points: i32,
    pub player_ids: [Uuid; 4],
    pub names: [Option<String>; 4],
    pub timer: Option<TimerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    /// Dealt hand per seat at the start of the round, sorted by `Card::Ord`.
    pub hands: [Vec<Card>; 4],
    /// Bets in seat order. Length 0..=4; a partial vec means the round was
    /// captured mid-betting.
    pub bets: Vec<i32>,
    /// Tricks in play order. Each inner Vec has 1..=4 cards; the last trick
    /// may be partial (mid-trick capture).
    pub tricks: Vec<Vec<Card>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Completed,
    Aborted,
    InProgress,
}

impl fmt::Display for Termination {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Termination::Completed => "Completed",
            Termination::Aborted => "Aborted",
            Termination::InProgress => "InProgress",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    UnexpectedEof,
    BadTag { line: usize, found: String },
    DuplicateTag { line: usize, key: String },
    MissingRequiredTag { key: &'static str },
    BadCard { line: usize, token: String },
    DuplicateRound { round: usize },
    NonMonotonicRound { expected: usize, found: usize },
    TooManyTricks { round: usize },
    TooManyBets { round: usize },
    TooManyCardsInTrick { round: usize, trick: usize },
    BadResult { line: usize, value: String },
    BadTermination { line: usize, value: String },
    BadUuid { line: usize, value: String },
    BadInteger { line: usize, value: String },
    BadEscape { line: usize, value: String },
    TrailingContent { line: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "transcript decode error: {:?}", self)
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// One of TimerInitial/TimerIncrement was present without the other.
    TimerHalfSpecified,
    /// Round R seat S's declared dealt hand contradicts the cards actually played.
    HandMismatch { round: usize, seat: usize },
    /// `Game::play` rejected a transition synthesized from the transcript.
    Transition {
        round: usize,
        trick: Option<usize>,
        seat: usize,
        err: TransitionError,
    },
    /// Header `Termination` doesn't match the state the replayed game ended in.
    TerminationMismatch {
        declared: Termination,
        actual: Termination,
    },
    /// Header `Result` doesn't match replayed cumulative scores.
    ResultMismatch {
        declared: (i32, i32),
        actual: (i32, i32),
    },
    /// `Bets` line had a count not matching the state when termination is final
    /// (e.g. Completed transcript with < 4 bets in a round).
    InconsistentBetCount { round: usize, found: usize },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "transcript replay error: {:?}", self)
    }
}

impl std::error::Error for ReplayError {}
```

- [ ] **Step 4: Verify it compiles**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p spades
```

Expected: warning about unused imports inside the stub files is fine; everything compiles. The `pub use encode::encode;` etc. lines will fail until the stubs grow real `encode/decode/replay` functions. **Fix:** replace each stub one-line comment with a placeholder function so the re-exports compile:

`crates/spades-core/src/transcript/encode.rs`:
```rust
use crate::Game;

pub fn encode(_game: &Game) -> String {
    String::new()
}
```

`crates/spades-core/src/transcript/decode.rs`:
```rust
use super::{DecodeError, Transcript};

pub fn decode(_text: &str) -> Result<Transcript, DecodeError> {
    Err(DecodeError::UnexpectedEof)
}
```

`crates/spades-core/src/transcript/replay.rs`:
```rust
use crate::Game;

use super::{ReplayError, Transcript};

pub fn replay(_t: &Transcript) -> Result<Game, ReplayError> {
    Err(ReplayError::TimerHalfSpecified)
}
```

(format.rs can stay a comment-only file for now.)

Re-run `cargo build -p spades`. Expected: builds clean.

- [ ] **Step 5: Commit**

```bash
git add crates/spades-core/src/lib.rs crates/spades-core/src/transcript/
git commit -m "transcript: scaffold module + types (encode/decode/replay stubs)"
```

---

## Task 3: Card notation in format.rs

**Files:**
- Modify: `crates/spades-core/src/transcript/format.rs`

- [ ] **Step 1: Write the failing tests**

Replace the comment in `crates/spades-core/src/transcript/format.rs` with:

```rust
use crate::cards::{Card, Rank, Suit};

pub(super) fn card_to_str(c: Card) -> [u8; 2] {
    [rank_byte(c.rank), suit_byte(c.suit)]
}

pub(super) fn rank_byte(r: Rank) -> u8 {
    match r {
        Rank::Two => b'2',
        Rank::Three => b'3',
        Rank::Four => b'4',
        Rank::Five => b'5',
        Rank::Six => b'6',
        Rank::Seven => b'7',
        Rank::Eight => b'8',
        Rank::Nine => b'9',
        Rank::Ten => b'T',
        Rank::Jack => b'J',
        Rank::Queen => b'Q',
        Rank::King => b'K',
        Rank::Ace => b'A',
    }
}

pub(super) fn suit_byte(s: Suit) -> u8 {
    match s {
        Suit::Club => b'C',
        Suit::Diamond => b'D',
        Suit::Heart => b'H',
        Suit::Spade => b'S',
    }
}

pub(super) fn parse_card(token: &str) -> Option<Card> {
    let bytes = token.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let rank = match bytes[0] {
        b'2' => Rank::Two,
        b'3' => Rank::Three,
        b'4' => Rank::Four,
        b'5' => Rank::Five,
        b'6' => Rank::Six,
        b'7' => Rank::Seven,
        b'8' => Rank::Eight,
        b'9' => Rank::Nine,
        b'T' => Rank::Ten,
        b'J' => Rank::Jack,
        b'Q' => Rank::Queen,
        b'K' => Rank::King,
        b'A' => Rank::Ace,
        _ => return None,
    };
    let suit = match bytes[1] {
        b'C' => Suit::Club,
        b'D' => Suit::Diamond,
        b'H' => Suit::Heart,
        b'S' => Suit::Spade,
        _ => return None,
    };
    Some(Card { rank, suit })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(c: Card) -> String {
        let b = card_to_str(c);
        String::from_utf8(b.to_vec()).unwrap()
    }

    #[test]
    fn every_card_round_trips() {
        for suit in [Suit::Club, Suit::Diamond, Suit::Heart, Suit::Spade] {
            for rank in [
                Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six,
                Rank::Seven, Rank::Eight, Rank::Nine, Rank::Ten, Rank::Jack,
                Rank::Queen, Rank::King, Rank::Ace,
            ] {
                let c = Card { suit, rank };
                let txt = s(c);
                assert_eq!(parse_card(&txt), Some(c), "round trip {}", txt);
            }
        }
    }

    #[test]
    fn known_examples() {
        assert_eq!(s(Card { suit: Suit::Club, rank: Rank::Two }), "2C");
        assert_eq!(s(Card { suit: Suit::Club, rank: Rank::Ten }), "TC");
        assert_eq!(s(Card { suit: Suit::Spade, rank: Rank::Ace }), "AS");
        assert_eq!(s(Card { suit: Suit::Diamond, rank: Rank::King }), "KD");
        assert_eq!(s(Card { suit: Suit::Heart, rank: Rank::Jack }), "JH");
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(parse_card("").is_none());
        assert!(parse_card("A").is_none());
        assert!(parse_card("AKS").is_none());
        assert!(parse_card("1C").is_none(), "1 is not a valid rank");
        assert!(parse_card("AX").is_none(), "X is not a valid suit");
        assert!(parse_card("aC").is_none(), "lowercase rank not accepted");
        assert!(parse_card("Ac").is_none(), "lowercase suit not accepted");
        assert!(parse_card("0S").is_none());
        assert!(parse_card("TT").is_none());
    }
}
```

- [ ] **Step 2: Run the tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::format
```

Expected: all three tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/spades-core/src/transcript/format.rs
git commit -m "transcript: card notation encode/decode helpers"
```

---

## Task 4: Tag-value escaping in format.rs

**Files:**
- Modify: `crates/spades-core/src/transcript/format.rs`

- [ ] **Step 1: Append the failing tests + functions**

Append to `crates/spades-core/src/transcript/format.rs` (before the `#[cfg(test)] mod tests {` line):

```rust
/// Escape a tag value for emission: `"` -> `\"`, `\` -> `\\`. Nothing else.
pub(super) fn escape_tag_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out
}

/// Unescape a tag value. Returns None on any unrecognized backslash sequence
/// or trailing backslash.
pub(super) fn unescape_tag_value(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                _ => return None,
            }
        } else if ch == '"' {
            // bare unescaped quote inside value: invalid
            return None;
        } else {
            out.push(ch);
        }
    }
    Some(out)
}
```

Then add to the existing `mod tests` block:

```rust
    #[test]
    fn escape_passthrough_for_safe_strings() {
        assert_eq!(escape_tag_value("Alice"), "Alice");
        assert_eq!(escape_tag_value(""), "");
        assert_eq!(escape_tag_value("hello world"), "hello world");
    }

    #[test]
    fn escape_handles_quotes_and_backslash() {
        assert_eq!(escape_tag_value("a\"b"), "a\\\"b");
        assert_eq!(escape_tag_value("a\\b"), "a\\\\b");
        assert_eq!(escape_tag_value("\\\""), "\\\\\\\"");
    }

    #[test]
    fn escape_unescape_round_trip() {
        for s in [
            "",
            "Alice",
            "a\"b",
            "a\\b",
            "\\",
            "\"",
            "a\"b\\c\"d",
            "Carol \"the queen\" Q",
        ] {
            let esc = escape_tag_value(s);
            assert_eq!(unescape_tag_value(&esc).as_deref(), Some(s), "round trip {:?}", s);
        }
    }

    #[test]
    fn unescape_rejects_bad_sequences() {
        assert_eq!(unescape_tag_value("\\n"), None, "\\n not allowed");
        assert_eq!(unescape_tag_value("\\t"), None);
        assert_eq!(unescape_tag_value("\\"), None, "trailing backslash");
        assert_eq!(unescape_tag_value("\"bare"), None, "bare quote inside value");
        assert_eq!(unescape_tag_value("safe\""), None);
    }
```

- [ ] **Step 2: Run the tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::format
```

Expected: all 7 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/spades-core/src/transcript/format.rs
git commit -m "transcript: tag-value escaping (only \\\" and \\\\)"
```

---

## Task 5: Encoder — headers

**Files:**
- Modify: `crates/spades-core/src/transcript/encode.rs`

- [ ] **Step 1: Replace the stub with a header encoder + tests**

Replace the contents of `crates/spades-core/src/transcript/encode.rs` with:

```rust
use std::fmt::Write as _;

use crate::{Game, State};

use super::format::{card_to_str, escape_tag_value};

pub fn encode(game: &Game) -> String {
    let mut out = String::with_capacity(1024);
    encode_headers(&mut out, game);
    out.push('\n');
    encode_rounds(&mut out, game);
    out
}

fn encode_headers(out: &mut String, g: &Game) {
    writeln!(out, "[GameId \"{}\"]", g.get_id()).unwrap();
    writeln!(out, "[MaxPoints \"{}\"]", g.get_max_points()).unwrap();

    let names = g.get_player_names();
    for i in 0..4 {
        writeln!(out, "[Player{} \"{}\"]", i, names[i].0).unwrap();
    }
    for i in 0..4 {
        if let Some(n) = names[i].1 {
            writeln!(out, "[Name{} \"{}\"]", i, escape_tag_value(n)).unwrap();
        }
    }

    if let Some(t) = g.get_timer_config() {
        writeln!(out, "[TimerInitial \"{}\"]", t.initial_time_secs).unwrap();
        writeln!(out, "[TimerIncrement \"{}\"]", t.increment_secs).unwrap();
    }

    let termination = match g.get_state() {
        State::Completed => "Completed",
        State::Aborted => "Aborted",
        _ => "InProgress",
    };
    writeln!(out, "[Termination \"{}\"]", termination).unwrap();

    let result = match g.get_state() {
        State::Completed | State::Aborted => {
            let a = g.get_team_a_score().copied().unwrap_or(0);
            let b = g.get_team_b_score().copied().unwrap_or(0);
            format!("{}-{}", a, b)
        }
        _ => "*".to_string(),
    };
    writeln!(out, "[Result \"{}\"]", result).unwrap();
}

fn encode_rounds(_out: &mut String, _g: &Game) {
    // Implemented in task 6.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, TimerConfig};
    use uuid::Uuid;

    fn fixed_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn header_not_started_no_names_no_timer() {
        let g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            500,
            None,
        );
        let s = encode(&g);
        let expected = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn header_with_names_and_timer() {
        let mut g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            300,
            Some(TimerConfig { initial_time_secs: 300, increment_secs: 5 }),
        );
        g.set_player_name(fixed_uuid(10), Some("Alice".into())).unwrap();
        g.set_player_name(fixed_uuid(12), Some("Carol \"Q\"".into())).unwrap();
        let s = encode(&g);
        assert!(s.contains("[Name0 \"Alice\"]\n"));
        assert!(s.contains("[Name2 \"Carol \\\"Q\\\"\"]\n"));
        assert!(!s.contains("[Name1 "));
        assert!(!s.contains("[Name3 "));
        assert!(s.contains("[TimerInitial \"300\"]\n"));
        assert!(s.contains("[TimerIncrement \"5\"]\n"));
    }
}
```

- [ ] **Step 2: Run the tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::encode
```

Expected: both tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/spades-core/src/transcript/encode.rs
git commit -m "transcript: header encoder"
```

---

## Task 6: Encoder — round body

**Files:**
- Modify: `crates/spades-core/src/transcript/encode.rs`

- [ ] **Step 1: Add helpers and replace `encode_rounds`**

In `crates/spades-core/src/transcript/encode.rs`, replace the `fn encode_rounds(_out: &mut String, _g: &Game)` stub with the real implementation, and add the helpers. Update imports at the top of the file to add `Card`, `Suit`, `Rank`, and `get_trick_winner`:

```rust
use std::fmt::Write as _;

use crate::cards::{get_trick_winner, Card};
use crate::{Game, State};

use super::format::{card_to_str, escape_tag_value};
```

Then replace the `fn encode_rounds` stub:

```rust
fn encode_rounds(out: &mut String, g: &Game) {
    let num_rounds = num_rounds_to_emit(g);
    if num_rounds == 0 {
        return;
    }
    for r in 0..num_rounds {
        if r > 0 {
            out.push('\n');
        }
        encode_round(out, g, r, num_rounds);
    }
}

/// How many round blocks to emit. Round indices are 0..num_rounds.
fn num_rounds_to_emit(g: &Game) -> usize {
    match g.get_state() {
        State::NotStarted => 0,
        State::Completed => g.get_round_index(),
        State::Aborted => {
            // Aborted may have been called before Start: in that case history
            // is exactly one empty slot and no bets are placed.
            let history = g.get_history();
            let no_play = history.len() <= 1 && history.iter().all(|t| t.iter().all(|c| c.is_none()));
            let no_bets = g.get_all_bets().first().copied().unwrap_or([0; 4]) == [0; 4]
                && g.get_round_index() == 0
                && g.get_in_betting_stage();
            if no_play && no_bets {
                0
            } else {
                g.get_round_index() + 1
            }
        }
        State::Betting(_) | State::Trick(_) => g.get_round_index() + 1,
    }
}

fn encode_round(out: &mut String, g: &Game, round_idx: usize, num_rounds: usize) {
    writeln!(out, "[Round \"{}\"]", round_idx + 1).unwrap();

    let hands = dealt_hands_for_round(g, round_idx);
    for seat in 0..4 {
        write!(out, "[Hand{} \"", seat).unwrap();
        let mut first = true;
        for c in &hands[seat] {
            if !first {
                out.push(' ');
            }
            first = false;
            let b = card_to_str(*c);
            out.push(b[0] as char);
            out.push(b[1] as char);
        }
        out.push_str("\"]\n");
    }

    let bets = bets_for_round(g, round_idx);
    write!(out, "[Bets \"").unwrap();
    let mut first = true;
    for b in &bets {
        if !first {
            out.push(' ');
        }
        first = false;
        write!(out, "{}", b).unwrap();
    }
    out.push_str("\"]\n");

    let tricks = tricks_for_round(g, round_idx);
    // Determine lead seat for each trick.
    let mut lead = 0usize; // round always starts with seat 0
    for (t, trick_cards) in tricks.iter().enumerate() {
        if trick_cards.is_empty() {
            // Don't emit an empty trick line — happens only for the very last
            // trailing slot when state is mid-betting of a later round (which
            // shouldn't appear here since num_rounds_to_emit gates that case).
            continue;
        }
        write!(out, "{}.", t + 1).unwrap();
        for c in trick_cards {
            out.push(' ');
            let b = card_to_str(*c);
            out.push(b[0] as char);
            out.push(b[1] as char);
        }
        out.push('\n');
        // If the trick is complete, determine winner -> next lead.
        if trick_cards.len() == 4 {
            // Reconstruct by-seat cards. trick_cards[0] is at seat `lead`, etc.
            let mut by_seat = [Card { rank: crate::cards::Rank::Two, suit: crate::cards::Suit::Club }; 4];
            for i in 0..4 {
                by_seat[(lead + i) % 4] = trick_cards[i];
            }
            lead = get_trick_winner(lead, &by_seat);
        }
    }

    // Avoid "unused" warning when last round emits no trailing newline.
    let _ = num_rounds;
}

/// Reconstruct the dealt hand per seat at the start of round R.
fn dealt_hands_for_round(g: &Game, round_idx: usize) -> [Vec<Card>; 4] {
    let history = g.get_history();
    let trick_slots = &history[13 * round_idx..(13 * round_idx + 13).min(history.len())];

    let mut hands: [Vec<Card>; 4] = Default::default();
    for trick in trick_slots {
        for (seat, slot) in trick.iter().enumerate() {
            if let Some(c) = slot {
                hands[seat].push(*c);
            }
        }
    }

    // If this is the current round and the player still has cards in hand,
    // include them. (For past completed rounds, the engine has dealt new cards
    // already, so we must NOT pull from the current hand.)
    let is_current_round = match g.get_state() {
        State::Betting(_) | State::Trick(_) => g.get_round_index() == round_idx,
        State::Aborted => g.get_round_index() == round_idx,
        _ => false,
    };
    if is_current_round {
        let names = g.get_player_names();
        for seat in 0..4 {
            let pid = names[seat].0;
            if let Ok(hand) = g.get_hand_by_player_id(pid) {
                for c in hand {
                    hands[seat].push(*c);
                }
            }
        }
    }

    for h in &mut hands {
        h.sort();
    }
    hands
}

/// Bets emitted for round R. May be 0..=4 entries.
fn bets_for_round(g: &Game, round_idx: usize) -> Vec<i32> {
    let all = g.get_all_bets();
    let row = all.get(round_idx).copied().unwrap_or([0; 4]);
    let count = match g.get_state() {
        State::Betting(k) if g.get_round_index() == round_idx => *k,
        State::Aborted if g.get_round_index() == round_idx && g.get_in_betting_stage() => {
            // Aborted from betting: we can't recover k exactly. Emit the
            // longest prefix that contains no trailing default-0 in a slot
            // that hasn't been written. Heuristic: emit all 4 if any later
            // bet is nonzero; otherwise emit prefix until last nonzero.
            // To stay correct on all-nil cases, prefer 4 (over-reporting is
            // a replay error caught later; under-reporting silently drops
            // info). Trade-off: a single nil bid is indistinguishable from
            // "not bet" — choose to emit 4 in Aborted-betting mode.
            4
        }
        _ => 4,
    };
    row[..count].to_vec()
}

/// Tricks played in round R. Each inner Vec has cards in play order; the
/// last Vec may be partial.
fn tricks_for_round(g: &Game, round_idx: usize) -> Vec<Vec<Card>> {
    let history = g.get_history();
    let start = 13 * round_idx;
    let end = (start + 13).min(history.len());
    let mut out = Vec::new();
    let mut lead = 0usize;
    for slot_idx in start..end {
        let trick = &history[slot_idx];
        let count = trick.iter().filter(|c| c.is_some()).count();
        if count == 0 {
            // empty slot — skip (only happens for the trailing slot during
            // mid-betting of a later round; outer caller's gating prevents
            // emitting this round in that case)
            continue;
        }
        let mut play_order = Vec::with_capacity(count);
        for i in 0..4 {
            let seat = (lead + i) % 4;
            if let Some(c) = trick[seat] {
                play_order.push(c);
            } else {
                break;
            }
        }
        // If the trick is complete, compute the winner to set next lead.
        if count == 4 {
            let by_seat: [Card; 4] = [
                trick[0].unwrap(),
                trick[1].unwrap(),
                trick[2].unwrap(),
                trick[3].unwrap(),
            ];
            lead = get_trick_winner(lead, &by_seat);
        }
        out.push(play_order);
    }
    out
}
```

Also remove the now-unused `num_rounds` parameter shim. Replace the call site `encode_round(out, g, r, num_rounds);` with `encode_round(out, g, r);` and update the signature to drop the param. (Apply that edit at both call site and definition.)

- [ ] **Step 2: Add the round-body tests**

Append to the existing `mod tests` block in `encode.rs`:

```rust
    /// Helper: build a deterministic 4-player game and play it forward N transitions
    /// by always choosing the first legal card. Returns the live Game.
    fn play_first_legal(g: &mut Game, transitions: usize) {
        use crate::GameTransition;
        for _ in 0..transitions {
            match g.get_state() {
                State::NotStarted => g.play(GameTransition::Start).unwrap(),
                State::Betting(_) => g.play(GameTransition::Bet(3)).unwrap(),
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    g.play(GameTransition::Card(legal[0])).unwrap()
                }
                State::Completed | State::Aborted => return,
            };
        }
    }

    #[test]
    fn encode_mid_first_bet() {
        let mut g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            500,
            None,
        );
        play_first_legal(&mut g, 1); // Start
        play_first_legal(&mut g, 2); // 2 bets

        let s = encode(&g);
        assert!(s.contains("[Round \"1\"]\n"), "should have Round 1 block");
        assert!(s.contains("[Bets \"3 3\"]\n"), "should have 2 bets, got:\n{}", s);
        // Should NOT have any numbered trick lines yet.
        for line in s.lines() {
            assert!(
                !line.starts_with("1. ") && !line.starts_with("2. "),
                "unexpected trick line: {}",
                line
            );
        }
    }

    #[test]
    fn encode_completed_two_round_short_game() {
        // Use a low max_points and bet 13 each so one round terminates the game
        // (team A bid 13+13 = 26 if all bet 13... but that's overkill). Instead:
        // bet aggressively so points cross 100 in ~2 rounds.
        let mut g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            50,
            None,
        );
        play_first_legal(&mut g, 10_000); // play to completion
        assert_eq!(*g.get_state(), State::Completed);

        let s = encode(&g);
        assert!(s.contains("[Termination \"Completed\"]\n"));
        assert!(s.contains("[Round \"1\"]\n"));
        // Result line is "A-B"
        let result_line = s.lines().find(|l| l.starts_with("[Result \"")).unwrap();
        assert!(result_line.contains("-"));
        assert_ne!(result_line, "[Result \"*\"]");

        // Every emitted round should have a sorted 13-card hand per seat.
        for seat in 0..4 {
            let tag = format!("[Hand{} \"", seat);
            let occurrences: Vec<&str> = s.match_indices(&tag).map(|(_, m)| m).collect();
            assert!(!occurrences.is_empty(), "Hand{} not present", seat);
        }
    }

    #[test]
    fn hands_are_sorted() {
        let mut g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            500,
            None,
        );
        play_first_legal(&mut g, 5); // Start + 4 bets -> Trick(0)
        let s = encode(&g);
        for line in s.lines().filter(|l| l.starts_with("[Hand")) {
            let inside = line.trim_start_matches(|c| c != '"').trim_matches('"');
            // Parse the cards and confirm they're sorted.
            let cards: Vec<Card> = inside
                .split_whitespace()
                .map(|tok| super::super::format::parse_card(tok).unwrap())
                .collect();
            let mut sorted = cards.clone();
            sorted.sort();
            assert_eq!(cards, sorted, "hand not sorted: {}", line);
        }
    }

    #[test]
    fn aborted_from_not_started_emits_no_rounds() {
        let mut g = Game::new(
            fixed_uuid(1),
            [fixed_uuid(10), fixed_uuid(11), fixed_uuid(12), fixed_uuid(13)],
            500,
            None,
        );
        g.set_state(State::Aborted);
        let s = encode(&g);
        assert!(s.contains("[Termination \"Aborted\"]\n"));
        assert!(!s.contains("[Round "), "no rounds should be emitted, got:\n{}", s);
    }
```

- [ ] **Step 3: Run all transcript tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript
```

Expected: all encoder tests pass. (Decoder/replay are still stubs returning Err — they're not exercised here.)

- [ ] **Step 4: Commit**

```bash
git add crates/spades-core/src/transcript/encode.rs
git commit -m "transcript: round-body encoder (hands, bets, tricks)"
```

---

## Task 7: Decoder — headers

**Files:**
- Modify: `crates/spades-core/src/transcript/decode.rs`

- [ ] **Step 1: Replace the stub with a header parser**

Replace the contents of `crates/spades-core/src/transcript/decode.rs` with:

```rust
use uuid::Uuid;

use crate::TimerConfig;

use super::format::{parse_card, unescape_tag_value};
use super::{DecodeError, Headers, Round, Termination, Transcript};

pub fn decode(text: &str) -> Result<Transcript, DecodeError> {
    let mut parser = Parser::new(text);
    let headers = parser.parse_headers()?;
    let (termination, result) = parser.consume_termination_and_result()?;
    let rounds = parser.parse_rounds()?;
    parser.expect_eof()?;
    Ok(Transcript { headers, rounds, termination, result })
}

struct Parser<'a> {
    lines: Vec<(usize, &'a str)>, // (1-based line number, content)
    cursor: usize,
    // captured during header parse:
    termination: Option<Termination>,
    result: Option<(i32, i32)>,
    result_was_star: bool,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        let lines = text
            .split('\n')
            .enumerate()
            .map(|(i, l)| (i + 1, l))
            .collect();
        Parser { lines, cursor: 0, termination: None, result: None, result_was_star: false }
    }

    fn peek(&self) -> Option<&(usize, &'a str)> {
        self.lines.get(self.cursor)
    }

    fn advance(&mut self) -> Option<&(usize, &'a str)> {
        let v = self.lines.get(self.cursor);
        self.cursor += 1;
        v
    }

    fn parse_headers(&mut self) -> Result<Headers, DecodeError> {
        let mut game_id: Option<Uuid> = None;
        let mut max_points: Option<i32> = None;
        let mut player_ids: [Option<Uuid>; 4] = [None; 4];
        let mut names: [Option<String>; 4] = Default::default();
        let mut timer_initial: Option<u64> = None;
        let mut timer_increment: Option<u64> = None;

        loop {
            let Some((ln, line)) = self.peek().copied() else {
                return Err(DecodeError::UnexpectedEof);
            };
            if line.is_empty() {
                self.advance();
                break; // header section ended
            }
            if !line.starts_with('[') {
                return Err(DecodeError::BadTag { line: ln, found: line.to_string() });
            }
            let (key, value) = parse_tag_line(ln, line)?;
            self.advance();
            match key.as_str() {
                "GameId" => set_once(&mut game_id, parse_uuid(ln, &value)?, ln, "GameId")?,
                "MaxPoints" => set_once(&mut max_points, parse_int(ln, &value)?, ln, "MaxPoints")?,
                "Player0" => set_once(&mut player_ids[0], parse_uuid(ln, &value)?, ln, "Player0")?,
                "Player1" => set_once(&mut player_ids[1], parse_uuid(ln, &value)?, ln, "Player1")?,
                "Player2" => set_once(&mut player_ids[2], parse_uuid(ln, &value)?, ln, "Player2")?,
                "Player3" => set_once(&mut player_ids[3], parse_uuid(ln, &value)?, ln, "Player3")?,
                "Name0" => set_once(&mut names[0], value, ln, "Name0")?,
                "Name1" => set_once(&mut names[1], value, ln, "Name1")?,
                "Name2" => set_once(&mut names[2], value, ln, "Name2")?,
                "Name3" => set_once(&mut names[3], value, ln, "Name3")?,
                "TimerInitial" => set_once(&mut timer_initial, parse_u64(ln, &value)?, ln, "TimerInitial")?,
                "TimerIncrement" => set_once(&mut timer_increment, parse_u64(ln, &value)?, ln, "TimerIncrement")?,
                "Termination" => {
                    let t = match value.as_str() {
                        "Completed" => Termination::Completed,
                        "Aborted" => Termination::Aborted,
                        "InProgress" => Termination::InProgress,
                        _ => return Err(DecodeError::BadTermination { line: ln, value }),
                    };
                    if self.termination.is_some() {
                        return Err(DecodeError::DuplicateTag { line: ln, key: "Termination".into() });
                    }
                    self.termination = Some(t);
                }
                "Result" => {
                    if self.result.is_some() || self.result_was_star {
                        return Err(DecodeError::DuplicateTag { line: ln, key: "Result".into() });
                    }
                    if value == "*" {
                        self.result_was_star = true;
                    } else {
                        let (a_str, b_str) = value
                            .split_once('-')
                            .ok_or_else(|| DecodeError::BadResult { line: ln, value: value.clone() })?;
                        let a = a_str.parse::<i32>().map_err(|_| DecodeError::BadResult { line: ln, value: value.clone() })?;
                        let b = b_str.parse::<i32>().map_err(|_| DecodeError::BadResult { line: ln, value: value.clone() })?;
                        self.result = Some((a, b));
                    }
                }
                _ => return Err(DecodeError::BadTag { line: ln, found: line.to_string() }),
            }
        }

        let game_id = game_id.ok_or(DecodeError::MissingRequiredTag { key: "GameId" })?;
        let max_points = max_points.ok_or(DecodeError::MissingRequiredTag { key: "MaxPoints" })?;
        let player_ids = [
            player_ids[0].ok_or(DecodeError::MissingRequiredTag { key: "Player0" })?,
            player_ids[1].ok_or(DecodeError::MissingRequiredTag { key: "Player1" })?,
            player_ids[2].ok_or(DecodeError::MissingRequiredTag { key: "Player2" })?,
            player_ids[3].ok_or(DecodeError::MissingRequiredTag { key: "Player3" })?,
        ];
        let timer = match (timer_initial, timer_increment) {
            (Some(a), Some(b)) => Some(TimerConfig { initial_time_secs: a, increment_secs: b }),
            (None, None) => None,
            _ => {
                // Defer to ReplayError::TimerHalfSpecified rather than DecodeError —
                // but for symmetry, reject during decode too.
                return Err(DecodeError::MissingRequiredTag { key: "TimerInitial/Increment pair" });
            }
        };
        Ok(Headers { game_id, max_points, player_ids, names, timer })
    }

    fn consume_termination_and_result(&mut self) -> Result<(Termination, Option<(i32, i32)>), DecodeError> {
        let term = self.termination.ok_or(DecodeError::MissingRequiredTag { key: "Termination" })?;
        if !self.result_was_star && self.result.is_none() {
            return Err(DecodeError::MissingRequiredTag { key: "Result" });
        }
        Ok((term, self.result))
    }

    fn parse_rounds(&mut self) -> Result<Vec<Round>, DecodeError> {
        // Stub: filled in task 9.
        Ok(Vec::new())
    }

    fn expect_eof(&mut self) -> Result<(), DecodeError> {
        // Skip trailing blank lines.
        while let Some((_, l)) = self.peek() {
            if l.is_empty() {
                self.advance();
            } else {
                let (ln, _) = self.peek().copied().unwrap();
                return Err(DecodeError::TrailingContent { line: ln });
            }
        }
        Ok(())
    }
}

fn parse_tag_line(line_no: usize, line: &str) -> Result<(String, String), DecodeError> {
    // Expect: [<Key> "<Value>"]
    let inside = line
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| DecodeError::BadTag { line: line_no, found: line.to_string() })?;
    let (key, rest) = inside
        .split_once(' ')
        .ok_or_else(|| DecodeError::BadTag { line: line_no, found: line.to_string() })?;
    let value = rest
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| DecodeError::BadTag { line: line_no, found: line.to_string() })?;
    let unescaped = unescape_tag_value(value)
        .ok_or_else(|| DecodeError::BadEscape { line: line_no, value: value.to_string() })?;
    Ok((key.to_string(), unescaped))
}

fn set_once<T>(slot: &mut Option<T>, value: T, ln: usize, key: &str) -> Result<(), DecodeError> {
    if slot.is_some() {
        return Err(DecodeError::DuplicateTag { line: ln, key: key.to_string() });
    }
    *slot = Some(value);
    Ok(())
}

fn parse_uuid(ln: usize, v: &str) -> Result<Uuid, DecodeError> {
    Uuid::parse_str(v).map_err(|_| DecodeError::BadUuid { line: ln, value: v.to_string() })
}

fn parse_int(ln: usize, v: &str) -> Result<i32, DecodeError> {
    v.parse::<i32>().map_err(|_| DecodeError::BadInteger { line: ln, value: v.to_string() })
}

fn parse_u64(ln: usize, v: &str) -> Result<u64, DecodeError> {
    v.parse::<u64>().map_err(|_| DecodeError::BadInteger { line: ln, value: v.to_string() })
}

// Suppress unused-import warnings until task 9 lands.
#[allow(dead_code)]
fn _silence_warnings(_: super::Round, _: fn(&str) -> Option<crate::cards::Card>) {
    let _ = parse_card;
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn decode_minimal_header() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.headers.game_id, u(1));
        assert_eq!(t.headers.max_points, 500);
        assert_eq!(t.headers.player_ids, [u(0x0a), u(0x0b), u(0x0c), u(0x0d)]);
        assert_eq!(t.headers.names, [None, None, None, None]);
        assert!(t.headers.timer.is_none());
        assert_eq!(t.termination, Termination::InProgress);
        assert!(t.result.is_none());
    }

    #[test]
    fn decode_header_with_names_and_timer() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"300\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Name0 \"Alice\"]
[Name2 \"Carol \\\"Q\\\"\"]
[TimerInitial \"300\"]
[TimerIncrement \"5\"]
[Termination \"Completed\"]
[Result \"520-430\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.headers.names[0].as_deref(), Some("Alice"));
        assert_eq!(t.headers.names[1], None);
        assert_eq!(t.headers.names[2].as_deref(), Some("Carol \"Q\""));
        assert_eq!(t.headers.names[3], None);
        assert_eq!(t.headers.timer.map(|tc| (tc.initial_time_secs, tc.increment_secs)), Some((300, 5)));
        assert_eq!(t.termination, Termination::Completed);
        assert_eq!(t.result, Some((520, 430)));
    }

    #[test]
    fn decode_rejects_missing_required_tag() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::MissingRequiredTag { key: "Player2" })
        ));
    }

    #[test]
    fn decode_rejects_duplicate_tag() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[MaxPoints \"600\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::DuplicateTag { key, .. }) if key == "MaxPoints"
        ));
    }

    #[test]
    fn decode_rejects_bad_uuid() {
        let s = "\
[GameId \"not-a-uuid\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadUuid { .. })));
    }
}
```

- [ ] **Step 2: Run the decoder tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::decode
```

Expected: all 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/spades-core/src/transcript/decode.rs
git commit -m "transcript: header decoder"
```

---

## Task 8: Decoder — round body

**Files:**
- Modify: `crates/spades-core/src/transcript/decode.rs`

- [ ] **Step 1: Implement `parse_rounds`**

Replace the `parse_rounds` stub in `crates/spades-core/src/transcript/decode.rs` with:

```rust
    fn parse_rounds(&mut self) -> Result<Vec<Round>, DecodeError> {
        let mut rounds: Vec<Round> = Vec::new();
        while let Some((ln, line)) = self.peek().copied() {
            if line.is_empty() {
                self.advance();
                continue;
            }
            if !line.starts_with("[Round ") {
                // No more round blocks; let `expect_eof` reject anything else.
                break;
            }

            // Parse [Round "N"]
            let (key, value) = parse_tag_line(ln, line)?;
            self.advance();
            if key != "Round" {
                return Err(DecodeError::BadTag { line: ln, found: line.to_string() });
            }
            let round_num = parse_int(ln, &value)?;
            if round_num < 1 {
                return Err(DecodeError::BadInteger { line: ln, value });
            }
            let expected = rounds.len() + 1;
            if (round_num as usize) != expected {
                return Err(DecodeError::NonMonotonicRound {
                    expected,
                    found: round_num as usize,
                });
            }

            // Parse 4 [HandN "..."] lines in seat order.
            let mut hands: [Vec<crate::cards::Card>; 4] = Default::default();
            for seat in 0..4 {
                let (ln2, line2) = self.advance().copied().ok_or(DecodeError::UnexpectedEof)?;
                let (k, v) = parse_tag_line(ln2, line2)?;
                if k != format!("Hand{}", seat) {
                    return Err(DecodeError::BadTag { line: ln2, found: line2.to_string() });
                }
                for tok in v.split_whitespace() {
                    let c = parse_card(tok).ok_or(DecodeError::BadCard {
                        line: ln2,
                        token: tok.to_string(),
                    })?;
                    hands[seat].push(c);
                }
            }

            // Parse [Bets "..."]
            let (ln3, line3) = self.advance().copied().ok_or(DecodeError::UnexpectedEof)?;
            let (k3, v3) = parse_tag_line(ln3, line3)?;
            if k3 != "Bets" {
                return Err(DecodeError::BadTag { line: ln3, found: line3.to_string() });
            }
            let bets: Vec<i32> = if v3.is_empty() {
                Vec::new()
            } else {
                let mut out = Vec::new();
                for tok in v3.split_whitespace() {
                    out.push(parse_int(ln3, tok)?);
                }
                out
            };
            if bets.len() > 4 {
                return Err(DecodeError::TooManyBets { round: round_num as usize });
            }

            // Parse trick lines: "N. C1 C2 ..." until blank/next [Round]/EOF.
            let mut tricks: Vec<Vec<crate::cards::Card>> = Vec::new();
            while let Some((ln4, line4)) = self.peek().copied() {
                if line4.is_empty() || line4.starts_with('[') {
                    break;
                }
                let (num_str, rest) = line4
                    .split_once('.')
                    .ok_or_else(|| DecodeError::BadTag { line: ln4, found: line4.to_string() })?;
                let trick_num = parse_int(ln4, num_str)?;
                let expected_t = tricks.len() + 1;
                if (trick_num as usize) != expected_t {
                    return Err(DecodeError::BadTag { line: ln4, found: line4.to_string() });
                }
                let mut cards: Vec<crate::cards::Card> = Vec::new();
                for tok in rest.split_whitespace() {
                    let c = parse_card(tok).ok_or(DecodeError::BadCard {
                        line: ln4,
                        token: tok.to_string(),
                    })?;
                    cards.push(c);
                }
                if cards.is_empty() {
                    return Err(DecodeError::BadTag { line: ln4, found: line4.to_string() });
                }
                if cards.len() > 4 {
                    return Err(DecodeError::TooManyCardsInTrick {
                        round: round_num as usize,
                        trick: trick_num as usize,
                    });
                }
                self.advance();
                tricks.push(cards);
                if tricks.len() > 13 {
                    return Err(DecodeError::TooManyTricks { round: round_num as usize });
                }
            }

            rounds.push(Round { hands, bets, tricks });
        }
        Ok(rounds)
    }
```

- [ ] **Step 2: Add round-body tests**

Append to the existing `mod tests` block in `decode.rs`:

```rust
    use crate::cards::{Card, Rank, Suit};

    fn c(r: Rank, su: Suit) -> Card {
        Card { rank: r, suit: su }
    }

    #[test]
    fn decode_one_round_block() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
1. 2C 5C 7C TC
";
        let t = decode(s).unwrap();
        assert_eq!(t.rounds.len(), 1);
        let r = &t.rounds[0];
        assert_eq!(r.hands[0].len(), 13);
        assert_eq!(r.hands[0][0], c(Rank::Two, Suit::Club));
        assert_eq!(r.bets, vec![3, 4, 2, 4]);
        assert_eq!(r.tricks.len(), 1);
        assert_eq!(r.tricks[0], vec![
            c(Rank::Two, Suit::Club),
            c(Rank::Five, Suit::Club),
            c(Rank::Seven, Suit::Club),
            c(Rank::Ten, Suit::Club),
        ]);
    }

    #[test]
    fn decode_partial_bets() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4\"]
";
        let t = decode(s).unwrap();
        assert_eq!(t.rounds[0].bets, vec![3, 4]);
        assert!(t.rounds[0].tricks.is_empty());
    }

    #[test]
    fn decode_rejects_non_monotonic_round() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"3\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::NonMonotonicRound { expected: 1, found: 3 })
        ));
    }

    #[test]
    fn decode_rejects_too_many_bets() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4 1\"]
";
        assert!(matches!(decode(s), Err(DecodeError::TooManyBets { round: 1 })));
    }

    #[test]
    fn decode_rejects_too_many_cards_in_trick() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S QS\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"3 4 2 4\"]
1. 2C 5C 7C TC 8C
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::TooManyCardsInTrick { round: 1, trick: 1 })
        ));
    }

    #[test]
    fn decode_rejects_bad_card() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

[Round \"1\"]
[Hand0 \"2C 5C 7C TC AC 3D 8D KD 4H 9H 2S 6S 1X\"]
[Hand1 \"3C 6C 8C JC 4D 9D AD 5H TH JH 3S 7S KS\"]
[Hand2 \"4C 7C 9C QC 5D TD 2H 6H QH 4S 8S TS AS\"]
[Hand3 \"2C 5D JD KD 7H 8H KH 5S 9S JS QS 6D 3H\"]
[Bets \"\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadCard { token, .. }) if token == "1X"));
    }
```

- [ ] **Step 3: Run all decoder tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::decode
```

Expected: all 11 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-core/src/transcript/decode.rs
git commit -m "transcript: round-body decoder"
```

---

## Task 9: Replay implementation

**Files:**
- Modify: `crates/spades-core/src/transcript/replay.rs`

- [ ] **Step 1: Replace the stub with replay logic**

Replace the contents of `crates/spades-core/src/transcript/replay.rs` with:

```rust
use crate::cards::{get_trick_winner, Card};
use crate::{Game, GameTransition, State};

use super::{ReplayError, Round, Termination, Transcript};

pub fn replay(t: &Transcript) -> Result<Game, ReplayError> {
    // Build a fresh game with header parameters.
    let mut game = Game::new(
        t.headers.game_id,
        t.headers.player_ids,
        t.headers.max_points,
        t.headers.timer,
    );
    for seat in 0..4 {
        if let Some(name) = &t.headers.names[seat] {
            // set_player_name takes ownership of an Option<String>; pass a clone.
            let _ = game.set_player_name(t.headers.player_ids[seat], Some(name.clone()));
        }
    }

    if t.rounds.is_empty() {
        // No Start may have been issued. Honor declared termination.
        finalize(&mut game, t)?;
        return Ok(game);
    }

    // Issue Start.
    game.play(GameTransition::Start).map_err(|e| ReplayError::Transition {
        round: 0,
        trick: None,
        seat: 0,
        err: e,
    })?;

    for (r_idx, round) in t.rounds.iter().enumerate() {
        // Bets (in seat order, starting at seat 0).
        for (i, &b) in round.bets.iter().enumerate() {
            game.play(GameTransition::Bet(b)).map_err(|e| ReplayError::Transition {
                round: r_idx,
                trick: None,
                seat: i,
                err: e,
            })?;
        }

        // If bets are partial, we stop here for this round.
        if round.bets.len() < 4 {
            // The Round contract: a partial-bets round has no tricks.
            if !round.tricks.is_empty() {
                return Err(ReplayError::InconsistentBetCount {
                    round: r_idx,
                    found: round.bets.len(),
                });
            }
            // Verify declared hands match observed hands (engine just dealt).
            verify_dealt_hands(&game, round, r_idx)?;
            break;
        }

        // Verify dealt hands now that we're entering trick play.
        verify_dealt_hands(&game, round, r_idx)?;

        // Tricks. First trick of any round leads with seat 0; subsequent leads
        // are the prior trick's winner.
        let mut lead = 0usize;
        for (t_idx, trick) in round.tricks.iter().enumerate() {
            for (i, &card) in trick.iter().enumerate() {
                let seat = (lead + i) % 4;
                game.play(GameTransition::Card(card)).map_err(|e| {
                    ReplayError::Transition {
                        round: r_idx,
                        trick: Some(t_idx),
                        seat,
                        err: e,
                    }
                })?;
            }
            if trick.len() == 4 {
                // Reconstruct seat-indexed cards to compute winner.
                let mut by_seat = [Card { rank: crate::cards::Rank::Two, suit: crate::cards::Suit::Club }; 4];
                for i in 0..4 {
                    by_seat[(lead + i) % 4] = trick[i];
                }
                lead = get_trick_winner(lead, &by_seat);
            }
        }
    }

    finalize(&mut game, t)?;
    Ok(game)
}

fn verify_dealt_hands(g: &Game, round: &Round, r_idx: usize) -> Result<(), ReplayError> {
    let names = g.get_player_names();
    for seat in 0..4 {
        let pid = names[seat].0;
        let actual = g.get_hand_by_player_id(pid).map_err(|_| ReplayError::HandMismatch {
            round: r_idx,
            seat,
        })?;
        let mut a: Vec<Card> = actual.clone();
        a.sort();
        let mut d = round.hands[seat].clone();
        d.sort();
        if a != d {
            return Err(ReplayError::HandMismatch { round: r_idx, seat });
        }
    }
    Ok(())
}

fn finalize(game: &mut Game, t: &Transcript) -> Result<(), ReplayError> {
    // Apply Aborted state if declared and game isn't already terminal.
    match t.termination {
        Termination::Aborted if *game.get_state() != State::Completed => {
            game.set_state(State::Aborted);
        }
        _ => {}
    }
    // Verify declared termination matches what the engine reached.
    let actual = match game.get_state() {
        State::Completed => Termination::Completed,
        State::Aborted => Termination::Aborted,
        _ => Termination::InProgress,
    };
    if actual != t.termination {
        return Err(ReplayError::TerminationMismatch {
            declared: t.termination,
            actual,
        });
    }
    // Verify declared result if terminal.
    if let Some(declared) = t.result {
        let a = game.get_team_a_score().copied().unwrap_or(0);
        let b = game.get_team_b_score().copied().unwrap_or(0);
        if (a, b) != declared {
            return Err(ReplayError::ResultMismatch {
                declared,
                actual: (a, b),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{decode, encode};
    use uuid::Uuid;

    fn u(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn build_short_game() -> Game {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 50, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        for _ in 0..13 {
            for _ in 0..4 {
                let legal = g.get_legal_cards().unwrap();
                g.play(GameTransition::Card(legal[0])).unwrap();
            }
        }
        g
    }

    #[test]
    fn replay_one_round_round_trip() {
        let g = build_short_game();
        let encoded = encode(&g);
        let parsed = decode(&encoded).expect("decode");
        let replayed = replay(&parsed).expect("replay");

        assert_eq!(replayed.get_id(), g.get_id());
        assert_eq!(replayed.get_team_a_score().copied().unwrap_or(0), g.get_team_a_score().copied().unwrap_or(0));
        assert_eq!(replayed.get_team_b_score().copied().unwrap_or(0), g.get_team_b_score().copied().unwrap_or(0));
        assert_eq!(replayed.get_state(), g.get_state());
        let re_encoded = encode(&replayed);
        assert_eq!(encoded, re_encoded, "encoder idempotence");
    }

    #[test]
    fn replay_mid_betting() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.play(GameTransition::Bet(2)).unwrap();
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), g.get_state());
        assert_eq!(encode(&replayed), encoded);
    }

    #[test]
    fn replay_mid_trick() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        for _ in 0..4 {
            g.play(GameTransition::Bet(3)).unwrap();
        }
        // Play 2 cards into trick 1.
        for _ in 0..2 {
            let legal = g.get_legal_cards().unwrap();
            g.play(GameTransition::Card(legal[0])).unwrap();
        }
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), g.get_state());
        assert_eq!(encode(&replayed), encoded);
    }

    #[test]
    fn replay_aborted_from_betting() {
        let mut g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        g.play(GameTransition::Start).unwrap();
        g.play(GameTransition::Bet(3)).unwrap();
        g.set_state(State::Aborted);
        let encoded = encode(&g);
        let parsed = decode(&encoded).unwrap();
        let replayed = replay(&parsed).unwrap();
        assert_eq!(replayed.get_state(), &State::Aborted);
    }

    #[test]
    fn replay_rejects_termination_mismatch() {
        // Build a not-started game, then mutate the encoded text to claim Completed.
        let g = Game::new(u(1), [u(10), u(11), u(12), u(13)], 500, None);
        let encoded = encode(&g).replace("InProgress", "Completed").replace("\"*\"", "\"100-50\"");
        let parsed = decode(&encoded).unwrap();
        assert!(matches!(
            replay(&parsed),
            Err(ReplayError::TerminationMismatch { .. })
        ));
    }

    #[test]
    fn replay_rejects_illegal_card() {
        // Take a real encoded game, swap a Played card for one the player didn't hold.
        let g = build_short_game();
        let encoded = encode(&g);
        // Find a "1. " trick line in round 1 and replace the first card with "2C 2C 2C 2C" — guaranteed illegal duplicate.
        let mutated = encoded.replacen("1. ", "1. 2C 2C 2C 2C\nXX. ", 1);
        // Cleaner: just inject an obviously invalid card sequence.
        let _ = mutated;
        // Instead of complex string surgery, build a transcript directly.
        let mut parsed = decode(&encoded).unwrap();
        // Force the first card of round 0 trick 0 to AS regardless.
        parsed.rounds[0].tricks[0][0] = Card { rank: crate::cards::Rank::Ace, suit: crate::cards::Suit::Spade };
        match replay(&parsed) {
            Err(ReplayError::Transition { .. }) | Err(ReplayError::HandMismatch { .. }) => {}
            other => panic!("expected replay error, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Run the replay tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::replay
```

Expected: all 6 replay tests pass. (If `replay_mid_trick` or `replay_one_round_round_trip` fails because the encoder mis-handles the partial trick lead seat, fix the encoder helper `tricks_for_round` so the lead seat advance only happens when `count == 4`. The code as written in task 6 already does this.)

- [ ] **Step 3: Commit**

```bash
git add crates/spades-core/src/transcript/replay.rs
git commit -m "transcript: replay drives engine + verifies termination/result"
```

---

## Task 10: Round-trip property test

**Files:**
- Modify: `crates/spades-core/src/transcript/mod.rs`

- [ ] **Step 1: Add a property test that plays many random games**

Append to `crates/spades-core/src/transcript/mod.rs` (after the type definitions, anywhere outside other functions):

```rust
#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::{Game, GameTransition, State};
    use rand::seq::SliceRandom;
    use rand::{RngCore, SeedableRng};
    use rand::rngs::StdRng;
    use uuid::Uuid;

    fn play_full_random_game(seed: u64) -> Game {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut g = Game::new(
            Uuid::from_u64_pair(seed, !seed),
            [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
            // small max_points so games end quickly
            60,
            None,
        );
        g.play(GameTransition::Start).unwrap();
        loop {
            match g.get_state().clone() {
                State::Completed | State::Aborted => return g,
                State::Betting(_) => {
                    // Bet a small random number to vary games.
                    let b = (rng.next_u32() % 4) as i32 + 1;
                    g.play(GameTransition::Bet(b)).unwrap();
                }
                State::Trick(_) => {
                    let legal = g.get_legal_cards().unwrap();
                    let card = *legal.choose(&mut rng).unwrap();
                    g.play(GameTransition::Card(card)).unwrap();
                }
                State::NotStarted => unreachable!(),
            }
        }
    }

    #[test]
    fn round_trip_is_idempotent_on_many_random_games() {
        for seed in 0..30u64 {
            let g = play_full_random_game(seed);
            let s1 = encode(&g);
            let parsed = decode(&s1).expect("decode");
            let replayed = replay(&parsed).expect("replay");
            let s2 = encode(&replayed);
            assert_eq!(s1, s2, "round trip differed for seed {}", seed);
        }
    }
}
```

If `Uuid::from_u64_pair` doesn't exist in the installed uuid version, swap for:
```rust
let mut bytes = [0u8; 16];
bytes[..8].copy_from_slice(&seed.to_be_bytes());
bytes[8..].copy_from_slice(&(!seed).to_be_bytes());
let game_id = Uuid::from_bytes(bytes);
```

- [ ] **Step 2: Run the property test**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript::property_tests -- --nocapture
```

Expected: all 30 random games round-trip without divergence.

- [ ] **Step 3: Commit**

```bash
git add crates/spades-core/src/transcript/mod.rs
git commit -m "transcript: round-trip property test over 30 random games"
```

---

## Task 11: Trailing-content and remaining decoder error coverage

**Files:**
- Modify: `crates/spades-core/src/transcript/decode.rs`

- [ ] **Step 1: Add the last missing decoder tests**

Append to the `mod tests` block in `decode.rs`:

```rust
    #[test]
    fn decode_rejects_bad_termination() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"Forfeit\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadTermination { .. })));
    }

    #[test]
    fn decode_rejects_bad_result() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"Completed\"]
[Result \"100/50\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadResult { .. })));
    }

    #[test]
    fn decode_rejects_unknown_tag() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[Mystery \"x\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadTag { .. })));
    }

    #[test]
    fn decode_rejects_trailing_content() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Termination \"InProgress\"]
[Result \"*\"]

garbage
";
        assert!(matches!(decode(s), Err(DecodeError::TrailingContent { .. })));
    }

    #[test]
    fn decode_rejects_bad_escape() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[Name0 \"A\\nB\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(decode(s), Err(DecodeError::BadEscape { .. })));
    }

    #[test]
    fn decode_rejects_timer_half_specified_at_parse_time() {
        let s = "\
[GameId \"01010101-0101-0101-0101-010101010101\"]
[MaxPoints \"500\"]
[Player0 \"0a0a0a0a-0a0a-0a0a-0a0a-0a0a0a0a0a0a\"]
[Player1 \"0b0b0b0b-0b0b-0b0b-0b0b-0b0b0b0b0b0b\"]
[Player2 \"0c0c0c0c-0c0c-0c0c-0c0c-0c0c0c0c0c0c\"]
[Player3 \"0d0d0d0d-0d0d-0d0d-0d0d-0d0d0d0d0d0d\"]
[TimerInitial \"300\"]
[Termination \"InProgress\"]
[Result \"*\"]
";
        assert!(matches!(
            decode(s),
            Err(DecodeError::MissingRequiredTag { key }) if key == "TimerInitial/Increment pair"
        ));
    }
```

- [ ] **Step 2: Run all transcript tests**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades transcript
```

Expected: every transcript test (format, encode, decode, replay, property) passes.

- [ ] **Step 3: Final sweep over the whole crate**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test -p spades && cargo test --workspace
```

Expected: workspace-wide green.

- [ ] **Step 4: Commit**

```bash
git add crates/spades-core/src/transcript/decode.rs
git commit -m "transcript: exhaustive decode error coverage"
```

---

## Self-review notes

- **Spec coverage:** every section of the design doc maps to a task. Headers + tag-value escaping → tasks 5 + 4. Round body (hands/bets/tricks) → tasks 6 + 8. Mid-game / Aborted → tasks 6, 7, 9. Replay → task 9. Round-trip property → task 10. Decode error variants → tasks 7, 8, 11. Engine prereq (history retention + accessors) → task 1.
- **Placeholder scan:** every code block is complete; no "implement later" except the `parse_rounds` stub in task 7 which is filled in task 8 and the `encode_rounds` stub in task 5 filled in task 6 — both serve as TDD scaffolds.
- **Type consistency:** `Headers`, `Round`, `Transcript`, `Termination`, `DecodeError`, `ReplayError` defined once in task 2; all later tasks reference identical signatures. `get_max_points`, `get_history`, `get_all_bets`, `get_round_index`, `get_in_betting_stage` accessor names are used consistently from task 1 through task 11.
