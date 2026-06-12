# Game-table polish: adaptive hand spread, turn cues — design

Date: 2026-06-11
Scope: in-game table view (`web/src/routes/game-view.ts`, `web/src/cards/`, `web/src/ui/design.css`)

## Problems

1. Hand cards overlap by a fixed amount (24px visible strip per card) regardless of available
   width — 13 cards use ~334px of the ~670px south row, and the fan stays cramped even as the
   hand shrinks.
2. There is no clear signal when it becomes the player's turn: the only cue is a 2px outline on
   the seat chip, with no center-text prompt during the playing phase and no audio.

## Design

### 1. Adaptive hand spread

- `HandManager` computes per-card overlap whenever the south hand changes or its container
  resizes (one `ResizeObserver`, registered in `setContainers`, disposed in `clear`).
- The computation is a pure exported function:
  `computeHandOverlap(containerWidth, cardWidth, count)` =
  `(containerWidth − count·cardWidth) / (count − 1)`, clamped to
  `[−(cardWidth − 24), 4]` — floor preserves today's 24px visible strip; the 4px ceiling keeps
  small endgame hands from scattering. `count ≤ 1` returns `0`.
- Card width is measured from the first card element (`offsetWidth`), so the responsive card
  sizes (46/40/36px) need no breakpoint-specific logic.
- The result is written to a CSS custom property `--hand-ml` on the hand container.
  `design.css` replaces the hardcoded `-22px` / `-16px` hand margins with
  `margin-left: var(--hand-ml, -22px)` (first child stays `0`), and adds `margin-left` to the
  existing card transition so re-spacing animates as cards leave the hand.
- The orchestrator/animation system is untouched: it measures card rects at execution time
  (per the game-event invariants), so spacing changes require no queue awareness.

### 2. Visual turn cue (layered)

- **Playable cards**: `.cm-clickable` (already applied by the orchestrator only to valid plays
  on the player's turn) gains a 2px accent edge along the card top and a 6px rise. The rise
  uses `top: -6px` (cards are `position: relative`), NOT `transform` — inline transforms belong
  to the animation system. Accent usage is consistent with the "accent = interactive" rule.
- **Center felt text**: during `PLAYING`, when it is the player's turn, the table center text
  reads "Your turn" (currently empty in that state; `BETTING` already shows "Place your bet!").
  The element is already `aria-live="polite"`, so the turn is announced to screen readers.
- **Seat chip pulse**: `.seat-south.active` runs a brief outline/box-shadow pulse animation
  (~3 beats, then settles to the existing solid outline) each time the active class is applied.
  Suppressed under `prefers-reduced-motion: reduce`, as is the card-lift transition.

### 3. Audio chime

- New module `web/src/lib/sound.ts`: lazily-created `AudioContext`; `chime()` synthesizes two
  soft sine notes (gentle gain envelope, ~300ms total, low volume). All failures — including
  browser autoplay policy before any user gesture — are swallowed silently.
- A signal effect in `game-view.ts` detects the "became my turn" edge (previous ≠ mine →
  current = mine) during `BETTING` or `PLAYING` and calls `chime()`. Edge state starts as
  "not my turn", so loading into your own turn chimes once (or is silently blocked).
  The effect's dispose is pushed onto `resources.cleanups` like its siblings.
- Settings page (`web/src/routes/settings.ts`) gains a "Turn sound" on/off toggle persisted to
  localStorage, default **on**; `chime()` reads the preference before playing.

### 4. Testing & accessibility

- Unit tests: `computeHandOverlap` (clamp floor/ceiling, count 0/1/13, narrow widths) and the
  turn-edge helper (no chime on repeat states, chime on edge, phase gating).
- No e2e changes expected: no DOM restructuring; hand cards remain direct children of the hand
  container, so existing selectors and stability checks hold.
- Reduced motion: pulse animation and lift transition gated behind `prefers-reduced-motion`.

## Out of scope

- Opponent fan spacing (north/east/west stacks keep fixed overlap).
- Any sound beyond the single turn chime; no volume control (toggle only).
- Other views (lobby, home, scores bar).

## Decisions log

- Spread mechanism: JS-measured (ResizeObserver + CSS variable) chosen over CSS container
  queries (would force seat-south grid restructuring) and flex-shrink wrappers (would interfere
  with orchestrator container moves).
- Turn cue: layered (cards + center text + chip pulse) per user choice.
- Audio: WebAudio synth with settings toggle, default on, per user choice.
