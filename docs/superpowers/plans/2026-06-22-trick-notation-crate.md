# trick-notation Crate (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `trick-notation` crate — a game-agnostic model + canonical text + JSON serialization for trick-taking games — and migrate spades-core's STF transcript onto it without changing observable replay behavior.

**Architecture:** One pure, rule-agnostic in-memory `Model { meta, deck, events }` where a deal is itself an event (games re-deal each round). The crate serializes the model two ways in Phase 1: canonical text (PGN/PBN-inspired, dot-grouped holdings) and JSON (serde). spades-core's `transcript` module becomes a thin adapter that converts `Game ↔ Model`, so the existing replay endpoint keeps working while STF retires into the canonical format.

**Tech Stack:** Rust (edition 2024), `serde`/`serde_json`, `thiserror`. No async deps (mirrors spades-core). Tests with the std test harness + `ntest` (already a spades-core dev-dep) where timeouts help.

## Global Constraints

- Edition: `2024`; workspace `version = "3.0.0"` (inherit via `version.workspace = true`).
- Workspace deps must be reused: `serde = { workspace = true }`, `serde_json = { workspace = true }`. Add `thiserror = "2"` (matches spades-core's pin).
- `spades` (spades-core) is published to crates.io; `spades::transcript` is **public API**. We break it within the unreleased 3.0.0 line only — keep the same function names (`encode`, `decode`, `replay`) and `Transcript`/error re-exports so downstream call sites compile.
- No raw game rules in the notation crate: it records events, never derived state (no trick winner, no score). The one exception consumers may rely on: each `Play` event carries its `leader` seat explicitly.
- Phase 1 deck scope: single-character suit and rank symbols; presets `french52`, `euchre24`; inline `suits=…/ranks=…`. Capabilities `exchange` and `piles`. **Deferred to later plans:** `multiplicity`, `specials` (jokers/Fool), `per-suit-ranks` (tarot), the compact binary codec, the server `replay.json` endpoint, and the web viewer.
- Run all Rust commands with `~/.cargo/bin` on PATH (e.g. `export PATH="$HOME/.cargo/bin:$PATH"` once per shell).
- Commit with pathspec (`git commit -- <paths>`); the repo may carry unrelated staged WIP.

---

## File Structure

- `crates/trick-notation/Cargo.toml` — new crate manifest (lib name `trick_notation`).
- `crates/trick-notation/src/lib.rs` — module wiring + crate docs + public re-exports.
- `crates/trick-notation/src/card.rs` — `Card`, `Sym`, card-token parse/format.
- `crates/trick-notation/src/deck.rs` — `Deck`, presets, inline parse, canonical card enumeration.
- `crates/trick-notation/src/model.rs` — `Meta`, `Event`, `Model`, `Target`.
- `crates/trick-notation/src/text/encode.rs` — `to_text(&Model) -> String`.
- `crates/trick-notation/src/text/decode.rs` — `from_text(&str) -> Result<Model, ParseError>`, `ParseError`.
- `crates/trick-notation/src/text/mod.rs` — text module wiring + holdings helpers shared by encode/decode.
- `crates/trick-notation/tests/round_trip.rs` — text round-trip + fuzz.
- `crates/trick-notation/tests/conformance.rs` — hearts/euchre golden fixtures.
- `crates/spades-core/src/transcript/` — rewired as an adapter over `trick_notation` (Tasks 7).
- Modify: root `Cargo.toml` (workspace members), `crates/spades-core/Cargo.toml` (add dep), `coverage-baseline.json` (new crate key).

---

### Task 1: Crate scaffold + `Card` model + card-token codec

**Files:**
- Modify: `Cargo.toml` (workspace `members`)
- Create: `crates/trick-notation/Cargo.toml`
- Create: `crates/trick-notation/src/lib.rs`
- Create: `crates/trick-notation/src/card.rs`

**Interfaces:**
- Produces:
  - `pub type Sym = String;`
  - `pub enum Card { Suited { suit: Sym, rank: Sym }, Special { name: Sym } }` (derives `Clone, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize`)
  - `pub fn format_card(c: &Card) -> String` — `Suited{rank:"K",suit:"C"}` → `"KC"`; `Special{name:"Fool"}` → `"*Fool"`.
  - `pub fn parse_card(tok: &str) -> Option<Card>` — inverse of `format_card` for single-char rank+suit and `*name` specials.

- [ ] **Step 1: Add the crate to the workspace**

Edit `Cargo.toml`:

```toml
[workspace]
members = ["crates/spades-core", "crates/spades-server", "crates/trick-notation"]
resolver = "3"
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/trick-notation/Cargo.toml`:

```toml
[package]
name = "trick-notation"
version.workspace = true
edition.workspace = true
authors.workspace = true
repository.workspace = true
license.workspace = true
description = "Game-agnostic notation for trick-taking card games (model, canonical text, JSON)."
categories = ["games", "game-engines", "encoding"]
keywords = ["cards", "trick-taking", "notation", "pgn", "serialization"]

[lib]
name = "trick_notation"
path = "src/lib.rs"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = "2"
```

- [ ] **Step 3: Create the lib root**

Create `crates/trick-notation/src/lib.rs`:

```rust
//! Game-agnostic notation for trick-taking card games.
//!
//! One in-memory [`Model`] serializes to canonical text and JSON. The model is
//! rule-agnostic: it records observed events (deals, calls, plays, exchanges),
//! never rule-derived facts like trick winners or scores. The leader seat of
//! every trick is recorded explicitly so a generic reader can lay out a game
//! without knowing any rules.

mod card;
mod deck;
mod model;
mod text;

pub use card::{Card, Sym, format_card, parse_card};
pub use deck::Deck;
pub use model::{Event, Meta, Model, Target};
pub use text::{ParseError, from_text, to_text};
```

(Note: `deck`, `model`, `text` modules are created in later tasks; this file will not compile until Task 3. That is expected — Task 1's tests are `card`-only and run via `cargo test -p trick-notation --lib card::` after Task 3 wires modules. To keep Task 1 independently testable, temporarily comment the `mod deck; mod model; mod text;` lines and their re-exports, then restore them in Task 3. The step below does this.)

Replace the module block with the Task-1-only version:

```rust
mod card;
pub use card::{Card, Sym, format_card, parse_card};
```

- [ ] **Step 4: Write the failing test for the card codec**

Create `crates/trick-notation/src/card.rs`:

```rust
//! A card is identity-only: a suited card (rank within a suit) or a named
//! special (joker, the Fool). No rank ordering is implied — ordering is a
//! per-game rule, not a property of the card.

use serde::{Deserialize, Serialize};

/// A short symbol for a suit, rank, seat, or special. Usually one character.
pub type Sym = String;

#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Card {
    Suited { suit: Sym, rank: Sym },
    Special { name: Sym },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suited_card_round_trips() {
        let c = Card::Suited { suit: "C".into(), rank: "K".into() };
        assert_eq!(format_card(&c), "KC");
        assert_eq!(parse_card("KC"), Some(c));
    }

    #[test]
    fn special_card_round_trips() {
        let c = Card::Special { name: "Fool".into() };
        assert_eq!(format_card(&c), "*Fool");
        assert_eq!(parse_card("*Fool"), Some(c));
    }

    #[test]
    fn parse_rejects_bad_tokens() {
        assert_eq!(parse_card(""), None);
        assert_eq!(parse_card("K"), None);
        assert_eq!(parse_card("*"), None);
    }
}
```

- [ ] **Step 5: Run the test to verify it fails**

Run: `cargo test -p trick-notation --lib`
Expected: FAIL — `cannot find function 'format_card'`.

- [ ] **Step 6: Implement the card codec**

Add to `crates/trick-notation/src/card.rs` (above the `tests` module):

```rust
/// Render a card as a canonical token: `rank` then `suit` for suited cards
/// (`KC`), or `*name` for specials (`*Fool`).
pub fn format_card(c: &Card) -> String {
    match c {
        Card::Suited { suit, rank } => format!("{rank}{suit}"),
        Card::Special { name } => format!("*{name}"),
    }
}

/// Parse a canonical card token. Phase 1 accepts single-character rank+suit
/// (`KC`) and `*name` specials. Returns `None` on anything else.
pub fn parse_card(tok: &str) -> Option<Card> {
    if let Some(name) = tok.strip_prefix('*') {
        if name.is_empty() {
            return None;
        }
        return Some(Card::Special { name: name.to_string() });
    }
    let mut chars = tok.chars();
    let rank = chars.next()?;
    let suit = chars.next()?;
    if chars.next().is_some() {
        return None; // more than two chars and not a special
    }
    Some(Card::Suited { suit: suit.to_string(), rank: rank.to_string() })
}
```

- [ ] **Step 7: Run the test to verify it passes**

Run: `cargo test -p trick-notation --lib`
Expected: PASS (3 tests).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/trick-notation/Cargo.toml crates/trick-notation/src/lib.rs crates/trick-notation/src/card.rs
git commit -m "feat(trick-notation): crate scaffold + card token codec"
```

---

### Task 2: `Deck` model, presets, inline parse, canonical enumeration

**Files:**
- Create: `crates/trick-notation/src/deck.rs`
- Modify: `crates/trick-notation/src/lib.rs` (add `mod deck; pub use deck::Deck;`)

**Interfaces:**
- Consumes: `Card`, `Sym` from Task 1.
- Produces:
  - `pub struct Deck { pub suits: Vec<Sym>, pub ranks: Vec<Sym> }` (derives `Clone, PartialEq, Eq, Debug, Serialize, Deserialize`)
  - `pub fn french52() -> Deck`, `pub fn euchre24() -> Deck` (associated fns on `Deck`)
  - `pub fn Deck::preset(name: &str) -> Option<Deck>` — `"french52"`/`"euchre24"`.
  - `pub fn Deck::parse_decl(s: &str) -> Option<Deck>` — preset name OR `"suits=SHDC ranks=23456789TJQKA"`.
  - `pub fn Deck::decl_string(&self) -> String` — emits `"french52"` when it matches a preset, else `"suits=… ranks=…"`.
  - `pub fn Deck::cards(&self) -> Vec<Card>` — canonical enumeration: outer loop suits in declared order, inner loop ranks in declared order.

- [ ] **Step 1: Wire the module + write the failing test**

Add to `crates/trick-notation/src/lib.rs` (restore the deck line):

```rust
mod deck;
pub use deck::Deck;
```

Create `crates/trick-notation/src/deck.rs`:

```rust
//! A self-describing deck: declared suits and ranks (single-char symbols in
//! Phase 1). A generic parser needs no built-in deck knowledge.

use serde::{Deserialize, Serialize};

use crate::card::{Card, Sym};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Deck {
    pub suits: Vec<Sym>,
    pub ranks: Vec<Sym>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french52_has_52_cards_in_canonical_order() {
        let d = Deck::french52();
        let cards = d.cards();
        assert_eq!(cards.len(), 52);
        assert_eq!(cards[0], Card::Suited { suit: "S".into(), rank: "2".into() });
        assert_eq!(cards[51], Card::Suited { suit: "C".into(), rank: "A".into() });
    }

    #[test]
    fn euchre24_has_24_cards() {
        assert_eq!(Deck::euchre24().cards().len(), 24);
    }

    #[test]
    fn preset_lookup_and_decl_string_round_trip() {
        let d = Deck::preset("french52").unwrap();
        assert_eq!(d, Deck::french52());
        assert_eq!(d.decl_string(), "french52");
    }

    #[test]
    fn inline_decl_parses_and_emits() {
        let d = Deck::parse_decl("suits=SHDC ranks=9TJQKA").unwrap();
        assert_eq!(d, Deck::euchre24());
        // euchre24 matches a preset, so decl_string prefers the preset name.
        assert_eq!(d.decl_string(), "euchre24");
    }

    #[test]
    fn inline_decl_emits_when_no_preset_matches() {
        let d = Deck::parse_decl("suits=AB ranks=12").unwrap();
        assert_eq!(d.decl_string(), "suits=AB ranks=12");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p trick-notation --lib deck::`
Expected: FAIL — `no function 'french52'`.

- [ ] **Step 3: Implement the deck**

Add to `crates/trick-notation/src/deck.rs` (above `tests`):

```rust
fn chars_to_syms(s: &str) -> Vec<Sym> {
    s.chars().map(|c| c.to_string()).collect()
}

impl Deck {
    pub fn french52() -> Deck {
        Deck { suits: chars_to_syms("SHDC"), ranks: chars_to_syms("23456789TJQKA") }
    }

    pub fn euchre24() -> Deck {
        Deck { suits: chars_to_syms("SHDC"), ranks: chars_to_syms("9TJQKA") }
    }

    pub fn preset(name: &str) -> Option<Deck> {
        match name {
            "french52" => Some(Deck::french52()),
            "euchre24" => Some(Deck::euchre24()),
            _ => None,
        }
    }

    /// Parse a `[Deck "…"]` value: a preset name, or `suits=… ranks=…`.
    pub fn parse_decl(s: &str) -> Option<Deck> {
        let s = s.trim();
        if let Some(d) = Deck::preset(s) {
            return Some(d);
        }
        let mut suits = None;
        let mut ranks = None;
        for field in s.split_whitespace() {
            let (key, val) = field.split_once('=')?;
            match key {
                "suits" => suits = Some(chars_to_syms(val)),
                "ranks" => ranks = Some(chars_to_syms(val)),
                _ => return None,
            }
        }
        Some(Deck { suits: suits?, ranks: ranks? })
    }

    /// Emit the value for a `[Deck "…"]` header: a preset name if one matches,
    /// otherwise an inline `suits=… ranks=…` declaration.
    pub fn decl_string(&self) -> String {
        for name in ["french52", "euchre24"] {
            if Deck::preset(name).as_ref() == Some(self) {
                return name.to_string();
            }
        }
        let suits: String = self.suits.concat();
        let ranks: String = self.ranks.concat();
        format!("suits={suits} ranks={ranks}")
    }

    /// Canonical card enumeration: suits in declared order, ranks within each.
    pub fn cards(&self) -> Vec<Card> {
        let mut out = Vec::with_capacity(self.suits.len() * self.ranks.len());
        for suit in &self.suits {
            for rank in &self.ranks {
                out.push(Card::Suited { suit: suit.clone(), rank: rank.clone() });
            }
        }
        out
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p trick-notation --lib deck::`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-notation/src/lib.rs crates/trick-notation/src/deck.rs
git commit -m "feat(trick-notation): self-describing deck + presets"
```

---

### Task 3: `Meta`, `Event`, `Model` types + JSON projection

**Files:**
- Create: `crates/trick-notation/src/model.rs`
- Modify: `crates/trick-notation/src/lib.rs` (restore `mod model;`/`mod text;` + re-exports)

**Interfaces:**
- Consumes: `Card`, `Sym`, `Deck`.
- Produces:
  - `pub type Target = String;` (a seat sym, or `@pile`)
  - `pub struct Meta { pub version: u8, pub game_hint: Option<Sym>, pub seats: Vec<Sym>, pub dealer: Option<Sym>, pub players: Vec<Option<String>>, pub partnerships: Option<Vec<Vec<Sym>>>, pub caps: Vec<Sym>, pub extra: Vec<(String, String)> }`
  - `pub enum Event { Deal { hands: Vec<(Target, Vec<Card>)> }, Call { start: Sym, values: Vec<Sym> }, Play { leader: Sym, cards: Vec<Card> }, Exchange { from: Sym, to: Sym, cards: Vec<Card> }, Reveal { target: Target, cards: Vec<Card> } }`
  - `pub struct Model { pub meta: Meta, pub deck: Deck, pub events: Vec<Event> }`
  - All derive `Clone, PartialEq, Eq, Debug, Serialize, Deserialize`.

Note: `extra` carries game-specific config tags (e.g. spades `MaxPoints`) as the open/custom tag namespace, keeping the core rule-agnostic while letting adapters round-trip.

- [ ] **Step 1: Restore module wiring + write the failing JSON test**

Set `crates/trick-notation/src/lib.rs` module block to the full version (from Task 1 Step 3, all four `mod`s and re-exports).

Create `crates/trick-notation/src/model.rs`:

```rust
//! The pure, rule-agnostic model: metadata, a self-describing deck, and an
//! ordered event stream. A deal is an event (games re-deal every round).

use serde::{Deserialize, Serialize};

use crate::card::{Card, Sym};
use crate::deck::Deck;

/// A deal target: a seat symbol, or a named pile written `@kitty`.
pub type Target = String;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub version: u8,
    pub game_hint: Option<Sym>,
    pub seats: Vec<Sym>,
    pub dealer: Option<Sym>,
    pub players: Vec<Option<String>>,
    pub partnerships: Option<Vec<Vec<Sym>>>,
    pub caps: Vec<Sym>,
    /// Open tag namespace for game-specific config (e.g. spades MaxPoints).
    pub extra: Vec<(String, String)>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Event {
    Deal { hands: Vec<(Target, Vec<Card>)> },
    Call { start: Sym, values: Vec<Sym> },
    Play { leader: Sym, cards: Vec<Card> },
    Exchange { from: Sym, to: Sym, cards: Vec<Card> },
    Reveal { target: Target, cards: Vec<Card> },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Model {
    pub meta: Meta,
    pub deck: Deck,
    pub events: Vec<Event>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;

    fn sample() -> Model {
        Model {
            meta: Meta {
                version: 1,
                game_hint: Some("spades".into()),
                seats: vec!["N".into(), "E".into(), "S".into(), "W".into()],
                dealer: Some("N".into()),
                players: vec![Some("Ann".into()), None, None, None],
                partnerships: None,
                caps: vec![],
                extra: vec![("MaxPoints".into(), "250".into())],
            },
            deck: Deck::french52(),
            events: vec![
                Event::Call { start: "E".into(), values: vec!["3".into(), "4".into(), "nil".into(), "4".into()] },
                Event::Play {
                    leader: "E".into(),
                    cards: vec![
                        Card::Suited { suit: "C".into(), rank: "K".into() },
                        Card::Suited { suit: "C".into(), rank: "5".into() },
                    ],
                },
            ],
        }
    }

    #[test]
    fn model_json_round_trips() {
        let m = sample();
        let json = serde_json::to_string(&m).unwrap();
        let back: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails, then passes**

Run: `cargo test -p trick-notation --lib model::`
Expected: FAIL to compile first if `text` module is missing. Create a stub so the crate compiles:

Create `crates/trick-notation/src/text/mod.rs`:

```rust
mod decode;
mod encode;

pub use decode::{ParseError, from_text};
pub use encode::to_text;
```

Create empty-ish `crates/trick-notation/src/text/encode.rs`:

```rust
use crate::model::Model;

/// Serialize a model to canonical text. Implemented in Task 4.
pub fn to_text(_model: &Model) -> String {
    String::new()
}
```

Create `crates/trick-notation/src/text/decode.rs`:

```rust
use crate::model::Model;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("placeholder")]
    Placeholder,
}

/// Parse canonical text into a model. Implemented in Task 5.
pub fn from_text(_text: &str) -> Result<Model, ParseError> {
    Err(ParseError::Placeholder)
}
```

Re-run: `cargo test -p trick-notation --lib model::`
Expected: PASS (1 test). (The text stubs exist only to compile; Tasks 4–5 replace them.)

- [ ] **Step 3: Commit**

```bash
git add crates/trick-notation/src/lib.rs crates/trick-notation/src/model.rs crates/trick-notation/src/text/
git commit -m "feat(trick-notation): model types + JSON projection"
```

---

### Task 4: Canonical text encode

**Files:**
- Modify: `crates/trick-notation/src/text/encode.rs`
- Modify: `crates/trick-notation/src/text/mod.rs` (add shared `holdings` helper)

**Interfaces:**
- Consumes: `Model`, `Event`, `Deck`, `Card`, `format_card`.
- Produces: `pub fn to_text(model: &Model) -> String` (replaces the Task 3 stub).
- Produces (in `text/mod.rs`): `pub(crate) fn format_holdings(cards: &[Card], deck: &Deck) -> String` — dot-grouped by `deck.suits` order; void suit emits `-`.

- [ ] **Step 1: Write the failing test**

Add to `crates/trick-notation/src/text/encode.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;
    use crate::deck::Deck;
    use crate::model::{Event, Meta, Model};

    fn card(rank: &str, suit: &str) -> Card {
        Card::Suited { suit: suit.into(), rank: rank.into() }
    }

    #[test]
    fn encodes_headers_and_events() {
        let m = Model {
            meta: Meta {
                version: 1,
                game_hint: Some("spades".into()),
                seats: vec!["N".into(), "E".into(), "S".into(), "W".into()],
                dealer: Some("N".into()),
                players: vec![Some("Ann".into()), Some("Bo".into()), Some("Cy".into()), Some("Di".into())],
                partnerships: None,
                caps: vec![],
                extra: vec![("MaxPoints".into(), "250".into())],
            },
            deck: Deck::french52(),
            events: vec![
                Event::Deal {
                    hands: vec![("N".into(), vec![card("A", "S"), card("K", "S"), card("T", "H")])],
                },
                Event::Call { start: "E".into(), values: vec!["3".into(), "4".into(), "nil".into(), "4".into()] },
                Event::Play { leader: "E".into(), cards: vec![card("K", "C"), card("5", "C")] },
            ],
        };
        let text = to_text(&m);
        assert!(text.starts_with("% trick-notation v1\n"), "got:\n{text}");
        assert!(text.contains(r#"[Game "spades"]"#), "{text}");
        assert!(text.contains(r#"[Deck "french52"]"#), "{text}");
        assert!(text.contains(r#"[Seats "N E S W"]"#), "{text}");
        assert!(text.contains(r#"[MaxPoints "250"]"#), "{text}");
        // dot-grouped holding: spades AK, hearts T, diamonds void, clubs void
        assert!(text.contains("D N:AK.T.-.-"), "{text}");
        assert!(text.contains("C E: 3 4 nil 4"), "{text}");
        assert!(text.contains("P E KC 5C"), "{text}");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p trick-notation --lib text::encode`
Expected: FAIL — assertion on `starts_with` (stub returns empty).

- [ ] **Step 3: Implement holdings helper**

Replace `crates/trick-notation/src/text/mod.rs` with:

```rust
mod decode;
mod encode;

pub use decode::{ParseError, from_text};
pub use encode::to_text;

use crate::card::Card;
use crate::deck::Deck;

/// Dot-grouped holdings (PBN-style): one group per deck suit in declared order,
/// ranks concatenated within a group; a void suit is written `-`.
pub(crate) fn format_holdings(cards: &[Card], deck: &Deck) -> String {
    let mut groups: Vec<String> = Vec::with_capacity(deck.suits.len());
    for suit in &deck.suits {
        let mut ranks = String::new();
        // Emit ranks in deck rank order for stable output.
        for rank in &deck.ranks {
            let present = cards.iter().any(|c| {
                matches!(c, Card::Suited { suit: s, rank: r } if s == suit && r == rank)
            });
            if present {
                ranks.push_str(rank);
            }
        }
        groups.push(if ranks.is_empty() { "-".to_string() } else { ranks });
    }
    groups.join(".")
}
```

- [ ] **Step 4: Implement encode**

Replace `crates/trick-notation/src/text/encode.rs` (keep the `tests` module):

```rust
use std::fmt::Write as _;

use crate::card::format_card;
use crate::model::{Event, Model};

use super::format_holdings;

/// Serialize a model to canonical trick-notation text. Deterministic.
pub fn to_text(model: &Model) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("% trick-notation v1\n");

    let m = &model.meta;
    if let Some(g) = &m.game_hint {
        let _ = writeln!(out, r#"[Game "{g}"]"#);
    }
    let _ = writeln!(out, r#"[Deck "{}"]"#, model.deck.decl_string());
    let _ = writeln!(out, r#"[Seats "{}"]"#, m.seats.join(" "));
    if let Some(d) = &m.dealer {
        let _ = writeln!(out, r#"[Dealer "{d}"]"#);
    }
    if m.players.iter().any(|p| p.is_some()) {
        let names: Vec<&str> = m.players.iter().map(|p| p.as_deref().unwrap_or("?")).collect();
        let _ = writeln!(out, r#"[Players "{}"]"#, names.join(" "));
    }
    if let Some(parts) = &m.partnerships {
        let groups: Vec<String> = parts.iter().map(|g| g.join("")).collect();
        let _ = writeln!(out, r#"[Partnerships "{}"]"#, groups.join(" "));
    }
    if !m.caps.is_empty() {
        let _ = writeln!(out, r#"[Caps "{}"]"#, m.caps.join(" "));
    }
    for (k, v) in &m.extra {
        let _ = writeln!(out, r#"[{k} "{v}"]"#);
    }
    out.push('\n');

    for event in &model.events {
        match event {
            Event::Deal { hands } => {
                out.push('D');
                for (target, cards) in hands {
                    let _ = write!(out, " {target}:{}", format_holdings(cards, &model.deck));
                }
                out.push('\n');
            }
            Event::Call { start, values } => {
                let _ = writeln!(out, "C {start}: {}", values.join(" "));
            }
            Event::Play { leader, cards } => {
                let toks: Vec<String> = cards.iter().map(format_card).collect();
                let _ = writeln!(out, "P {leader} {}", toks.join(" "));
            }
            Event::Exchange { from, to, cards } => {
                let toks: Vec<String> = cards.iter().map(format_card).collect();
                let _ = writeln!(out, "X {from}>{to}: {}", toks.join(" "));
            }
            Event::Reveal { target, cards } => {
                let toks: Vec<String> = cards.iter().map(format_card).collect();
                let _ = writeln!(out, "U {target}:{}", toks.join(" "));
            }
        }
    }
    out
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p trick-notation --lib text::encode`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/trick-notation/src/text/encode.rs crates/trick-notation/src/text/mod.rs
git commit -m "feat(trick-notation): canonical text encode"
```

---

### Task 5: Canonical text decode

**Files:**
- Modify: `crates/trick-notation/src/text/decode.rs`
- Modify: `crates/trick-notation/src/text/mod.rs` (add `parse_holdings` helper)

**Interfaces:**
- Consumes: `Model`, `Meta`, `Event`, `Deck`, `Card`, `parse_card`, `format_holdings` counterpart.
- Produces: `pub fn from_text(text: &str) -> Result<Model, ParseError>` and an expanded `ParseError`.
- Produces (in `text/mod.rs`): `pub(crate) fn parse_holdings(s: &str, deck: &Deck) -> Option<Vec<Card>>`.

- [ ] **Step 1: Add the holdings parser + write the failing test**

Add to `crates/trick-notation/src/text/mod.rs`:

```rust
/// Inverse of [`format_holdings`]: `AK.T.-.-` → the cards, using `deck.suits`
/// order to assign each dot-group its suit.
pub(crate) fn parse_holdings(s: &str, deck: &Deck) -> Option<Vec<Card>> {
    let groups: Vec<&str> = s.split('.').collect();
    if groups.len() != deck.suits.len() {
        return None;
    }
    let mut cards = Vec::new();
    for (suit, group) in deck.suits.iter().zip(groups) {
        if group == "-" {
            continue;
        }
        for ch in group.chars() {
            cards.push(Card::Suited { suit: suit.clone(), rank: ch.to_string() });
        }
    }
    Some(cards)
}
```

Replace `crates/trick-notation/src/text/decode.rs`:

```rust
use crate::card::{Card, parse_card};
use crate::deck::Deck;
use crate::model::{Event, Meta, Model};

use super::parse_holdings;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("missing version marker '% trick-notation v1' on line 1")]
    MissingVersion,
    #[error("malformed header on line {line}: {text:?}")]
    BadHeader { line: usize, text: String },
    #[error("missing required header {key:?}")]
    MissingHeader { key: &'static str },
    #[error("unknown deck declaration {decl:?} on line {line}")]
    BadDeck { line: usize, decl: String },
    #[error("malformed event on line {line}: {text:?}")]
    BadEvent { line: usize, text: String },
    #[error("invalid card token {token:?} on line {line}")]
    BadCard { line: usize, token: String },
    #[error("invalid holdings {holding:?} on line {line}")]
    BadHoldings { line: usize, holding: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::to_text;
    use crate::model::Meta;

    #[test]
    fn round_trips_a_small_game() {
        let text = "\
% trick-notation v1
[Game \"spades\"]
[Deck \"french52\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Players \"Ann Bo Cy Di\"]
[MaxPoints \"250\"]

D N:AK.T.-.-
C E: 3 4 nil 4
P E KC 5C
";
        let model = from_text(text).expect("parse");
        assert_eq!(model.meta.game_hint.as_deref(), Some("spades"));
        assert_eq!(model.meta.seats, vec!["N", "E", "S", "W"]);
        assert_eq!(model.meta.extra, vec![("MaxPoints".to_string(), "250".to_string())]);
        assert_eq!(model.events.len(), 3);
        // Re-encoding the parsed model reproduces the input exactly.
        assert_eq!(to_text(&model), text);
    }

    #[test]
    fn rejects_missing_version() {
        assert_eq!(from_text("[Game \"x\"]\n"), Err(ParseError::MissingVersion));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p trick-notation --lib text::decode`
Expected: FAIL — stub returns `Placeholder`.

- [ ] **Step 3: Implement decode**

Add to `crates/trick-notation/src/text/decode.rs` (above the `tests` module), replacing the stub `from_text`:

```rust
/// Parse a header line `[Key "value"]`. Returns `(key, value)`.
fn parse_header(line: &str) -> Option<(String, String)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (key, rest) = inner.split_once(' ')?;
    let value = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some((key.to_string(), value.to_string()))
}

const KNOWN_HEADERS: &[&str] =
    &["Game", "Deck", "Seats", "Dealer", "Players", "Partnerships", "Caps"];

pub fn from_text(text: &str) -> Result<Model, ParseError> {
    let mut lines = text.lines().enumerate();

    // Line 1: version marker.
    match lines.next() {
        Some((_, l)) if l.trim() == "% trick-notation v1" => {}
        _ => return Err(ParseError::MissingVersion),
    }

    let mut meta = Meta {
        version: 1,
        game_hint: None,
        seats: vec![],
        dealer: None,
        players: vec![],
        partnerships: None,
        caps: vec![],
        extra: vec![],
    };
    let mut deck: Option<Deck> = None;
    let mut events: Vec<Event> = Vec::new();

    for (idx, raw) in lines {
        let line_no = idx + 1;
        let line = match raw.split_once(';') {
            Some((code, _comment)) => code.trim_end(),
            None => raw,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            let (key, value) = parse_header(line)
                .ok_or_else(|| ParseError::BadHeader { line: line_no, text: line.to_string() })?;
            match key.as_str() {
                "Game" => meta.game_hint = Some(value),
                "Deck" => {
                    deck = Some(Deck::parse_decl(&value).ok_or(ParseError::BadDeck {
                        line: line_no,
                        decl: value.clone(),
                    })?);
                }
                "Seats" => meta.seats = value.split_whitespace().map(String::from).collect(),
                "Dealer" => meta.dealer = Some(value),
                "Players" => {
                    meta.players = value
                        .split_whitespace()
                        .map(|n| if n == "?" { None } else { Some(n.to_string()) })
                        .collect();
                }
                "Partnerships" => {
                    meta.partnerships = Some(
                        value
                            .split_whitespace()
                            .map(|g| g.chars().map(|c| c.to_string()).collect())
                            .collect(),
                    );
                }
                "Caps" => meta.caps = value.split_whitespace().map(String::from).collect(),
                _ if KNOWN_HEADERS.contains(&key.as_str()) => unreachable!(),
                _ => meta.extra.push((key, value)),
            }
            continue;
        }

        // Event lines. Deck must be known by now (events reference it).
        let deck_ref = deck
            .as_ref()
            .ok_or(ParseError::MissingHeader { key: "Deck" })?;
        events.push(parse_event(line, line_no, deck_ref)?);
    }

    if meta.seats.is_empty() {
        return Err(ParseError::MissingHeader { key: "Seats" });
    }
    let deck = deck.ok_or(ParseError::MissingHeader { key: "Deck" })?;
    Ok(Model { meta, deck, events })
}

fn cards_from_tokens(toks: &[&str], line_no: usize) -> Result<Vec<Card>, ParseError> {
    toks.iter()
        .map(|t| {
            parse_card(t).ok_or(ParseError::BadCard { line: line_no, token: t.to_string() })
        })
        .collect()
}

fn parse_event(line: &str, line_no: usize, deck: &Deck) -> Result<Event, ParseError> {
    let bad = || ParseError::BadEvent { line: line_no, text: line.to_string() };
    let (code, rest) = line.split_once(char::is_whitespace).ok_or_else(bad)?;
    let rest = rest.trim();
    match code {
        "D" => {
            let mut hands = Vec::new();
            for spec in rest.split_whitespace() {
                let (target, holding) = spec.split_once(':').ok_or_else(bad)?;
                let cards = parse_holdings(holding, deck).ok_or(ParseError::BadHoldings {
                    line: line_no,
                    holding: holding.to_string(),
                })?;
                hands.push((target.to_string(), cards));
            }
            Ok(Event::Deal { hands })
        }
        "C" => {
            let (start, vals) = rest.split_once(':').ok_or_else(bad)?;
            let values = vals.split_whitespace().map(String::from).collect();
            Ok(Event::Call { start: start.trim().to_string(), values })
        }
        "P" => {
            let (leader, cards) = rest.split_once(char::is_whitespace).ok_or_else(bad)?;
            let toks: Vec<&str> = cards.split_whitespace().collect();
            Ok(Event::Play {
                leader: leader.to_string(),
                cards: cards_from_tokens(&toks, line_no)?,
            })
        }
        "X" => {
            let (dirs, cards) = rest.split_once(':').ok_or_else(bad)?;
            let (from, to) = dirs.trim().split_once('>').ok_or_else(bad)?;
            let toks: Vec<&str> = cards.split_whitespace().collect();
            Ok(Event::Exchange {
                from: from.to_string(),
                to: to.to_string(),
                cards: cards_from_tokens(&toks, line_no)?,
            })
        }
        "U" => {
            let (target, cards) = rest.split_once(':').ok_or_else(bad)?;
            let toks: Vec<&str> = cards.split_whitespace().collect();
            Ok(Event::Reveal {
                target: target.to_string(),
                cards: cards_from_tokens(&toks, line_no)?,
            })
        }
        _ => Err(bad()),
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p trick-notation --lib text::decode`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/trick-notation/src/text/decode.rs crates/trick-notation/src/text/mod.rs
git commit -m "feat(trick-notation): canonical text decode"
```

---

### Task 6: Round-trip + fuzz tests

**Files:**
- Create: `crates/trick-notation/tests/round_trip.rs`

**Interfaces:**
- Consumes: public API `to_text`, `from_text`, `Model`, `Deck`, `Event`, `Meta`, `Card`.

- [ ] **Step 1: Write the round-trip + fuzz tests**

Create `crates/trick-notation/tests/round_trip.rs`:

```rust
use trick_notation::{Card, Deck, Event, Meta, Model, from_text, to_text};

fn card(rank: &str, suit: &str) -> Card {
    Card::Suited { suit: suit.into(), rank: rank.into() }
}

fn sample_model() -> Model {
    Model {
        meta: Meta {
            version: 1,
            game_hint: Some("hearts".into()),
            seats: vec!["N".into(), "E".into(), "S".into(), "W".into()],
            dealer: Some("N".into()),
            players: vec![Some("Ann".into()), None, Some("Cy".into()), None],
            partnerships: None,
            caps: vec!["exchange".into()],
            extra: vec![],
        },
        deck: Deck::french52(),
        events: vec![
            Event::Deal { hands: vec![("N".into(), vec![card("A", "S"), card("K", "H")])] },
            Event::Exchange { from: "N".into(), to: "E".into(), cards: vec![card("2", "C")] },
            Event::Play { leader: "S".into(), cards: vec![card("2", "C"), card("5", "C")] },
        ],
    }
}

#[test]
fn text_to_model_to_text_is_stable() {
    let m = sample_model();
    let text1 = to_text(&m);
    let parsed = from_text(&text1).expect("parse");
    assert_eq!(parsed, m);
    let text2 = to_text(&parsed);
    assert_eq!(text1, text2);
}

#[test]
fn fuzz_garbage_never_panics() {
    let inputs = [
        "",
        "garbage",
        "% trick-notation v1\n[Deck \"nope\"]\n",
        "% trick-notation v1\n[Seats \"N E S W\"]\nP\n",
        "% trick-notation v1\n[Deck \"french52\"]\n[Seats \"N E\"]\nD N:ZZ.-.-.-\n",
        "% trick-notation v1\n[Deck \"french52\"]\n[Seats \"N E S W\"]\nQ foo\n",
    ];
    for inp in inputs {
        // Must return Err, not panic.
        let _ = from_text(inp);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p trick-notation --test round_trip`
Expected: PASS (2 tests). If `fuzz_garbage_never_panics` panics, the offending parser branch needs a guard — fix in `decode.rs` and re-run.

- [ ] **Step 3: Commit**

```bash
git add crates/trick-notation/tests/round_trip.rs
git commit -m "test(trick-notation): text round-trip + fuzz"
```

---

### Task 7: spades-core adapter (Game ↔ Model) + retire STF

**Files:**
- Modify: `crates/spades-core/Cargo.toml` (add `trick-notation` dep)
- Create: `crates/spades-core/src/transcript/adapter.rs` (Game ↔ `trick_notation::Model`)
- Modify: `crates/spades-core/src/transcript/mod.rs` (re-wire `encode`/`decode`/`replay` over the adapter; keep public names)

**Interfaces:**
- Consumes: `trick_notation::{Model, Meta, Event, Deck, Card as TnCard, to_text, from_text}`; spades `Game`, `State`, `GameTransition`, `cards::{Card, Suit, Rank, get_trick_winner}`.
- Produces (unchanged public surface):
  - `pub fn spades::transcript::encode(&Game) -> String` (now canonical trick-notation text)
  - `pub fn spades::transcript::decode(&str) -> Result<trick_notation::Model, DecodeError>`
  - `pub fn spades::transcript::replay(&trick_notation::Model) -> Result<Game, ReplayError>`
- New internal:
  - `pub(crate) fn game_to_model(g: &Game) -> trick_notation::Model`
  - `pub(crate) fn model_to_game(m: &trick_notation::Model) -> Result<Game, ReplayError>`
  - `fn tn_card(c: Card) -> TnCard` and `fn from_tn_card(c: &TnCard) -> Option<Card>` (suit/rank char mapping: `Club→"C"`, `Diamond→"D"`, `Heart→"H"`, `Spade→"S"`; ranks `2..9`, `T,J,Q,K,A`).

> **Migration note:** this replaces the bespoke STF in `encode.rs`/`decode.rs`/`format.rs`/`replay.rs`. Keep `replay.rs`'s engine-driving logic (the `Game::override_hands` + transition loop) but have it read from a `trick_notation::Model` instead of the old `Transcript`. Map spades seats to fixed names `["N","E","S","W"]` (index→name). Store `GameId`, `MaxPoints`, and `Timer` (if any) in `meta.extra` so replay can reconstruct the `Game`. Spades is a single-deal-per-`Game` engine snapshot: emit one `Deal` event (current hands), one `Call` event when bets exist, and one `Play` event per completed trick (leader = trick's first seat).

- [ ] **Step 1: Add the dependency**

Edit `crates/spades-core/Cargo.toml` `[dependencies]`:

```toml
trick-notation = { path = "../trick-notation", version = "3.0.0" }
```

- [ ] **Step 2: Write the failing adapter test**

Create `crates/spades-core/src/transcript/adapter.rs` with the test first:

```rust
//! Bridge between the spades `Game` engine and the game-agnostic
//! `trick_notation::Model`. Spades-specific config (game id, max points, timer)
//! rides in `model.meta.extra`.

use trick_notation::{Card as TnCard, Model};

use crate::Game;
use super::ReplayError;

const SEAT_NAMES: [&str; 4] = ["N", "E", "S", "W"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameTransition, State};
    use uuid::Uuid;

    fn played_game(seed_byte: u8) -> Game {
        let ids = [
            Uuid::from_bytes([seed_byte; 16]),
            Uuid::from_bytes([seed_byte.wrapping_add(1); 16]),
            Uuid::from_bytes([seed_byte.wrapping_add(2); 16]),
            Uuid::from_bytes([seed_byte.wrapping_add(3); 16]),
        ];
        let mut g = Game::new(Uuid::from_bytes([9; 16]), ids, 60, None);
        g.play(GameTransition::Start).unwrap();
        // Place bets for whatever betting state we are in.
        while let State::Betting(_) = *g.get_state() {
            g.play(GameTransition::Bet(2)).unwrap();
        }
        g
    }

    #[test]
    fn game_to_model_to_game_preserves_state() {
        let g = played_game(1);
        let model = game_to_model(&g);
        let back = model_to_game(&model).expect("replay");
        assert_eq!(back.get_team_scores(), g.get_team_scores());
        assert_eq!(*back.get_state(), *g.get_state());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p spades --lib transcript::adapter`
Expected: FAIL — `game_to_model`/`model_to_game` not found.

- [ ] **Step 4: Implement the card mapping + `game_to_model`**

Add to `crates/spades-core/src/transcript/adapter.rs` (above `tests`). Use the existing `Game` accessors that the old STF `encode.rs` relied on (e.g. `get_id`, `get_max_points`, `get_player_ids`, `get_player_names`, per-round hands and tricks). Mirror the data the old `encode_rounds` collected, but emit `trick_notation` events:

```rust
use trick_notation::{Deck, Event, Meta};

use crate::cards::{Card, Rank, Suit};

fn suit_sym(s: Suit) -> &'static str {
    match s { Suit::Club => "C", Suit::Diamond => "D", Suit::Heart => "H", Suit::Spade => "S" }
}

fn rank_sym(r: Rank) -> &'static str {
    match r {
        Rank::Two => "2", Rank::Three => "3", Rank::Four => "4", Rank::Five => "5",
        Rank::Six => "6", Rank::Seven => "7", Rank::Eight => "8", Rank::Nine => "9",
        Rank::Ten => "T", Rank::Jack => "J", Rank::Queen => "Q", Rank::King => "K",
        Rank::Ace => "A",
    }
}

fn tn_card(c: Card) -> TnCard {
    TnCard::Suited { suit: suit_sym(c.suit).into(), rank: rank_sym(c.rank).into() }
}

pub(crate) fn from_tn_card(c: &TnCard) -> Option<Card> {
    let TnCard::Suited { suit, rank } = c else { return None };
    let suit = match suit.as_str() {
        "C" => Suit::Club, "D" => Suit::Diamond, "H" => Suit::Heart, "S" => Suit::Spade,
        _ => return None,
    };
    let rank = match rank.as_str() {
        "2" => Rank::Two, "3" => Rank::Three, "4" => Rank::Four, "5" => Rank::Five,
        "6" => Rank::Six, "7" => Rank::Seven, "8" => Rank::Eight, "9" => Rank::Nine,
        "T" => Rank::Ten, "J" => Rank::Jack, "Q" => Rank::Queen, "K" => Rank::King,
        "A" => Rank::Ace, _ => return None,
    };
    Some(Card { suit, rank })
}

pub(crate) fn game_to_model(g: &Game) -> Model {
    let mut extra = vec![
        ("GameId".to_string(), g.get_id().to_string()),
        ("MaxPoints".to_string(), g.get_max_points().to_string()),
    ];
    if let Some(t) = g.get_timer_config() {
        extra.push(("Timer".to_string(), format!("{}+{}", t.initial_time_secs, t.increment_secs)));
    }

    let names = g.get_player_names();
    let players = (0..4).map(|i| names[i].clone()).collect();

    let meta = Meta {
        version: 1,
        game_hint: Some("spades".into()),
        seats: SEAT_NAMES.iter().map(|s| s.to_string()).collect(),
        dealer: None,
        players,
        partnerships: Some(vec![vec!["N".into(), "S".into()], vec!["E".into(), "W".into()]]),
        caps: vec![],
        extra,
    };

    let mut events: Vec<Event> = Vec::new();
    // Emit the current dealt hands as a single Deal event (seat order N E S W).
    let mut hands = Vec::new();
    for seat in 0..4 {
        let cards: Vec<TnCard> = g.get_dealt_hand(seat).iter().map(|c| tn_card(*c)).collect();
        hands.push((SEAT_NAMES[seat].to_string(), cards));
    }
    events.push(Event::Deal { hands });

    // Bets, if placed (seat order from first bidder).
    if let Some(bets) = g.get_all_bets() {
        events.push(Event::Call {
            start: SEAT_NAMES[g.first_bidder_seat()].to_string(),
            values: bets.iter().map(|b| b.to_string()).collect(),
        });
    }

    // Completed tricks → Play events.
    for trick in g.completed_tricks() {
        let leader = SEAT_NAMES[trick.leader_seat].to_string();
        let cards = trick.cards_in_play_order.iter().map(|c| tn_card(*c)).collect();
        events.push(Event::Play { leader, cards });
    }

    Model { meta, deck: Deck::french52(), events }
}
```

> **Adapter accessor note:** the method names above (`get_dealt_hand`, `get_all_bets`, `first_bidder_seat`, `completed_tricks`, `get_timer_config`) describe the data the old STF encoder already read. If a spades-core accessor has a different name, use the existing one — confirm against `crates/spades-core/src/transcript/encode.rs` (the retiring encoder) and `crates/spades-core/src/lib.rs`. Do not add new engine APIs unless a needed datum is genuinely unexposed; if so, add a minimal `pub(crate)` getter.

- [ ] **Step 5: Implement `model_to_game`**

Port `crates/spades-core/src/transcript/replay.rs`'s engine-driving logic, reading from the model. Add to `adapter.rs`:

```rust
use crate::GameTransition;
use uuid::Uuid;

fn extra<'a>(m: &'a Model, key: &str) -> Option<&'a str> {
    m.meta.extra.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

pub(crate) fn model_to_game(m: &Model) -> Result<Game, ReplayError> {
    let game_id = extra(m, "GameId")
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(|| Uuid::from_bytes([0; 16]));
    let max_points = extra(m, "MaxPoints").and_then(|s| s.parse().ok()).unwrap_or(0);
    // Player ids are not part of the rule-agnostic model; synthesize stable ids
    // (replay reconstructs game *state*, not external identity).
    let player_ids = [
        Uuid::from_bytes([1; 16]), Uuid::from_bytes([2; 16]),
        Uuid::from_bytes([3; 16]), Uuid::from_bytes([4; 16]),
    ];

    let mut game = Game::new(game_id, player_ids, max_points, None);
    game.play(GameTransition::Start)
        .map_err(|err| ReplayError::Transition { round: 0, trick: None, seat: 0, err })?;

    for event in &m.events {
        match event {
            Event::Deal { hands } => {
                let mut dealt: [Vec<Card>; 4] = Default::default();
                for (seat_idx, (_target, cards)) in hands.iter().enumerate() {
                    dealt[seat_idx] = cards.iter().filter_map(from_tn_card).collect();
                }
                game.override_hands(dealt);
            }
            Event::Call { values, .. } => {
                for (seat, v) in values.iter().enumerate() {
                    let bet: i32 = v.parse().unwrap_or(0);
                    game.play(GameTransition::Bet(bet)).map_err(|err| {
                        ReplayError::Transition { round: 0, trick: None, seat, err }
                    })?;
                }
            }
            Event::Play { cards, .. } => {
                for (seat, c) in cards.iter().enumerate() {
                    if let Some(card) = from_tn_card(c) {
                        game.play(GameTransition::Card(card)).map_err(|err| {
                            ReplayError::Transition { round: 0, trick: Some(0), seat, err }
                        })?;
                    }
                }
            }
            Event::Exchange { .. } | Event::Reveal { .. } => {
                // Not produced by the spades adapter; ignore on replay.
            }
        }
    }
    Ok(game)
}
```

> **Override note:** `override_hands` is the existing engine hook the old `replay.rs` used to inject declared hands after `Start` shuffles. Match its real signature (it takes the per-seat dealt hands). If it currently takes `[[Option<Card>;4]]` or similar, adapt the construction accordingly — confirm in `crates/spades-core/src/lib.rs`.

- [ ] **Step 6: Re-wire the public transcript API**

Replace `crates/spades-core/src/transcript/mod.rs`'s public functions so they delegate to the adapter and `trick_notation` text. Keep `DecodeError`/`ReplayError` enums (they are public). Add `mod adapter;`. The new bodies:

```rust
mod adapter;

pub use trick_notation::Model;

/// Serialize a `Game` to canonical trick-notation text.
pub fn encode(game: &Game) -> String {
    trick_notation::to_text(&adapter::game_to_model(game))
}

/// Parse canonical trick-notation text into a `Model`.
pub fn decode(text: &str) -> Result<Model, DecodeError> {
    trick_notation::from_text(text).map_err(|e| DecodeError::Text(e.to_string()))
}

/// Replay a `Model` back into a `Game`.
pub fn replay(model: &Model) -> Result<Game, ReplayError> {
    adapter::model_to_game(model)
}
```

Add a `Text` variant to `DecodeError`:

```rust
    #[error("text parse error: {0}")]
    Text(String),
```

Remove the now-unused `mod encode; mod decode; mod format;` declarations and delete those files (`encode.rs`, `decode.rs`, `format.rs`) plus the old `Transcript`/`Headers`/`Round` structs if no longer referenced. Keep the `property_tests` module but update it to assert `Game → model → Game` state equality (see Step 7).

- [ ] **Step 7: Update the property test**

Replace the body of `crates/spades-core/src/transcript/mod.rs`'s `property_tests::round_trip_is_idempotent_on_many_random_games`:

```rust
    #[test]
    fn round_trip_is_idempotent_on_many_random_games() {
        for seed in 0..30u64 {
            let g = play_full_random_game(seed);
            let text = encode(&g);
            let model = decode(&text).expect("decode");
            let replayed = replay(&model).expect("replay");
            assert_eq!(encode(&replayed), text, "round trip differed for seed {}", seed);
        }
    }
```

- [ ] **Step 8: Run all spades-core tests + clippy**

Run: `cargo test -p spades`
Expected: PASS (adapter test + property test + existing suite). Fix accessor-name mismatches against the real engine API as flagged in the notes.

Run: `cargo clippy -p trick-notation -p spades -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/spades-core/Cargo.toml crates/spades-core/src/transcript/
git commit -m "refactor(spades-core): retire STF, adapt transcript onto trick-notation"
```

---

### Task 8: Conformance fixtures (hearts, euchre)

**Files:**
- Create: `crates/trick-notation/tests/conformance.rs`

**Interfaces:**
- Consumes: public API (`from_text`, `to_text`, `Model`).

These prove the format is genuinely game-agnostic, not spades-shaped. Each fixture is a hand-written canonical document that must parse and re-encode identically.

- [ ] **Step 1: Write the hearts + euchre conformance tests**

Create `crates/trick-notation/tests/conformance.rs`:

```rust
use trick_notation::{from_text, to_text};

/// A canonical doc must parse and re-encode byte-identically (stable canonical form).
fn assert_canonical(text: &str) {
    let model = from_text(text).unwrap_or_else(|e| panic!("parse failed: {e}\n---\n{text}"));
    assert_eq!(to_text(&model), text, "re-encode differed");
}

#[test]
fn hearts_with_pass_phase() {
    let text = "\
% trick-notation v1
[Game \"hearts\"]
[Deck \"french52\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Caps \"exchange\"]

D N:AK.T9.-.-.- E:-.-.-.- S:-.-.-.- W:-.-.-.-
X N>E: 2C 7D KH
P S 2C 5C 9C KC
";
    // NOTE: holdings groups must match deck.suits length (4 for french52).
    // Replace the deal line above with valid 4-group holdings before running.
    let _ = text;
    let fixed = "\
% trick-notation v1
[Game \"hearts\"]
[Deck \"french52\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Caps \"exchange\"]

D N:AK.T9.-.-
X N>E: 2C 7D KH
P S 2C 5C 9C KC
";
    assert_canonical(fixed);
}

#[test]
fn euchre_with_kitty_and_turnup() {
    let text = "\
% trick-notation v1
[Game \"euchre\"]
[Deck \"euchre24\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Partnerships \"NS EW\"]
[Caps \"piles\"]

D E:9T.Q.KA.- S:JQ.-.9T.A W:-.9TJ.-.QK N:KA.K.J.9T @kitty:-.-.-.J
U @kitty:JD
C E: pass pass make N
P E 9S TS JS QS
";
    assert_canonical(text);
}
```

> **Fixture note:** remove the first (broken) `text` block in `hearts_with_pass_phase` and keep only the `fixed` document — it is shown twice to make the 4-group holdings rule explicit. The euchre deal distributes 24 cards across four seats + a kitty pile; the exact card split is illustrative — what matters is that every holding has exactly `deck.suits.len()` dot-groups and re-encodes identically.

- [ ] **Step 2: Run the conformance tests**

Run: `cargo test -p trick-notation --test conformance`
Expected: PASS (2 tests). If a holding has the wrong group count, the parser returns `BadHoldings` — fix the fixture's dot-groups.

- [ ] **Step 3: Commit**

```bash
git add crates/trick-notation/tests/conformance.rs
git commit -m "test(trick-notation): hearts + euchre conformance fixtures"
```

---

### Task 9: Coverage baseline + final gate

**Files:**
- Modify: `coverage-baseline.json` (add `trick-notation` key)

- [ ] **Step 1: Add a coverage baseline entry**

Read `docs/coverage.md` for the ratchet procedure and run the project's coverage baseline updater (per CLAUDE.md: `hooks/update-coverage-baseline.sh`) so `trick-notation` gets an honest starting line-coverage number. Do not hand-invent the number.

Run: `bash hooks/update-coverage-baseline.sh` (or the procedure docs/coverage.md specifies)
Expected: `coverage-baseline.json` gains a `trick-notation` key.

- [ ] **Step 2: Full pre-push gate**

Run: `make check`
Expected: fmt-check clean, clippy `-D warnings` clean, all tests pass (workspace + web). Address any fmt/clippy issues.

- [ ] **Step 3: Commit**

```bash
git add coverage-baseline.json
git commit -m "chore: coverage baseline for trick-notation crate"
```

---

## Self-Review notes (for the implementer)

- **Spec coverage:** This plan covers trick-notation Phase 1 — model, canonical text (encode/decode), JSON projection, and the spades-core adapter that retires STF. Deferred per spec: compact binary codec, exotic capabilities (multiplicity/specials/per-suit-ranks → pinochle/tarot), the server `replay.json` endpoint, and the web viewer. Those get their own plans.
- **Engine accessor names** in Task 7 are described by their data, not guaranteed to match the current spades-core API verbatim — the implementer must reconcile against `crates/spades-core/src/lib.rs` and the retiring `encode.rs`/`replay.rs`. This is the one place the plan cannot be fully literal without the engine's full public surface in hand.
- **Type consistency:** `Model { meta, deck, events }`, `Event::{Deal,Call,Play,Exchange,Reveal}`, `Deck { suits, ranks }`, `Card::{Suited,Special}`, and `Meta` fields are used identically across Tasks 3–8.
