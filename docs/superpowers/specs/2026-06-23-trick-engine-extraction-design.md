# Trick-Engine Extraction — Design

**Date:** 2026-06-23
**Status:** Approved (design phase)
**Topic:** Extract a generic, game-agnostic trick-taking engine from `spades-core`, so future trick-taking card games (Hearts, Euchre, Oh Hell, Whist, …) share one round/trick/rotation skeleton.

## Goal

Today `spades-core` is a monolithic engine with spades rules woven through trick
resolution, legality, and scoring. We already have a *notation* layer
(`trick-notation`) that is game-agnostic for **recording** games. This project
adds the missing peer: a game-agnostic layer for **playing** games. Spades
becomes the first ruleset that plugs into it; the published `spades` crate keeps
its current public API.

Non-goals: shipping a second game, redesigning the public spades API, building a
runtime game-picker UI. Those come later, on top of this seam.

## Decisions (locked during brainstorming)

1. **Ruleset dispatch:** trait object — `Box<dyn Ruleset>`. One concrete `Game`
   type; the (future) server selects a game by string at runtime. Engine pays a
   small dynamic-dispatch cost on the few ruleset calls per move.
   - *Consequence:* the `Ruleset` trait must be **object-safe** (no generic
     methods, no `Self`-returning methods, no associated consts in signatures).
2. **Table shape:** variable seat count from the start — `Vec<Player>`,
   `rules.seat_count()`, `(seat + 1) % n`. Deal sizes, first leader, and team
   grouping all come from the ruleset.
3. **Published API:** preserve the spades facade. `spades-core` keeps `Game`,
   `GameTransition`, `State`, and every existing accessor
   (`get_team_a_score`, `get_team_a_bags`, `get_player_bets`, …), delegating
   inward to `trick-engine`. Server/web/README keep compiling.
4. **Ruleset state + serialization:** stateful ruleset via the `typetag` crate.
   The ruleset object owns its scoring state (`Spades` holds the existing
   `Scoring` struct); `#[typetag::serde]` serializes `Box<dyn Ruleset>` with a
   `{"game":"spades", …}` tag. Each ruleset is `Serialize`/`Deserialize`.

## Architecture — three-layer stack

```
trick-notation   (exists)  Card, Deck, Model, text/JSON serialization — pure data
      ▲
trick-engine     (NEW)     Game state machine + Ruleset trait, on notation Cards
      ▲
spades-core      (facade)  `Spades: Ruleset` + scoring.rs + the `spades` public API
```

- `trick-engine` depends on `trick-notation` and **reuses its `Card`/`Deck`
  types**. The engine treats cards as opaque identity tokens — all suit/trump
  reasoning lives in the ruleset — so the engine never needs a rank ordering or
  a trump concept of its own.
- Because engine state is already expressed in notation `Card`s, the transcript
  adapter's bespoke card-mapping table (`spades-core/src/transcript/adapter.rs`)
  largely collapses.
- `spades-core` depends on `trick-engine` + `trick-notation`, keeps its API,
  still publishes as `spades`.

## The `Ruleset` trait

Object-safe and `typetag`-serialized. The engine drives the skeleton and asks
the ruleset every game-specific question:

```rust
#[typetag::serde(tag = "game")]
pub trait Ruleset {
    // table shape (variable seats)
    fn seat_count(&self) -> usize;
    fn team_of(&self, seat: usize) -> TeamId;       // singleton teams ⇒ no partnerships

    // setup
    fn build_deck(&self) -> Vec<Card>;              // engine shuffles + deals
    fn hand_size(&self, round: usize) -> usize;
    fn first_leader(&self, round: usize) -> usize;

    // bidding (optional phase)
    fn bid_phase(&self) -> Option<BidSpec>;         // None ⇒ skip straight to tricks
    fn bid_is_legal(&self, seat: usize, bid: i32) -> bool;

    // trick play — trump/follow-suit live here, not in the engine
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<Card>;
    fn trick_winner(&self, lead: usize, played: &[Card]) -> usize;

    // scoring + termination (stateful — Spades holds its Scoring here)
    fn score_round(&mut self, outcome: &RoundOutcome);
    fn is_over(&self) -> bool;
    fn scores(&self) -> Vec<i32>;                   // per TeamId, for generic readers
}
```

Engine-owned support types (plain structs, not part of the trait surface):

- `TeamId(usize)` — a grouping key; spades maps `seat % 2`, hearts maps `seat`
  (every seat its own team).
- `BidSpec` — describes the bid phase (e.g. inclusive range) for generic readers.
- `PlayContext` — `{ hand: &[Card], table: &[Option<Card>], leader, leading_suit,
  round }`. Everything `legal_plays` needs.
- `RoundOutcome` — `{ tricks_won: Vec<i32> (per seat), bids: Vec<i32> }`. What
  `score_round` consumes.

## The generic `Game` (engine)

```rust
pub struct Game {
    rules: Box<dyn Ruleset>,     // typetag-serialized
    state: State,                // NotStarted | Bidding(seat) | Trick(seat) | Completed | Aborted
    players: Vec<Player>,        // variable seat count
    current_seat: usize,
    deck: Vec<Card>,
    trick: Vec<Option<Card>>,    // sized to seat_count
    history: Vec<[Option<Card>]>,
    round: usize,
    // timer/clock fields carry over unchanged (engine-generic, not spades-specific)
}
```

`play()` keeps today's control-flow shape (`spades-core/src/lib.rs:330`) but each
spades-specific branch becomes a `rules.*` call:

| Today (spades-specific, inline) | After (ruleset call) |
|---|---|
| bid in `0..=13` | `rules.bid_is_legal(seat, bid)` |
| spades-broken / follow-suit in `play` + `get_legal_cards` | `rules.legal_plays(ctx)` |
| `get_trick_winner` (spade trump hardcoded) | `rules.trick_winner(lead, played)` |
| `Scoring::trick` / round totals / termination | `rules.score_round(outcome)` + `rules.is_over()` |

`State::Betting` is renamed `State::Bidding` (generic); the facade re-exports the
spades-facing name if needed for API stability.

## `spades-core` as the facade

- `scoring.rs` moves essentially intact into the ruleset:
  `#[derive(Serialize, Deserialize)] struct Spades { scoring: Scoring }`.
- Spades rules become method bodies: spades-always-trump and
  spades-broken-to-lead in `legal_plays`/`trick_winner`; `0..=13` in
  `bid_is_legal`; bags / nil / double-nil / 10-bag penalty / `MIN_POINTS` /
  `MAX_ROUNDS` in `score_round` + `is_over`.
- `team_of(seat) = seat % 2`; `seat_count() = 4`; `build_deck` = french-52;
  `hand_size = 13`.
- Public `Game` becomes a delegating newtype:

```rust
pub struct Game(trick_engine::Game);
impl Game {
    pub fn new(id, players: [Uuid;4], max_points, timer) -> Game { /* builds Spades ruleset */ }
    pub fn get_team_a_score(&self) -> Result<i32, GetError> { /* reads rules.scores()[0] */ }
    // every existing accessor preserved, delegating inward
}
```

- The transcript adapter simplifies (engine state already in notation `Card`s);
  its public `encode`/`decode`/`replay` API is unchanged.

## Data flow

1. `Game::new` builds a `Spades` ruleset, boxes it, hands it to the engine.
2. `Start` → engine asks `build_deck` + `hand_size`, shuffles, deals
   `seat_count` hands, sets `first_leader`, enters `Bidding` (or `Trick` if
   `bid_phase()` is `None`).
3. Each `Bid` → `bid_is_legal`; rotate; last bid → enter `Trick`.
4. Each `Card` → `legal_plays` validates; on the last card of a trick,
   `trick_winner` picks the lead of the next trick.
5. Last trick of a round → engine builds `RoundOutcome`, calls `score_round`,
   then `is_over`; either `Completed` or re-deal for the next round.

## Error handling

- `TransitionError` / `GetError` stay in the facade and keep their variants for
  API stability. Generic engine errors map onto them. Spades-named variants
  (`SpadesNotBroken`) remain valid spades errors surfaced from `legal_plays`
  rejection; the generic engine reports an opaque "illegal play" that the facade
  translates. (Exact mapping decided in the plan.)

## Testing

- **Regression oracle:** the entire existing `spades-core` test suite
  (`src/tests/`, scoring tests, transcript round-trip + property tests) must stay
  green. This is the primary safety net for the facade.
- **New engine unit tests:** a toy 4-seat "highest-card-of-led-suit-wins"
  ruleset exercises the generic skeleton independently of spades (deal,
  rotation, legality delegation, round/termination plumbing).
- **Coverage ratchet:** new `trick-engine` crate gets its own
  `coverage-baseline.json` entry; spades-core coverage must not drop.
- `make check` (fmt + clippy -D warnings + all tests + e2e) is the gate.

## Build sequence (strangler)

1. Create `trick-engine`; reuse notation `Card`/`Deck`; define `Ruleset` +
   engine `Game`; unit-test with the toy ruleset.
2. Implement `Spades: Ruleset` in spades-core; move `scoring.rs` in.
3. Rewrite spades-core `Game` as the delegating facade.
4. Keep the existing spades-core test suite green; update the transcript adapter.
5. Confirm server/web compile unchanged; run `make check`.

## Risks / accepted tradeoffs

1. **`typetag` dependency** pulls in `inventory` + `erased-serde`. Pre-flight
   check in the plan: confirm it does not collide with the sqlx/oauth2 version
   ceilings documented in `crates/spades-server/Cargo.toml`. If it conflicts,
   fall back to a manual tagged-enum (de)serializer for the ruleset.
2. **Persisted-game serde shape changes.** `Game`'s JSON gains a `rules` tag,
   renames `Betting`→`Bidding`, and uses `Vec` where it used `[_;4]`. Existing
   stored rows in the prod SQLite DB will not deserialize. **Accepted decision:**
   one-time reset of in-flight stored games — no migration shim. Rationale: the
   `hands_played` table is already pruned operationally, in-flight games are
   short-lived, and the owner treats internal breaking changes liberally.
   Completed games are preserved as transcripts, which are unaffected (notation
   format is stable). The plan must include the reset step (or a `--db` wipe
   note) in the deploy runbook.
3. **`oasgen`/`openapi` feature.** DTOs derive `OaSchema`. The facade keeps
   exposing spades-shaped DTOs, so the OpenAPI surface should be unchanged, but
   the plan must re-run `openapi:fetch` + `openapi:generate` and diff to confirm.

## Resolved during review

- **`State` naming:** the engine uses `State::Bidding` internally; `spades-core`
  re-exports it as a **facade alias `Betting`** so the web/server (`web/src/state/`)
  keep reading the same state name. No web/server churn.

## Open items for the implementation plan

- Exact `TransitionError` ↔ generic-engine-error mapping.
- `typetag` vs manual tagged enum (gated on the pre-flight dep check).
