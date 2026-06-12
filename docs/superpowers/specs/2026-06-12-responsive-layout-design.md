# Responsive layout across monitor and window sizes — design

**Date:** 2026-06-12
**Status:** Approved (audit findings + all improvements approved verbatim; seat-rotation fix added on request)

## Problem

A live audit (code reading + Playwright at 9 viewport sizes) found the app is tuned for one
viewport class (~960×1000) and degrades in both directions:

| Viewport | Failure |
| --- | --- |
| ≤ ~1000px tall (any laptop with browser chrome, split screen) | Game page is 1017px tall during betting → **own hand + bid bar below the fold**; while playing, no scroll position shows scores/north and the hand together (377px overflow at 1280×640) |
| Large monitors (1440p/4K/ultrawide) | Table capped at 720px (~28% of 1440p width), cards fixed 46×64px, ~450px dead space under the footer |
| Tall screens, home route | Footer floats mid-screen (~330px of void below) |
| ≥1920px wide, home route | One 360px column; leaderboard preview queues under the menu in the same strip |

Root causes: fixed `max-width: 720px` everywhere (`--content-max` token exists but is unused);
fixed-px cards and card-face type; the east/west opponent fans are fixed-overlap stacks (~304px
tall) that dictate the table's center-row height; the bid bar renders below the table in document
flow; no height media queries; footer not pinned.

Separately: the client renders seat rotation counter-clockwise (`seatRel`:
`['south','east','north','west']`) while the server seats and names bots clockwise
(idx+1 = "West (CPU)"), so the bot named "East" appears on the west side and vice versa.

## Goals

1. The full game (scores, table, hand, bid bar) fits with **zero scrolling** in any window from
   ~560px tall up, at any width ≥ 360px.
2. Big monitors get a proportionally larger table and legible cards instead of margins.
3. No regression on the existing sweet spots (960×1010 half-snap, phones below 600px).
4. On-screen turn order matches the server's clockwise convention and bot names.
5. Keep the design-system rules: all styling via tokens, accent reserved for interactives.

## Non-goals (out of scope)

- Side-rail info density on ultrawide (score history / last trick panels) — possible phase 2.
- North fan adaptivity (13 backs ≈ 214px wide fits all supported widths).
- Server-side changes of any kind.

## Design

### A. Viewport-fit play screen (fixes short windows)

- `appShell(children, opts?: { fit?: boolean })` — `fit` adds `page--fit` to `.page` and omits
  the footer. Used by the in-game render (`game-view.ts`) and the play-route loading skeleton;
  lobby/error states stay normal pages.
- Height chain (all in `design.css`):
  - `body:has(.page--fit) { height: 100dvh; min-height: 0; }` (evergreen `:has()` is fine here)
  - `main#root` already `flex: 1` → add `min-height: 0` under the same `:has()` scope.
  - `.page--fit { flex: 1; min-height: 0; display: flex; flex-direction: column; }`
  - `.spades-table { flex: 1; min-height: 0; }` inside `.page--fit`, replacing the fixed
    `min-height: 480px` with a floor of `min-height: 24rem` — below that the page scrolls
    (today's behavior) instead of crushing the felt.
- **Bid bar and Play Again move into the felt center.** `gameTable()` gains a
  `centerExtra?: TemplateResult` slot rendered inside `.spades-table-center`, which becomes a
  column flex (trick area / center text / extra). `game-view.ts` passes `betButtons()` /
  `playAgain` there instead of appending after the table. Betting adds zero page height and the
  buttons appear where the "Place your bet!" prompt already is. The 7-col bid grid narrows to
  ~44px buttons inside the center column — at or above the `pointer: coarse` minimum.
- **Adaptive east/west fans** (the center row's height driver):
  - `computeHandOverlap(available, cardSize, count, minStrip)` grows an explicit `minStrip`
    parameter (default preserves today's south behavior; see B for scaling).
  - `HandManager` observes the west/east containers too, computes vertical overlap from
    container `clientHeight` and card `offsetHeight`, and publishes `--fan-mt` on each side
    container (mirror of the existing `--hand-ml` pattern). Vertical `minStrip` ≈ 10px — backs
    carry no index, they only need to read as a stack.
  - CSS: `.seat-east/.seat-west` get `align-self: stretch`; their `.opp-container` gets
    `flex: 1; min-height: 0; justify-content: safe center;` and
    `margin-top: var(--fan-mt, calc(var(--card-h) * -0.6875))` on cards (≡ today's −44px at
    64px card height). Fixed `-44px` / mobile `-40px` overrides are deleted.
- Compaction layer: `@media (max-height: 760px)` shrinks seat-chip padding, clock type
  (`--text-lg`), and `main#root` / `.page` block padding — token swaps only.

### B. Cards and table scale up on big monitors

- `tokens.css`:
  - `--card-w: clamp(46px, min(28px + 1.41vw, 10px + 5.5vh), 64px)` — 46px at ≤1280w,
    ~55px at 1920×1080, 64px at 2560w+; the `vh` term stops cards outgrowing short-wide windows.
  - `--card-h: calc(var(--card-w) * 64 / 46)` (preserves the 46:64 aspect).
  - `--table-max: 68.75rem` (1100px); `--content-max` bumped to `75rem` and **actually wired**:
    `.page { max-width: var(--content-max) }`.
- `design.css`:
  - `.card`, `.skeleton-card` use the tokens. Card-face type converts to em so faces scale with
    the card: `.card { font-size: calc(var(--card-w) * 0.348); }` → rank `1em`, corner suit
    `0.8125em`, pip `1.625em` (exactly today's 16/13/26px at 46px).
  - Phone breakpoints override only `--card-w` (40px / 36px); the height and face sizes derive.
  - North fan overlap becomes ratio-based: `margin-left: calc(var(--card-w) * -0.7)`.
  - `.spades-table`, `.spades-scores`, `.skeleton-game` max-width: `var(--table-max)`.
- `hand-layout.ts`: south `minStrip` scales with the card: `round(cardW * 24 / 46)` — identical
  at 46px, proportional above. (`MAX_GAP` stays 4px.)
- Safety: drag + orchestrator already measure `offsetWidth` / `getBoundingClientRect()` at
  execution time, and `HandManager` re-measures on resize, so token-driven sizes need no motion
  changes.

### C. Footer pinned to the viewport bottom

`.site-footer { margin-top: auto; }` — `main#root` is already a flex column. (Game route hides
the footer entirely via A.)

### D. Home uses desktop width

- `.home` becomes a single-column grid (`justify-items: center`) so the banner keeps its place;
  at `min-width: 1100px` it switches to two columns: menu left, leaderboard preview right
  (`align-items: start`, banner spans both).
- `.menu` and `.home-leaderboard` width: `clamp(22.5rem, 30vw, 26.25rem)` (360→420px).
- `.home-leaderboard`'s `margin-top` drops in two-column mode so the cards top-align.

### E. Token/hygiene

Covered by B (`--content-max` wired, fixed card numbers deleted); plus the dead fixed fan
margins removed in A. No other token changes.

### F. Clockwise seat rotation (east/west fix)

The server seats turn order clockwise (S → W → N → E viewed from your seat) and names bots
accordingly; the client renders it counter-clockwise. Fix the client mapping in two places so
they stay consistent:

- `state/helpers.ts` `seatRel`: `['south', 'east', 'north', 'west']` →
  `['south', 'west', 'north', 'east']`.
- `game-view.ts`: `west = (i + 3) % 4` / `east = (i + 1) % 4` swap to `west = (i + 1) % 4` /
  `east = (i + 3) % 4` (chips, `oppCardCount` calls, and orchestrator `westIdx`/`eastIdx` all
  read these two consts, so the swap is one edit site).

Result: the bot named "West (CPU)" sits on the west (left) side, and play visibly proceeds
clockwise like a real table. Trick-slot and animation code key off `seatRel`/these indices, so
they follow automatically.

## Testing

- **Unit:** `hand-layout.spec.ts` — new `minStrip` param + scaling behavior; `helpers.spec.ts` —
  `seatRel` clockwise mapping (update expectations).
- **Component:** `hand-manager.spec.ts` — side-fan `--fan-mt` published on count change
  (happy-dom has no ResizeObserver; same degradation as the existing `--hand-ml` path).
- **E2E:** existing `ai-game` flows exercise the bid bar via the `game.bet()` fixture — verify
  the fixture's selector still resolves with the bid bar inside the felt center; run `make e2e`.
- **Visual:** repeat the audit matrix (Playwright MCP) at 3440×1440, 2560×1440, 1920×1080,
  1280×640, 960×1010, 700×900, 550×750 — acceptance: no vertical scroll in-game at ≥ ~560px
  tall; cards ≥ 46px everywhere, ~64px at 2560; footer pinned; home two-column at ≥1100px;
  "West (CPU)" on the left.
- Full gate: `make check`.

## Rollout

Feature branch off `master`, one commit per design section, `make check` before merge.
Note: pushing to `master` auto-deploys (deploy.yml full gate + ship).
