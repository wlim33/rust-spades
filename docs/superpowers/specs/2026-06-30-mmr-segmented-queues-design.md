# MMR Segmented Matchmaking Queues — Design

**Date:** 2026-06-30
**Status:** Approved design, pre-implementation

## Context

The live site has a tiny, mostly-bot player base; the public leaderboard is currently
100% bots. On 2026-06-29 we shipped a `bot_seeker_cap: 3` fix (spades-bots) that stops
bots self-matching: the server matchmaker forms a game the instant 4 seeks share a bucket
(FIFO, no human priority), so keeping ≤3 bots waiting in the single quickplay bucket means
the 4th seat must be a human. That works but gives only one undifferentiated queue.

We want two things at once: a **relatively large, lively-looking lobby** spread across skill
tiers, and **skill-appropriate games** for humans who join. The approach: replicate the
`cap=3` "table-minus-one" invariant **per rating band**. Several tiers, each stocked with
~3 same-tier bots that play at that tier's strength, so a human joining any tier completes
a fair table and the lobby looks populated across tiers — with no bot-vs-bot games.

## Goals

- Busy multi-tier lobby (perception of an active player base).
- Skill-appropriate human matches: a human is matched with bots near their rating **and**
  those bots play at that tier's strength.
- No bot self-matching — the `cap=3` invariant holds in every band.

## Non-goals (v1)

- Segmenting by game length: quickplay is standardized on `max_points = 500`; custom lengths
  live in private challenges, not quickplay.
- Adjacent-band widening when a tier runs dry — rely on the per-band floor; revisit later.
- Excluding bots from the leaderboard — decided to **keep bots on the board** (stratified).
- Dedicated human-vs-human matchmaking changes — the unified band queue handles it: when
  multiple humans share a band they match each other (plus any same-band bots) naturally.

## Design

### Rating bands

Config boundaries `[1400.0, 1600.0]` → 3 bands:

| Band | Rating range |
|------|--------------|
| Low  | `< 1400`     |
| Mid  | `1400 – 1600` |
| High | `≥ 1600`     |

A `band_of(rating: f64) -> u8` helper plus the boundary constant live next to
`crates/spades-server/src/ratings.rs`. Boundaries are server config so they can be tuned
without a protocol change.

### Server — banded matching (rust-spades)

- **Seek carries rating.** `PendingSeek` (`crates/spades-server/src/matchmaking.rs:45`) gains
  `rating: f64`. The `seek` handler (`crates/spades-server/src/bin/server/handlers/matchmaking.rs`)
  already has `identity.user()`; it looks up the user's Glicko rating from the store
  (new/anonymous → `1500.0`, which lands in Mid) and passes it into `add_seek` (signature
  gains `rating`).
- **Match within band.** `try_match` (`matchmaking.rs:147`) changes its grouping key from
  `(max_points, timer_config)` to `(band_of(rating), max_points, timer_config)`. It forms a
  game only when 4 seeks share the same band + config; the FIFO take-4, create-and-start
  logic is otherwise unchanged. Because the controller keeps ≤3 bots per band, the 4th seat
  in any band must be a human → no bot self-match, per band.
- **Lobby data.** `list_seeks` / `queue_sizes` group by band so the API exposes per-tier
  waiting counts (`SeekSummary` gains a `band`, or a new per-band summary type).
- **Quickplay standardized on `max_points = 500`** so band is the sole segmentation and every
  human always finds same-tier bots (no empty length-buckets).

### Bots — per-band presence + strength (spades-bots, private repo)

- **Uniform banding.** The server derives a seeker's band from their *account rating* — bots
  included. A bot does not pass a band in its seek; instead each bot account is **seeded with
  a Glicko rating inside its assigned band** (Low ≈1300, Mid ≈1500, High ≈1700). When it seeks,
  the server bands it exactly like a human. No new seek-API field.
- **Rating seeding.** Bot accounts get their stored Glicko set to their band once at
  provisioning (they rarely play, so it stays put), via a rating-store write keyed by bot
  `user_id` — analogous to the existing pool provisioning step.
- **Per-band stocking.** The controller's `cap=3`/floor/diurnal logic
  (`controller.rs` `run_controller`, `floored_target`) becomes **per band**: maintain ~3
  seekers in each band (~9 total across 3 bands, comfortably within the 40-pool with rotation).
  Config shifts from a single `BOT_SEEKER_CAP=3` to a per-band target. The
  `≤3 per band → can't self-match` invariant holds in every band.
- **Per-band strength**, reusing the existing `mc`/`heuristic` + `BOT_MISTAKE_RATE` levers
  (and the per-seat mistake-rate machinery from the arena work):
  - Low (`<1400`): weak — `heuristic`, or `mc` with high mistake-rate (~0.30+)
  - Mid (`1400–1600`): `mc@0.12` (today's ~strong-human parity)
  - High (`≥1600`): near-perfect — `mc@~0.0`

### Web — tiered lobby (rust-spades web/)

Mostly a display change. The lobby route reads the new per-band counts and renders the 3
tiers (Low / Mid / High) with their waiting counts, highlighting the signed-in player's own
tier (computed from their rating). Seeking is unchanged from the user's side — they hit
"play," the server bands them by rating; no tier picker. Styling stays on the existing
design-system tokens (neutral; accent reserved for the interactive seek control, per the
web-color-restraint convention).

### Leaderboard

Keep bots on the board (decided). Seeded ratings stratify bots across tiers rather than
clustering them at the top.

## Reconciliation with the shipped `cap=3` fix

The shipped single-bucket `cap=3` generalizes to **per-band `cap=3`**. The spades-bots
`BOT_SEEKER_CAP` becomes a per-band target. The "table-minus-one waiting so a lone human
completes a game, while bots can't self-match" property is preserved — now in every band.

## Edge cases

- New/anonymous/unrated human → `1500` → Mid tier (Glicko default; high RD is fine).
- Underfilled band (overnight, or a human at a rating extreme) → the per-band floor keeps
  ~3 bots in every tier 24/7, so a human almost always has 3 same-tier bots. v1 has no
  adjacent-band widening; if a tier runs dry the human waits (revisit later).
- Hard boundaries (1399 vs 1401 never match) → accepted; it is the broad-tiers model.
- Bot daily caps still apply per band; with no self-match, per-bot volume stays low.

## Build order

1. **Server** (independently shippable): band helper + `rating` on the seek + banded
   `try_match` + per-band lobby data + standardize quickplay on 500. With current bots all
   at ~1500 they all land in Mid — correct behavior, just single-tier until bots are seeded.
2. **Bots** (spades-bots): seed ratings across bands + per-band stocking + per-band strength.
3. **Web**: tiered lobby display.

## Testing

- **Server unit tests** (extend the existing `matchmaking.rs` test module): `band_of`
  boundaries; `try_match` forms a game for 4 same-band seeks but not for 3-same-band + 1
  other-band; per-band `list_seeks` counts.
- **Bots**: per-band target stocking; band→strength mapping.
- **Web**: component test for the tiered lobby render; e2e — a rated user seeks and lands in
  the correct tier.
- **End-to-end manual**: a human at rating X matches 3 bots in `band_of(X)` that play at that
  tier's strength.

## Deferred / future

- Adjacent-band widening when a tier is dry.
- Provisional-rating refinement (down-weight high-RD new players).
- `max_points` variety via challenges.
