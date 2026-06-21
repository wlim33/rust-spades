# Gameplay Juice + Turn-Plane Cue — Design

**Date:** 2026-06-20
**Scope:** In-game presentation only. Two changes: (1) the active-seat turn cue
moves from an accent **outline** to a **background-plane** fill in the seat's team
color, retiring the deprecated team keel on the seat chips; (2) card movement gains
a "tasteful spring" — flight overshoot-and-settle plus a landing impact and a
springy hand-card hover. Engine, server, and the web event/`seq` architecture are
untouched.

## Principle

Continues the state-cue migration
(`docs/superpowers/specs/2026-06-12-state-cue-tokens-design.md`): **state lives in
the background plane, identity in icons/position, borders are structure only.** The
active seat is "lit" in its team color; the accent outline (which also violated the
color-restraint rule by accenting non-interactive opponent seats) goes away. Motion
juice stays within the existing hand-rolled rAF tweener — no new dependency — and is
fully gated by the existing reduced-motion / hidden-tab / no-`rAF` skip, so
accessibility behavior is unchanged.

Two values are **tuning knobs**, to be set by eye during implementation: the
`backOut` overshoot constant and the on-felt wash alpha.

## 1. Tokens (`web/src/ui/tokens.css`)

New on-felt team washes, in the existing `/* State cues */` block:

```css
/* Active-seat "lit plane": a transparent team wash for the on-felt seat zone.
   Distinct from --team-*-fill, which is tuned as an opaque tint for chips on the
   cream raised surface; these sit over green felt, so they are low-alpha washes of
   the team hue. Alpha (~16%) is a tuning knob — set by eye on the felt. */
--team-1-glow: color-mix(in oklab, var(--team-1) 16%, transparent);
--team-2-glow: color-mix(in oklab, var(--team-2) 16%, transparent);
```

- Resolves through the dark theme automatically (`--team-1/2` re-resolve under
  `[data-theme="dark"]`); retune only if contrast checking demands it.
- The existing `--team-1` / `--team-2` keel tokens **stay**: the scoreboard
  (`.spades-scoreboard__team`) still consumes them and is out of scope here. Their
  deprecation comment already names `--team-*-fill` as the chip replacement; leave
  it. Removing the seat-chip keel (§2) does not orphan the tokens.

## 2. Active-seat plane (`web/src/ui/design.css`)

State now lives in the seat's background, not its border:

- **Drop the outline.** Remove `outline` / `outline-offset` from
  `.spades-seat.active`. Keep a `border-radius` on `.spades-seat` — the wash panel
  now uses it.
- **Light the zone.** `.spades-seat` gets a transparent default background and a
  `background-color` transition over `--dur-cue` (400ms, the gauge-motion duration)
  so the plane fades in like the lobby gauges, not a snap. `.spades-seat.active`
  fills with the team wash, keyed off the existing seat geometry:

  ```css
  .spades-seat { background: transparent;
    transition: background-color var(--dur-cue) var(--ease); border-radius: var(--radius-md); }
  .seat-north.active, .seat-south.active { background: var(--team-1-glow); }
  .seat-east.active,  .seat-west.active  { background: var(--team-2-glow); }
  ```

- **Fill the chip** in the readable cream tint (keeps `--fg` legible — that is what
  `--team-*-fill` was designed for), replacing today's `background: var(--surface)`
  shift on the active chip:

  ```css
  .seat-north.active .spades-seat-chip, .seat-south.active .spades-seat-chip { background: var(--team-1-fill); }
  .seat-east.active  .spades-seat-chip, .seat-west.active  .spades-seat-chip { background: var(--team-2-fill); }
  ```

  Keep `.spades-seat.active .spades-seat-label` bold.
- **Remove the keel.** Delete the `border-bottom-color: var(--team-1/2)` overrides
  on `.seat-* .spades-seat-chip`. The chip keeps its `border-bottom-width: 2px` at
  the neutral `--border` color — structure only. Team color now appears **only** as
  the lit plane; it stays persistently anchored in the scoreboard ("Team A/B" label
  + keel) and in seat position (N/S = Team A, E/W = Team B), so no identity is lost.
- **Your-turn pulse**, re-expressed in plane language. Replace the outline-offset
  `seat-pulse` keyframe with a one-shot brighten-and-settle of the wash on the
  south seat (always Team A), still gated under `prefers-reduced-motion`:

  ```css
  @media (prefers-reduced-motion: no-preference) {
    .seat-south.active { animation: turn-plane-pulse 700ms var(--ease) 2; }
    @keyframes turn-plane-pulse {
      50% { background-color: color-mix(in oklab, var(--team-1) 28%, transparent); }
    }
  }
  ```

  Settling to the steady `--team-1-glow` between/after iterations.

**Contrast check (do during implementation):** confirm `--fg` text on
`--team-*-fill` chips and the felt-ink seat labels over `--team-*-glow` stay legible
in **both** themes; nudge the mix percentages by eye if not.

## 3. Flight spring (`web/src/cards/animation.ts`, `web/src/cards/orchestrator.ts`)

- **Add a `backOut` ease** to the `EASE` map (overshoot then settle). Modest
  overshoot — `OVERSHOOT` is the knob (standard is 1.70158; start gentler):

  ```ts
  const OVERSHOOT = 1.2;            // tuning knob — higher = more overshoot
  const c3 = OVERSHOOT + 1;
  // EASE record gains:
  backOut: (t) => 1 + c3 * (t - 1) ** 3 + OVERSHOOT * (t - 1) ** 2,
  ```

  Extend the `EASE` Record's key union to include `'backOut'`.
- **Use it for the play flight.** In `flyToSlot`, swap `ease: 'quartOut'` →
  `ease: 'backOut'` and nudge `duration: 250 → 280` so the settle reads. This
  covers both the south player's plays and opponent plays (both route through
  `flyToSlot`). The flight only runs when `!skipAnims()`, so the spring is already
  reduced-motion-gated.

## 4. Landing impact (`web/src/ui/design.css`, `web/src/cards/orchestrator.ts`)

A one-shot scale-pop + shadow deepen as the card "lands" in its slot:

```css
@media (prefers-reduced-motion: no-preference) {
  .card.card-land { animation: card-land 160ms var(--ease); }
  @keyframes card-land {
    0%, 100% { transform: scale(1); }
    40%      { transform: scale(1.06); box-shadow: 4px 4px 0 rgb(var(--shadow-color) / 0.3); }
  }
}
```

- In `flyToSlot`, after `slotEl.style.visibility = ''`, add the class and remove it
  on `animationend` (one-shot): `slotEl.classList.add('card-land');
  slotEl.addEventListener('animationend', () => slotEl.classList.remove('card-land'), { once: true });`
- **Transform-composition note:** the keyframe animates `transform: scale(...)`,
  which overrides the slot card's inline `translate(0,0)`. Revealed slot cards rest
  at the origin (`_cm = {0,0}`), so the 160ms of lost translate is invisible and the
  inline transform reasserts on `animationend`. The pop fires **only** in
  `flyToSlot` — silent placements (`placeCardInTrick`, `setupImmediate`,
  `completeTrick` backfill) do not pop, which is correct: those are reconnect /
  catch-up paths, not live plays.

## 5. Hover spring (`web/src/ui/design.css`)

The playable-card hover lift springs instead of gliding linearly. Keep the resting
`top: -6px` + accent-edge interactive cue and the `top`/`margin-left`/`box-shadow`
transitions as they are; spring **only** the `transform`:

```css
@media (prefers-reduced-motion: no-preference) {
  .hand-container .card.cm-clickable {
    transition: transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1),
      margin-left var(--dur) var(--ease), top var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
  }
}
```

(The existing `.hand-container .card` transition stays as the reduced-motion / base
case; this overrides only the transform timing for clickable cards.)

## 6. Trick collect — deferred

The gather → sweep-to-winner → fade stays as-is in this pass to protect the
collect/backfill invariant (`.trick-container { position: relative }`, absolute-slot
positioning, the within-event table clear). An optional gather "inhale" scale-down
is noted as a future knob, not built here.

## Scope boundaries (not touched)

- Scoreboard keel (`.spades-scoreboard__team`), sound cues, and a new deal-in
  animation: **out of scope**. `--team-1/2` keel tokens stay.
- Untouched: WS event ordering + `seq` guard, the FIFO animation chain + `generation`
  invalidation, the POST-inside-play-step ordering, `disableInteraction` immediacy,
  the responsive height chain, `--card-w`/`--card-h` tokens, and
  `.trick-container { position: relative }`.
- `setPos` / the transform contract is unchanged (the land-pop is pure CSS).

## Testing

- `make check`: fmt-check + clippy + all tests. Rust crates untouched → the coverage
  baseline is unaffected.
- `tests/component/orchestrator.spec.ts` (queue ordering) stays green — the land-pop
  class toggle is synchronous and adds no queue step.
- ai-game e2e (origin-probe) stays green; it runs under `reducedMotion: 'reduce'`,
  so the spring/pop/pulse are all skipped there. The one animation e2e that opts back
  into motion still exercises the flight path.
- Manual `make dev`: play a hand and watch the flight overshoot-settle, the landing
  pop, the hand-hover spring, and the active-seat plane (including the south
  your-turn pulse) in **both light and dark themes** and once under
  `prefers-reduced-motion: reduce` (expect instant placement, steady plane, no pop).

## Out of scope / future migrations

- Scoreboard keels → background-plane cue.
- A soft "land" sound tick tied to the card-land moment (`lib/sound.ts` discipline).
- Deal-in animation at hand start; trick-collect gather "inhale."
