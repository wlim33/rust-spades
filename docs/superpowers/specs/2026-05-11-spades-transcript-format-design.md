# Spades Transcript Format (STF) — Design

Status: approved
Date: 2026-05-11
Scope: `spades-core` crate — new `transcript` module for serializing full game histories in a PGN-inspired text format with round-trip and replay support.

## Goals

1. **Correctness first.** The format records pure game inputs; replay through the engine is the source of truth for derived state.
2. **Deterministic.** Byte-equal output for byte-equal game state. No timestamps, no nondeterministic ordering.
3. **Efficient enough.** Text PGN-style is the target shape; not optimizing for minimum size, but no waste.
4. **Vigorously tested.** Encoder, decoder, replay, and round-trip properties each have unit coverage.

## Non-goals

- Binary encoding (out of scope for v1; can be layered later).
- Backwards-compatible parsing of legacy formats (no prior format exists).
- Embedded comments, annotations, or human-edited transcripts (rejected at decode).

## Module layout

```
crates/spades-core/src/transcript/
  mod.rs       — public API: encode, decode, replay, error types, re-exports
  format.rs    — card notation tables, tag tokens, grammar constants
  encode.rs    — Game → String
  decode.rs    — &str → Transcript (syntactic only; no engine calls)
  replay.rs    — Transcript → Game (drives GameTransition through Game::play)
  tests/       — unit + golden fixtures + round-trip property test
```

Exposed from `lib.rs` as `pub mod transcript`.

## Format syntax

ASCII-only, LF line endings, single trailing LF.

### Header section

Tag pairs in **fixed order**, one per line, with the form `[Key "Value"]`:

| Tag | Required | Notes |
|-----|----------|-------|
| `GameId` | yes | UUID, hyphenated lowercase |
| `MaxPoints` | yes | `i32` as decimal |
| `Player0` | yes | UUID |
| `Player1` | yes | UUID |
| `Player2` | yes | UUID |
| `Player3` | yes | UUID |
| `Name0` | optional | omitted entirely when `None`; never written empty |
| `Name1` | optional | |
| `Name2` | optional | |
| `Name3` | optional | |
| `TimerInitial` | optional | u64 seconds; both timer tags present or both absent |
| `TimerIncrement` | optional | u64 seconds |
| `Termination` | yes | `Completed` \| `Aborted` \| `InProgress` |
| `Result` | yes | `"A B"` cumulative scores, space-separated (decimal, may be negative), or `"*"` when `InProgress` |

**Quoting:** value is double-quoted. Tag values support two escape sequences and only these two: `\"` for a literal `"`, `\\` for a literal `\`. Encoder applies escapes when writing names; decoder unescapes when reading. No other backslash sequences are recognized — `\n`, `\t`, etc. inside a tag value are a `DecodeError::BadTag`. This keeps `encode` total over any `Game` state without inviting general-purpose escape parsing.

A single blank line separates the header from the first round.

### Round body

For each round (starting at `1`):

```
[Round "<N>"]
[Hand0 "<sorted-cards>"]
[Hand1 "<sorted-cards>"]
[Hand2 "<sorted-cards>"]
[Hand3 "<sorted-cards>"]
[Bets "<bets>"]

<T>. <c1> <c2> <c3> <c4>
...
```

- `Hand{N}` lists the player's dealt hand at start of the round, **sorted** by `Card::Ord` (suit ascending, rank ascending), space-separated.
- `Bets` lists 0..=4 integers in seat order (seat 0 first; bets always start at seat 0). Empty string `""` if no bets placed yet.
- Trick lines are numbered `1.`..`13.` and contain 0..=4 cards in **play order** (lead first). Lead seat is derivable: seat 0 for trick 1 of any round; otherwise winner of prior trick (computed via `cards::get_trick_winner`).
- Trailing trick may be partial when the game is mid-trick.
- A round with no tricks played at all has no numbered trick lines (just header + hands + bets).
- A single blank line separates rounds.

### Card notation

Two ASCII chars: rank then suit.

- Ranks: `2 3 4 5 6 7 8 9 T J Q K A`
- Suits: `C D H S`

Examples: `2C`, `TC` (ten of clubs), `AS` (ace of spades), `KD`.

### Mid-game serialization

The format encodes any reachable `Game` state:

- **NotStarted:** header only, `Termination "InProgress"`, `Result "*"`, no round blocks.
- **Betting(k):** the current round block has `Bets` with `k` entries and no trick lines.
- **Trick(k):** the current round block has `Bets "..."` (4 entries) and trick lines for completed tricks plus a final partial line with the cards played so far in play order.
- **Completed/Aborted:** all completed rounds plus the in-progress round if termination happened mid-round.

## Public API

```rust
pub mod transcript {
    pub fn encode(game: &Game) -> String;
    pub fn decode(text: &str) -> Result<Transcript, DecodeError>;
    pub fn replay(t: &Transcript) -> Result<Game, ReplayError>;

    pub struct Transcript {
        pub headers: Headers,
        pub rounds: Vec<Round>,
        pub termination: Termination,
        pub result: Option<(i32, i32)>, // None when InProgress
    }

    pub struct Headers {
        pub game_id: Uuid,
        pub max_points: i32,
        pub player_ids: [Uuid; 4],
        pub names: [Option<String>; 4],
        pub timer: Option<TimerConfig>,
    }

    pub struct Round {
        pub hands: [Vec<Card>; 4], // canonical sorted order
        pub bets: Vec<i32>,        // 0..=4 in seat order
        pub tricks: Vec<Vec<Card>>, // each inner Vec has 1..=4 cards in play order; last may be partial
    }

    pub enum Termination { Completed, Aborted, InProgress }

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
        TrailingContent { line: usize },
    }

    pub enum ReplayError {
        HandMismatch { round: usize, seat: usize },          // declared hand doesn't match the cards played
        TransitionError { round: usize, trick: Option<usize>, seat: usize, err: TransitionError },
        TerminationMismatch { declared: Termination, actual: Termination },
        ResultMismatch { declared: (i32, i32), actual: (i32, i32) },
    }
}
```

`encode` is total: every valid `Game` produces a valid transcript. `decode` checks syntax only. `replay` drives moves through `Game::play`; any rule violation becomes a `ReplayError::TransitionError`.

## Determinism guarantees

- Tag order fixed by encoder; no alphabetization variance.
- Optional tags omitted entirely when `None` (never written as empty).
- Hands sorted by `Card::Ord` before emission.
- Trick cards in play order (lead first).
- LF only, single trailing newline.
- **Encoder idempotence:** `encode(replay(decode(s))?) == s` for any well-formed transcript.
- **Engine round-trip:** for any `Game` reachable through legal play, `replay(decode(encode(g))?)?` produces a `Game` observationally equal to `g` (same state, scores, hands, current player, bets, last trick, spades_broken).

## Edge cases & decisions

- **Lead seat derivation:** seat 0 leads trick 1 of every round (engine guarantees `current_player_index = 0` on betting→trick transition and on new-round betting). For trick `T>1`, lead = winner of trick `T-1` computed by `get_trick_winner(prev_lead, by_seat)`.
- **Bet order:** always seat 0, 1, 2, 3 in order. Confirmed in `lib.rs:382-392` and `lib.rs:455-462`.
- **Aborted state:** `Game::set_state` allows external transition to `Aborted`. Encoder treats it as terminal; replay verifies declared termination matches.
- **Spades broken:** not recorded; replay reconstructs it from the play sequence.
- **Player clocks / turn-started-at-epoch:** intentionally omitted. These are runtime/transport concerns, not part of the canonical history.
- **Deck remainder:** the unplayed-deck pile is internal mechanism, not part of history.
- **Tag value escaping:** only `\"` and `\\` are recognized in tag values. Encoder applies these escapes; decoder reverses them. Any other backslash is a decode error.
- **`Result` separator:** scores are space-separated rather than hyphen-separated because scores may be negative (failed bid penalties), and `format!("{}-{}", -60, 62)` produces the unambiguous-looking but unparseable `"-60-62"`. Space separation sidesteps the issue.

## Testing plan

### Encoder unit tests
- Hand-crafted `Game` states produce exact-string golden output for each of: NotStarted, mid-betting, mid-trick (1 card, 2 cards, 3 cards in current trick), end-of-round (just before next bet), multi-round (≥3 rounds), Aborted, Completed.
- Sort order: hands emitted sorted regardless of internal order.
- Optional tag omission: `Name0 = None` produces no `[Name0]` line; only `Name2` set → only `[Name2]` line emitted.
- Timer omission: no `TimerConfig` → no `TimerInitial` / `TimerIncrement` lines.
- Name escaping: a name containing `"` and `\` round-trips through encode and decode unchanged.

### Decoder unit tests
- Each golden fixture decodes to the structurally expected `Transcript`.
- Malformed input rejections, one test per variant:
  - bad rank (`"1C"`), bad suit (`"AX"`), wrong card length (`"AKS"`),
  - missing required tag, duplicate tag,
  - duplicate `[Round "1"]`, non-monotonic round (`1` then `3`),
  - 14th trick line, 5th bet, 5th card in a trick,
  - bad UUID, bad integer, bad result format,
  - trailing content after final round.

### Replay unit tests
- Each golden transcript replays into a `Game` equal to the encoder's source (observational equality helper).
- Mutated transcripts surface semantic errors:
  - swap a card the player doesn't hold → `HandMismatch` or `TransitionError(CardNotInHand)`,
  - off-suit when player can follow → `TransitionError(CardIncorrectSuit)`,
  - spade lead before broken → `TransitionError(SpadesNotBroken)`,
  - declared `Completed` but transcript ends mid-trick → `TerminationMismatch`.

### Round-trip property test
- Generate N=100 random valid games using a seeded `StdRng` (so the test itself is deterministic). For each: assert `encode(replay(decode(encode(g))?)?) == encode(g)`. Cover games that end via team-A win, team-B win, and that include ≥1 successful nil, ≥1 failed nil, and ≥1 bag-penalty crossing.

### Mid-game coverage
- For each reachable `State` variant (NotStarted, Betting(0..=3), Trick(0..=3) at multiple round indices, Completed, Aborted), assert encode → decode → replay yields the same state.

Target: ~25 unit tests + 1 property loop.

## Out of scope (future work)

- Binary encoding.
- Comments / annotations.
- Streaming encode/decode.
- Backwards-compatible upgrades (the format is versioned implicitly via crate version).
