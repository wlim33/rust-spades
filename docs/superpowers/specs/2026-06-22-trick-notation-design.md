# Trick-Taking Notation — Design

**Date:** 2026-06-22
**Status:** Approved (pending spec review)

## Summary

Design a **portable, game-agnostic notation for trick-taking card games** — the
PGN of trick-taking. It must encode the deck, the deal, hands, and the full play
sequence for *any* trick-taking game (spades, hearts, euchre, pinochle, tarot, …),
be both **readable** and **efficient**, and never bake in game-specific rules.

It generalizes and replaces the current spades-only STF (Spades Transcript Format).
It ships as a standalone crate and becomes the canonical text/interop format behind
the replay viewer (see `2026-06-22-replay-viewer-design.md`).

## Reference: prior art

- **PBN (Portable Bridge Notation)** — PGN-style, `[Key "value"]` headers + sections.
  Its **dot-grouped suit holdings** (`AKQ.J95.T.8732` = a 13-card hand, suit
  boundaries via dot position, no per-card suit repetition) are the readable-yet-dense
  trick we adopt. PBN is bridge-only: it hardcodes 4 French suits, 4 seats, SHDC order.
- **LIN (BBO)** — maximally compact but **undocumented and cryptic** ("lives in
  people's memory"). The cautionary tale: our compact codec is a strict, fully
  documented *derivative* of the canonical text, never the source of truth.

## Core principles

1. **Self-describing.** The file declares its own deck and seat roster, so a generic
   parser needs zero built-in game knowledge.
2. **Events, not derived state.** The notation records *observed events* (dealt,
   played, passed) — never rule-derived facts (trick winner, score). Derived facts
   vary by game; events don't.
3. **Explicit per-trick leader.** Each trick records its leader seat. This is the one
   decision that makes the format truly rule-agnostic: knowing who leads trick 2
   otherwise requires knowing who *won* trick 1 (needs trump/rank rules). Cost is one
   seat token per trick (~13/game) — cheap for full rule-independence.
4. **Readable canonical text is the source of truth; compact bytes derive from it.**
5. **Extensible core + capability profiles.** A tiny mandatory core every game uses,
   plus opt-in capabilities for the exotic cases.

## Section A — Data model

One in-memory model, four parts:

```
Model {
  meta:   { version, game_hint, seats[], rotation, dealer, players[], partnerships? }
  deck:   Deck            // self-describing: suits, ranks-per-suit, specials, multiplicity
  deal:   Map<Target, [Card]>   // Target = a seat OR a named pile (@kitty, @talon, @chien)
  events: [Event]         // ordered stream of what happened
}
```

### Capability profiles

A spades parser ignores capabilities it doesn't need; a generic viewer reads all.

| Capability      | Unlocks                          | Needed by                  |
|-----------------|----------------------------------|----------------------------|
| `multiplicity`  | duplicate cards in deck          | pinochle (double deck)     |
| `specials`      | non-suit/rank cards              | jokers, tarot Fool         |
| `per-suit-ranks`| different rank sets per suit      | tarot (21 trumps)          |
| `piles`         | non-seat deal targets             | euchre kitty, tarot chien  |
| `exchange`      | card-passing / discard phases     | hearts pass, draw games    |
| `meld`          | declaring card combinations       | pinochle                   |
| `seats=N`       | any seat count (3–6+)             | tarot (3–5), skat (3)      |

### Event vocabulary (core + capability)

Uniform shape for multi-seat events: `<code> <start-seat>: v1 v2 v3 …` in declared
rotation order.

| Code | Event        | Core?       | Payload                                   |
|------|--------------|-------------|-------------------------------------------|
| `D`  | Deal         | core        | per-target holdings                        |
| `C`  | Call         | core        | one free-form token per seat (rules interpret) |
| `P`  | Play (trick) | core        | leader seat + cards in rotation order      |
| `X`  | Exchange     | `exchange`  | `from>to: cards`                           |
| `U`  | Reveal/turn-up | `piles`   | target + card(s) made visible              |
| `M`  | Meld         | `meld`      | seat + cards [+ label]                      |

Call tokens are free-form within a small charset (`3`, `nil`, `pass`, `X`, `1NT`,
`make:H`, …). The notation records the token verbatim; only game rules interpret it.

### Derived-state policy

An optional, clearly **non-normative** `[Result …]` / annotation block is permitted
(like PBN's score tags) for convenience, but is never required and never authoritative.
Consumers recompute derived facts from events using their own rules.

## Section B — Canonical text grammar

`% version` marker, `[Key "value"]` headers, line-oriented event stream. Cards use
PBN dot-grouped holdings. `;` begins a comment to end of line.

### Spades

```
% trick-notation v1
[Game "spades"] [Deck "french52"] [Seats "N E S W"] [Dealer "N"]
[Players "Ann Bo Cy Di"]

D N:AKQ4.T98.AK6.QJ97 E:J32.AKQ.QJT.AK85 S:T9.J765.987.T762 W:765.432.5432.43
C E: 3 4 nil 4          ; calls, E first (left of dealer), rotation order
P E KC 5C 2C TC         ; trick: E leads KC, then S W N in rotation
P S QH JH 3H AH
…
```

### Hearts (no bids, 3-card pass; `exchange` capability)

```
[Game "hearts"] [Deck "french52"] [Seats "N E S W"] [Caps "exchange"]
D N:… E:… S:… W:…
X N>E: 2C 7D KH         ; pass-left: N gives 3 to E (one X line per passer)
X E>S: …
P S 2C 5C 9C KC        ; leader explicit — no "holder of 2♣ leads" rule needed
…
```

### Euchre (24-card, kitty pile, turn-up, partnerships; `piles` capability)

```
[Game "euchre"] [Deck "euchre24"] [Seats "N E S W"] [Dealer "N"]
[Partnerships "NS EW"] [Caps "piles"]
D E:9TJ.Q.KA.- S:… W:… N:… @kitty:JD     ; 5 each; kitty holds turn-up
U @kitty:JD            ; turn-up revealed
C E: pass pass make:H N
P E 9S TS JS QS
…
```

### Exotic decks (presets expand to inline declarations)

```
[Deck "pinochle48"]    ; ⇒ suits=SHDC ranks=9TJQKA mult=2
[Deck "tarot78"]       ; ⇒ suits=SHDCT ranks=23456789TJQKA ranks@T=1..21 specials=Fool
```

Inline custom deck when no preset fits:

```
[Deck "suits=SHDC ranks=9TJQKA mult=2"]
[Deck "suits=SHDCT ranks=23456789TJQKA ranks@T=1..21 specials=Fool"]
```

## Section C — Compact binary codec

Strict derivative of the model; MSB-first bit-packing, documented precisely.

```
[magic "TN" + version byte]
[header]  preset-id varint (0 ⇒ inline deck decl) · seats(4b) · dealer · caps bitfield · names(optional,len-prefixed)
[deal]    assignment vector: for each card in canonical deck order, its target index
[events]  tagged stream: type(3–4b) + payload
```

Density levers:

- **Card = index into declared deck**, `ceil(log2(deckSize))` bits → french52 = 6 bits.
  Multiplicity = repeated indices (pinochle still 6 bits).
- **Deal = assignment vector** (target index per card slot in canonical order), not
  card lists → 4 seats = 2 bits × 52 = **13 bytes**. Undealt/pile cards get a slot too.
- **Trick = leader(2b) + 4 card-indices(6b) = 26 bits ≈ 3.25 B.** Partial final trick
  carries a small length field.
- **Calls/melds** use a file-level token dictionary (repeated `pass`/`nil` → one index).

**Bit budget, full spades game:** header ~6 B + deal 13 B + calls ~6 B +
13 × 3.25 B ≈ **~70 B** (names excluded) vs ~600–800 chars canonical text — ~10×.

Round-trips property-tested across all three serializations; equality asserted on the
**model**, not bytes (canonical text and compact bytes both parse to the same model).

## Section D — Crate layout & integration

- **`crates/trick-notation`** — new workspace member, no async deps (mirrors
  spades-core). One model, three serializations: `text`, `compact`, `json` (serde).
  The model is **pure / rule-agnostic** — it does *not* carry `cumulative`/`tricks_won`
  (derived).
- **spades-core**: `transcript` module becomes a thin adapter — `Game →
  trick_notation::Model` (encode) and `Model → Game` (existing replay logic). STF
  retires into the canonical format.
  - ⚠️ `spades::transcript` is **public API** on the published `spades` crate. It is at
    **3.0.0 in-tree, unpublished** (crates.io max 2.0.0), so the break lands within the
    unreleased 3.0.0 line — no published-consumer breakage. Bump per the three-file
    version rule (CLAUDE.md) only at publish time.
- **Replay endpoint** (reconciles `2026-06-22-replay-viewer-design.md`):
  - `text/plain` export now emits **canonical trick-notation** (replaces STF).
  - `replay.json` = **JSON projection of the general model** (`deck/seats/deal/events`)
    **+** server-only annotation fields (`cumulative`, `tricks_won`, `viewer_seat`)
    wrapped in the HTTP DTO. Notation crate stays pure; server composes
    pure-model-JSON + annotations.
  - A compact-format endpoint (`?format=compact`) is **deferred** to a follow-on.

## Section E — Validation & testing

- **Conformance fixtures:** encode a real recorded game of each target — spades,
  hearts, euchre, pinochle, tarot — to canonical text; round-trip
  `text ↔ model ↔ compact ↔ model`; assert canonical text stable + models equal. These
  double as golden examples.
- **Property tests:** random spades games (via the engine) round-trip across all three
  serializations (migrates the existing transcript property test).
- **Fuzz** both parsers: malformed input → typed errors, never panics.
- **spades-core adapter:** existing transcript tests become `Game → model → Game`.
- **Coverage:** new crate gets a baseline entry per `docs/coverage.md`'s ratchet.

## Out of scope (YAGNI for v1)

- A TypeScript implementation of the notation (the web viewer consumes `replay.json`,
  the JSON projection — it does not parse canonical text or compact bytes).
- The `?format=compact` HTTP endpoint (model supports it; not needed for the viewer).
- Engine evaluation / annotations beyond the non-normative result block.
- A general TS *rules* library (trick-winner stays the one bit of per-game TS logic).

## Build order

1. `trick-notation` crate: model + canonical text (encode/decode) + tests.
2. Compact codec + round-trip property tests.
3. JSON projection (serde).
4. spades-core transcript → adapter over the new crate; migrate its tests.
5. Server: `replay.json` DTO (model JSON + annotations) + text endpoint emits canonical.
6. Conformance fixtures for hearts/euchre/pinochle/tarot.
