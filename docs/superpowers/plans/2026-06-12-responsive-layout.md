# Responsive Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the game fit any window ≥ ~560px tall with zero scrolling, scale cards/table up on large monitors, pin the footer, give the home page a two-column desktop layout, and render seat rotation clockwise to match the server.

**Architecture:** All layout via CSS tokens (`tokens.css`) + `design.css`; two pieces of TS logic change: `HandManager` learns to compress the east/west fans (mirror of the south `--hand-ml` pattern, publishing `--fan-mt`), and `appShell`/`gameTable` gain a `fit` option / `centerExtra` slot so the bid bar lives inside the felt. Seat rotation fix is a two-site swap (`seatRel` + `game-view` consts).

**Tech Stack:** lit-html templates, @preact/signals-core, plain CSS custom properties, vitest (unit + component, happy-dom), Playwright e2e, `make check` gate.

**Spec:** `docs/superpowers/specs/2026-06-12-responsive-layout-design.md`

---

### Task 1: Clockwise seat rotation (spec F)

**Files:**
- Modify: `web/src/state/helpers.ts` (seatRel array)
- Modify: `web/src/routes/game-view.ts:74-76` (west/east consts)
- Test: `web/tests/unit/helpers.spec.ts:53-58`

- [ ] **Step 1: Update the seatRel test to the clockwise mapping (failing first)**

In `web/tests/unit/helpers.spec.ts`, replace the `seatRel` describe block:

```ts
describe('seatRel', () => {
  // Server seats turn order clockwise: from your seat, +1 sits to your LEFT
  // (west on screen), +3 to your right (east). Matches server bot names.
  it('south for self', () => expect(seatRel(2, 2)).toBe('south'));
  it('west for +1', () => expect(seatRel(3, 2)).toBe('west'));
  it('north for +2', () => expect(seatRel(0, 2)).toBe('north'));
  it('east for +3', () => expect(seatRel(1, 2)).toBe('east'));
});
```

- [ ] **Step 2: Run to verify the two swapped cases fail**

Run: `pnpm -C web vitest run --project=unit tests/unit/helpers.spec.ts`
Expected: FAIL — `west for +1` gets `'east'`, `east for +3` gets `'west'`.

- [ ] **Step 3: Swap the mapping in helpers.ts**

In `web/src/state/helpers.ts`:

```ts
export function seatRel(absIdx: number, myIdx: number): RelativeSeat {
  const rel = (((absIdx - myIdx) % 4) + 4) % 4;
  return (['south', 'west', 'north', 'east'] as const)[rel]!;
}
```

- [ ] **Step 4: Swap the inline consts in game-view.ts**

In `web/src/routes/game-view.ts` (template fn, ~line 74), change:

```ts
    const north = (i + 2) % 4;
    const west = (i + 1) % 4;
    const east = (i + 3) % 4;
```

And in the orchestrator-sync effect (~lines 227-235 and 244-255), swap the index math the same way wherever `west`/`east` are computed inline: `west: oppCardCount(phase, tricksDone, tableCards, (i + 1) % 4)`, `east: ... (i + 3) % 4`, `westIdx: (i + 1) % 4`, `eastIdx: (i + 3) % 4`, and the matching `updateOpponentCount('west', ... (i + 1) % 4)` / `('east', ... (i + 3) % 4)` calls.

- [ ] **Step 5: Check for other hardcoded mappings**

Run: `grep -rn "i + 1\|i + 3\|+ 1) % 4\|+ 3) % 4" web/src/ | grep -v node_modules`
Expected: only `game-view.ts` lines just edited (the lead-suit calc at ~line 306 uses absolute seat math via `currentSeat - n` — leave it). Also run `grep -rn "'east'\|'west'" web/tests/component/orchestrator.spec.ts web/tests/component/trick-manager.spec.ts | head` — those tests drive seats explicitly (no seatRel), expected unaffected.

- [ ] **Step 6: Run unit + component tests**

Run: `pnpm -C web test`
Expected: PASS (helpers suite green; orchestrator/trick-manager unaffected).

- [ ] **Step 7: Commit**

```bash
git add web/src/state/helpers.ts web/src/routes/game-view.ts web/tests/unit/helpers.spec.ts
git commit -m "fix(web): render seat rotation clockwise to match server seating"
```

---

### Task 2: Pin footer + wire --content-max (spec C, E)

**Files:**
- Modify: `web/src/ui/tokens.css:87` (`--content-max`)
- Modify: `web/src/ui/design.css` (`.site-footer`, `.page`)

- [ ] **Step 1: Bump and wire the token**

`web/src/ui/tokens.css`: change `--content-max: 60rem;` → `--content-max: 75rem;`

`web/src/ui/design.css` `.page` rule: change `max-width: 720px;` → `max-width: var(--content-max);`

(Route content keeps its own inner max-widths — menu 360px, lobby 480px, auth 24rem — so only the game-route children, which Task 3 widens deliberately, can use the extra room.)

- [ ] **Step 2: Pin the footer**

Add to the `.site-footer` rule in `design.css`:

```css
.site-footer {
  margin-top: auto; /* pin to viewport bottom; main#root is a flex column */
  ...existing declarations unchanged...
}
```

- [ ] **Step 3: Guard the game scores/table width (pre-Task-3 placeholder cap)**

`.spades-scores` and `.spades-table` keep `max-width: 720px` for now (Task 3 retokens them); verify nothing else used `.page`'s 720px: `grep -n "720px" web/src/ui/design.css` → expected remaining: `.spades-table`, `.spades-scores`, `.skeleton-game`.

- [ ] **Step 4: Run component tests (layout CSS is inert there — regression canary only)**

Run: `pnpm -C web test:component`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/tokens.css web/src/ui/design.css
git commit -m "fix(web): pin footer to viewport bottom; wire --content-max into .page"
```

---

### Task 3: Card + table scale tokens (spec B)

**Files:**
- Modify: `web/src/ui/tokens.css` (add `--card-w`, `--card-h`, `--table-max`)
- Modify: `web/src/ui/design.css` (card/face/skeleton/fan/table rules + phone breakpoints)
- Modify: `web/src/cards/hand-layout.ts` (minStrip derives from card width)
- Test: `web/tests/unit/hand-layout.spec.ts`

- [ ] **Step 1: Extend hand-layout tests (failing first)**

Append to `web/tests/unit/hand-layout.spec.ts` inside the describe:

```ts
  it('scales the minimum strip with card width above the 46px baseline', () => {
    // strip = round(64 * 24/46) = 33 -> floor -(64 - 33)
    expect(computeHandOverlap(200, 64, 13)).toBe(-31);
  });

  it('keeps the 24px strip floor for small cards', () => {
    expect(computeHandOverlap(0, 36, 13)).toBe(-12); // -(36 - 24)
  });

  it('accepts an explicit minStrip override (vertical fans)', () => {
    expect(computeHandOverlap(0, 64, 13, 10)).toBe(-54); // -(64 - 10)
  });
```

- [ ] **Step 2: Run to verify the new cases fail**

Run: `pnpm -C web vitest run --project=unit tests/unit/hand-layout.spec.ts`
Expected: FAIL — first new case returns `-40` (fixed 24 strip), third errors on arity (extra arg is fine in JS, so it returns `-40` too; the assertion fails).

- [ ] **Step 3: Implement minStrip derivation + parameter**

Replace `web/src/cards/hand-layout.ts` body:

```ts
/** Layout math for the hand fans. Pure — unit-tested in isolation. */

/** Baseline visible strip of an overlapped card at the 46px base card width. */
const BASE_STRIP = 24;
const BASE_CARD_W = 46;
/** Maximum air between fully spread cards, px — keeps endgame hands fan-like. */
const MAX_GAP = 4;

/**
 * Per-card overlap margin for a fan: spread to fill the container, clamped
 * between full compression (minStrip visible) and a small positive gap.
 * Works for either axis; pass height-based sizes for vertical fans.
 * minStrip defaults to the corner-index strip, scaled up with the card so
 * bigger cards keep their rank readable (floor: the 24px baseline).
 */
export function computeHandOverlap(
  containerSize: number,
  cardSize: number,
  count: number,
  minStrip: number = Math.max(BASE_STRIP, Math.round(cardSize * (BASE_STRIP / BASE_CARD_W))),
): number {
  if (count <= 1) return 0;
  const ideal = (containerSize - count * cardSize) / (count - 1);
  return Math.min(MAX_GAP, Math.max(-(cardSize - minStrip), ideal));
}
```

- [ ] **Step 4: Run unit tests**

Run: `pnpm -C web vitest run --project=unit tests/unit/hand-layout.spec.ts`
Expected: PASS (all 8 — existing 5 unchanged: at 46px and 40px the derived strip stays 24).

- [ ] **Step 5: Add the size tokens**

`web/src/ui/tokens.css`, after the `--gutter` line in `:root`:

```css
  /* cards & table sizing: 46px cards up to 1280w, ~55px at 1920, 64px at 2560+.
     The vh term keeps cards from outgrowing short-wide windows. */
  --card-w: clamp(46px, min(28px + 1.41vw, 10px + 5.5vh), 64px);
  --card-h: calc(var(--card-w) * 64 / 46);
  --table-max: 68.75rem;
```

- [ ] **Step 6: Convert card rules in design.css to tokens**

`.card` rule: `width: var(--card-w); height: var(--card-h);` plus add `font-size: calc(var(--card-w) * 0.348);` (= 16px at 46px).

Face typography rules become em-based (today's px at 46px exactly):

```css
.card-corner-rank {
  font-size: 1em;
  letter-spacing: -0.03em;
}
.card-corner-suit {
  font-size: 0.8125em;
  margin-top: 1px;
}
.card-pip {
  ...
  font-size: 1.625em;
}
```

`.skeleton-card`: `width: var(--card-w); height: var(--card-h);`

North fan: `.seat-north .opp-container .card { margin-left: calc(var(--card-w) * -0.7); }` (≡ −32.2px at 46px).

`.spades-table`, `.spades-scores`, `.skeleton-game`: `max-width: var(--table-max);`

Default south-hand fallback in `.hand-container` rules: replace both `-22px` literals with `calc(var(--card-w) * -0.478)` (≡ −22px at 46px) in `padding-left: max(0px, calc(-1 * var(--hand-ml, ...)))` and `margin-left: var(--hand-ml, ...)`.

- [ ] **Step 7: Collapse the phone breakpoints to token overrides**

In `@media (max-width: 600px)`: delete the `.card { width/height/font-size }`, `.skeleton-card`, `.card-corner-rank`, `.card-corner-suit`, `.card-pip`, and `.seat-north .opp-container .card { margin-left: -29px }` overrides; add instead:

```css
  :root {
    --card-w: 40px;
  }
```

In `@media (max-width: 360px)`: delete `.card`/`.skeleton-card` size overrides; add `:root { --card-w: 36px; }`.

Leave the `.seat-east/.seat-west ... margin-top` mobile override for Task 4 to delete (it goes adaptive there).

- [ ] **Step 8: Sanity-check no stray fixed card sizes remain**

Run: `grep -n "46px\|64px\|40px\|56px\|36px\|50px" web/src/ui/design.css`
Expected: no `.card`-related hits (other uses like icon sizes may remain — verify each is non-card).

- [ ] **Step 9: Run all web tests**

Run: `pnpm -C web test`
Expected: PASS — `card-face.spec.ts` and `card-el.spec.ts` test DOM structure, not CSS sizes; `hand-manager.spec.ts` uses the `|| 46` fallback (happy-dom offsetWidth=0) which is unchanged.

- [ ] **Step 10: Commit**

```bash
git add web/src/ui/tokens.css web/src/ui/design.css web/src/cards/hand-layout.ts web/tests/unit/hand-layout.spec.ts
git commit -m "feat(web): token-driven card and table sizing that scales with the monitor"
```

---

### Task 4: Adaptive east/west fans (spec A, fan half)

**Files:**
- Modify: `web/src/cards/hand-manager.ts`
- Modify: `web/src/ui/design.css` (side-fan rules)
- Test: `web/tests/component/hand-manager.spec.ts`

- [ ] **Step 1: Write the failing component test**

Append to the describe in `web/tests/component/hand-manager.spec.ts` (note: the shared `beforeEach` wires west/east to the same node as south; this test needs real separate containers):

```ts
  it('publishes --fan-mt vertical overlap on side-fan count changes', () => {
    document.body.innerHTML = '<div id="s2"></div><div id="w2"></div><div id="e2"></div>';
    const s2 = document.getElementById('s2') as HTMLDivElement;
    const w2 = document.getElementById('w2') as HTMLDivElement;
    const e2 = document.getElementById('e2') as HTMLDivElement;
    const hm2 = new HandManager();
    hm2.setContainers({ south: s2, north: s2, west: w2, east: e2, trick: s2 });
    hm2.setOpponentCount('west', 13);
    // happy-dom: clientHeight/offsetHeight are 0 -> full compression at the
    // 10px strip with the 64px fallback card height: -(64 - 10) = -54.
    expect(w2.style.getPropertyValue('--fan-mt')).toBe('-54px');
    expect(e2.style.getPropertyValue('--fan-mt')).toBe('');
    hm2.setOpponentCount('east', 5);
    expect(e2.style.getPropertyValue('--fan-mt')).toBe('-54px');
  });
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web vitest run --project=component tests/component/hand-manager.spec.ts`
Expected: FAIL — `--fan-mt` is `''`.

- [ ] **Step 3: Implement side-fan spacing in HandManager**

In `web/src/cards/hand-manager.ts`:

1. Add a constant under the imports:

```ts
/** Vertical fans show card backs only — no index to keep readable. */
const SIDE_MIN_STRIP = 10;
```

2. In `setContainers`, observe the side containers too:

```ts
    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObs = new ResizeObserver(() => {
        this.updateHandSpacing();
        this.updateFanSpacing('west');
        this.updateFanSpacing('east');
      });
      this.resizeObs.observe(containers.south);
      this.resizeObs.observe(containers.west);
      this.resizeObs.observe(containers.east);
    }
```

3. Add the mirror of `updateHandSpacing`:

```ts
  /** Measure and publish the per-card vertical overlap as --fan-mt on a side container. */
  private updateFanSpacing(seat: 'west' | 'east'): void {
    if (!this.containers) return;
    const container = this.containers[seat];
    const cardH = this.hands[seat][0]?.el.offsetHeight || 64;
    const mt = computeHandOverlap(
      container.clientHeight,
      cardH,
      this.hands[seat].length,
      SIDE_MIN_STRIP,
    );
    container.style.setProperty('--fan-mt', `${mt}px`);
  }
```

4. In `setOpponentCount`, after the add/remove logic, end with:

```ts
    if (seat === 'west' || seat === 'east') this.updateFanSpacing(seat);
```

- [ ] **Step 4: Run the component test**

Run: `pnpm -C web vitest run --project=component tests/component/hand-manager.spec.ts`
Expected: PASS.

- [ ] **Step 5: Switch the side-fan CSS to the variable + stretch the seats**

In `web/src/ui/design.css`:

Replace the fixed side-fan margins (desktop rules ~line 1097 and the 600px-breakpoint override):

```css
.seat-east .opp-container .card,
.seat-west .opp-container .card {
  margin-top: var(--fan-mt, calc(var(--card-h) * -0.6875)); /* ≡ -44px at 64px */
}
```

(delete the `@media (max-width: 600px)` `.seat-east/.seat-west ... margin-top: -40px` block; `:first-child { margin-top: 0 }` rules stay.)

Stretch the side seats so the fan container knows its available height:

```css
.seat-west {
  align-self: stretch;
}
.seat-east {
  align-self: stretch;
  justify-self: end;
}
.seat-east .opp-container,
.seat-west .opp-container {
  flex-direction: column;
  flex: 1;
  min-height: 0;
  justify-content: safe center;
}
```

(These replace the existing `.seat-west { align-self: center }` / `.seat-east { align-self: center; justify-self: end }` rules and extend the existing `.opp-container` column rule.)

- [ ] **Step 6: Run all web tests**

Run: `pnpm -C web test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src/cards/hand-manager.ts web/src/ui/design.css web/tests/component/hand-manager.spec.ts
git commit -m "feat(web): east/west fans compress adaptively like the south hand"
```

---

### Task 5: Viewport-fit game screen + bid bar in the felt (spec A, shell half)

**Files:**
- Modify: `web/src/ui/templates.ts` (appShell opts)
- Modify: `web/src/ui/components/game-table.ts` (centerExtra slot)
- Modify: `web/src/routes/game-view.ts` (fit shell, centerExtra wiring)
- Modify: `web/src/routes/play.ts` (skeleton uses fit shell)
- Modify: `web/src/ui/design.css` (height chain, center column, compaction)

- [ ] **Step 1: appShell gains a fit option**

Replace `web/src/ui/templates.ts`:

```ts
import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';
import { footer } from './components/footer';
import { toastStack } from './components/toast';

export type AppShellOpts = {
  /** Game screens: page fills 100dvh, no footer, no page scroll. */
  fit?: boolean;
};

export function appShell(children: TemplateResult, opts: AppShellOpts = {}): TemplateResult {
  return html`<div class="app-shell">
    ${header()}
    <section class="page${opts.fit ? ' page--fit' : ''}">${children}</section>
    ${opts.fit ? null : footer()} ${toastStack()}
  </div>`;
}
```

- [ ] **Step 2: gameTable gains a centerExtra slot**

In `web/src/ui/components/game-table.ts`, extend the args type and center markup:

```ts
export function gameTable(args: {
  north: SeatProps;
  west: SeatProps;
  east: SeatProps;
  south: SeatProps;
  centerText: string;
  centerExtra?: TemplateResult | null;
  refs: GameTableRefs;
}): TemplateResult {
```

and in the returned template:

```ts
    <div class="spades-table-center">
      <div class="spades-trick-area">
        <div class="card-container trick-container" ${ref(args.refs.trick)}></div>
      </div>
      <span class="spades-center-text" aria-live="polite" aria-atomic="true"
        >${args.centerText}</span
      >
      ${args.centerExtra ?? null}
    </div>
```

- [ ] **Step 3: game-view renders fit shell with bid bar / play-again in the center**

In `web/src/routes/game-view.ts`, three mechanical edits to the template return (every
`scores(...)` and seat-props argument keeps its current content):

1. `appShell(html\`...\`)` → `appShell(html\`...\`, { fit: true })`.
2. Inside the `gameTable({ ... })` call, add one property after `centerText`:
   `centerExtra: store.phase.value === 'GAME_OVER' ? playAgain : betButtons(),`
3. Delete the trailing `${betButtons()} ${playAgain}` interpolation after the `gameTable` call.

(`${betButtons()} ${playAgain}` after the table is removed; `playAgain` stays defined as-is — it renders `html\`\`` when not game-over, and betButtons() renders `html\`\`` outside betting, so passing both through one slot is safe: exactly one is ever non-empty.)

- [ ] **Step 4: play.ts loading skeleton uses the fit shell**

In `web/src/routes/play.ts`, the pre-render call becomes `render(appShell(html\`...skeleton...\`, { fit: true }), root);` (error and lobby renders stay default).

- [ ] **Step 5: The CSS height chain + center column + compaction**

In `web/src/ui/design.css`:

In the `.spades-table` rule, change `min-height: 480px;` → `min-height: 24rem;` (the scroll-fallback floor; all other declarations, including `max-width: var(--table-max)`, stay). Then add after the `.page` rule:

```css
/* Game screens fill the viewport exactly; below the table's floor the page
   scrolls as before. :has() scopes the constraint to fit routes only. */
body:has(.page--fit) {
  height: 100dvh;
  min-height: 0;
}
body:has(.page--fit) main#root {
  min-height: 0;
}
.page--fit {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding-block: var(--space-2);
}
.page--fit .spades-table {
  flex: 1;
  min-height: 0;
  width: 100%;
}
```

Make the felt center a column (replace the existing `.spades-table-center` rule):

```css
.spades-table-center {
  grid-area: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  color: var(--felt-ink);
  min-width: 0;
}
```

The bid grid inherits `width: 100%; max-width: 480px` — inside the center column that yields ~44px buttons at the default table width (≥ the coarse-pointer minimum).

Add the short-window compaction at the end of the media-query section:

```css
/* Short windows: trade chrome for felt. */
@media (max-height: 760px) {
  main#root {
    padding-block: var(--space-2);
  }
  .spades-seat-chip {
    padding: var(--space-1) var(--space-2);
  }
  .spades-clock {
    font-size: var(--text-lg);
  }
  .spades-scores {
    padding: var(--space-1) 0;
  }
  .spades-table {
    padding: var(--space-3);
    gap: var(--space-2);
  }
}
```

- [ ] **Step 6: Run all web tests**

Run: `pnpm -C web test`
Expected: PASS (appShell change is backward-compatible; no component spec renders gameTable directly).

- [ ] **Step 7: Run the e2e suite (auto-starts backend)**

Run: `make e2e`
Expected: PASS — `game-page.ts` locates `.spades-bets`, which moved inside `.spades-table-center` but kept its class; Playwright auto-scrolls regardless.

- [ ] **Step 8: Commit**

```bash
git add web/src/ui/templates.ts web/src/ui/components/game-table.ts web/src/routes/game-view.ts web/src/routes/play.ts web/src/ui/design.css
git commit -m "feat(web): viewport-fit game screen with the bid bar inside the felt"
```

---

### Task 6: Two-column home on desktop (spec D)

**Files:**
- Modify: `web/src/ui/design.css` (`.home`, `.menu`, `.home-leaderboard`)

- [ ] **Step 1: Home becomes a grid; two columns ≥ 1100px**

Replace the `.home` rule and add a wide-screen query next to it:

```css
.home {
  width: 100%;
  display: grid;
  grid-template-columns: minmax(0, auto);
  justify-content: center;
  justify-items: center;
}
@media (min-width: 1100px) {
  .home {
    grid-template-columns: auto auto;
    column-gap: var(--space-12);
    align-items: start;
  }
  .home > .banner {
    grid-column: 1 / -1;
  }
  .home-leaderboard {
    margin-top: 0;
  }
}
```

Widen both columns' clamp (replace `max-width: 360px` in `.menu` and `.home-leaderboard`):

```css
.menu {
  ...
  width: 100%;
  max-width: clamp(22.5rem, 30vw, 26.25rem);
}
.home-leaderboard {
  width: 100%;
  max-width: clamp(22.5rem, 30vw, 26.25rem);
  margin-top: var(--space-4);
}
```

(`.home::before/::after` grain overlay rules are untouched; the reveal animation keys off `.home .menu > *` and `.home-leaderboard`, also untouched.)

- [ ] **Step 2: Run home component test**

Run: `pnpm -C web vitest run --project=component tests/component/home.spec.ts`
Expected: PASS (DOM unchanged).

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/design.css
git commit -m "feat(web): two-column home layout on wide screens"
```

---

### Task 7: Full verification

**Files:** none (verification only; fixes commit under the relevant task's message)

- [ ] **Step 1: Full local gate**

Run (cargo needs `export PATH="$HOME/.cargo/bin:$PATH"`): `make check`
Expected: fmt + clippy + workspace tests + web tests all green.

- [ ] **Step 2: Visual matrix re-audit (Playwright MCP, live bot game)**

Start servers (`make backend` + `pnpm -C web dev`), play into a bot game, and verify at
3440×1440, 2560×1440, 1920×1080, 1280×640, 960×1010, 700×900, 550×750:

- No vertical scroll in-game at every size ≥ ~560px tall (betting AND playing phases).
- Cards ≈46px at ≤1280w, ≈55px at 1920×1080, ≈64px at 2560×1440.
- Bid buttons visible inside the felt without scrolling; tap targets ≥ ~44px.
- Footer pinned to the bottom on home/leaderboard/profile; absent in-game.
- Home: two columns at ≥1100px, single centered column below.
- The bot named "West (CPU)" renders on the LEFT; played cards sweep clockwise.
- Phone widths (≤600px): unchanged stacked layout, 40px cards.

- [ ] **Step 3: Re-run the audit measurements**

At 1280×640 in-game, `document.documentElement.scrollHeight === window.innerHeight` (was 1017 vs 640).

- [ ] **Step 4: Push branch**

```bash
git push -u origin responsive-layout
```

(Deploy fires only on `master`; the branch is safe to push.)
