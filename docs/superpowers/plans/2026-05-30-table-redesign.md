# In-Game Table Redesign (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the in-game table real me.uk card faces, a felt surface, live clocks, a token-styled bid bar, and a responsive layout — without disturbing the imperative card orchestrator's animations.

**Architecture:** Card faces become vendored me.uk CC0 SVGs served from `web/public/cards/` and rendered as `<img>` into the existing `.card` element via a shared `setCardFace(el, card)` helper used by both `card-el.ts` (hand/opponents) and `trick-manager.ts` (trick slots). The orchestrator/drag/animation/keyboard code is untouched. The felt, bid bar, hand fan, and responsive layout are CSS on the existing structure; clocks are a small `state/clocks.ts` ticking module wired into the seat chips.

**Tech Stack:** TypeScript, Vite (serves `public/` at `/`), lit-html, `@preact/signals-core`, vitest (unit=node, component=happy-dom). `svgo` via `pnpm dlx` (build-time only).

**Reference:** Spec `docs/superpowers/specs/2026-05-30-table-redesign-design.md`. Visual reference: `web/table-preview.html` (throwaway). Builds on Phase 0 (tokens in `web/src/ui/tokens.css`).

**TDD note:** Behavioral TS (`cardFaceUrl`, `setCardFace`, clocks, `bidBar`) is built test-first. Pure-asset and pure-CSS work (vendoring, felt, hand fan, responsive) is **[VERIFY]** — confirmed by `pnpm -C web build`, the suite staying green, and a visual check against the preview. Run all commands from the repo root; on branch `table-redesign`.

---

### Task 1: Vendor + optimize the me.uk card faces **[VERIFY]**

**Files:**
- Create: `web/public/cards/<RANK><SUIT>.svg` (52 files), `web/public/cards/SOURCE.md`

- [ ] **Step 1: Download, extract the 52 faces, optimize** — run from repo root:

```bash
mkdir -p web/public/cards
TMP=$(mktemp -d)
curl -s -m 60 -F "zip=Download zip file of SVG for web use" \
  "https://www.me.uk/cards/makeadeck.cgi" -o "$TMP/cards.zip"
head -c2 "$TMP/cards.zip" | grep -q PK || { echo "ERROR: not a zip"; exit 1; }
unzip -q "$TMP/cards.zip" -d "$TMP/cards"
# 52 faces only: ranks 2-9,T,J,Q,K,A × suits S,H,D,C (excludes backs 1B/2B and jokers 1J/2J)
cp "$TMP"/cards/[2-9TJQKA][SHDC].svg web/public/cards/
echo "faces: $(ls web/public/cards/*.svg | wc -l | tr -d ' ') (expect 52)"
# Optimize in place (preserves viewBox/preserveAspectRatio by default)
pnpm dlx svgo -q -f web/public/cards
echo "optimized total: $(du -sh web/public/cards | cut -f1)"
rm -rf "$TMP"
```

Expected: `faces: 52`, svgo reports per-file savings, and a smaller total. If `faces` ≠ 52, stop and report.

- [ ] **Step 2: Sanity-check a court still renders after svgo** — confirm a court SVG is still valid markup with a viewBox:

```bash
grep -l 'viewBox' web/public/cards/KH.svg && head -c 120 web/public/cards/KH.svg
```
Expected: `KH.svg` printed + an `<svg … viewBox=…>` opening. (Final visual check is in Task 3/8.)

- [ ] **Step 3: Record provenance** — create `web/public/cards/SOURCE.md`:

```markdown
# Playing-card faces

52 card faces from the me.uk SVG playing cards generator (CC0 / public domain;
court designs based on 19th-century Goodall & Son). No attribution required.

Source: https://www.me.uk/cards/  ·  GitHub: https://github.com/revk/SVG-playing-cards

Regenerate (default Goodall style):

    curl -F "zip=Download zip file of SVG for web use" https://www.me.uk/cards/makeadeck.cgi -o cards.zip

then keep `[2-9TJQKA][SHDC].svg`, drop backs/jokers, and run `svgo` over them.
The card *back* is not from this set — it's the app's CSS `.card-back` (teal hatch).
```

- [ ] **Step 4: Commit**

```bash
git add web/public/cards
git commit -m "feat(web): vendor me.uk CC0 card faces (svgo-optimized)"
```

---

### Task 2: `cardFaceUrl` mapping **[TDD]**

**Files:**
- Modify: `web/src/cards/card-el.ts` (add export)
- Test: `web/tests/unit/card-face.spec.ts`

- [ ] **Step 1: Write the failing test** — create `web/tests/unit/card-face.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { cardFaceUrl } from '../../src/cards/card-el';

describe('cardFaceUrl', () => {
  it('maps Ten of Hearts to /cards/TH.svg', () => {
    expect(cardFaceUrl({ rank: 'Ten', suit: 'Heart' })).toBe('/cards/TH.svg');
  });
  it('maps Ace of Spades to /cards/AS.svg', () => {
    expect(cardFaceUrl({ rank: 'Ace', suit: 'Spade' })).toBe('/cards/AS.svg');
  });
  it('maps number + court ranks and all suits', () => {
    expect(cardFaceUrl({ rank: 'Two', suit: 'Club' })).toBe('/cards/2C.svg');
    expect(cardFaceUrl({ rank: 'King', suit: 'Diamond' })).toBe('/cards/KD.svg');
    expect(cardFaceUrl({ rank: 'Jack', suit: 'Heart' })).toBe('/cards/JH.svg');
  });
});
```

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:unit -- card-face` (cannot import `cardFaceUrl`).

- [ ] **Step 3: Implement** — in `web/src/cards/card-el.ts`, add near the top (after the imports):

```ts
import type { Card, Rank, Suit } from '../state/helpers';

const RANK_FILE: Record<Rank, string> = {
  Two: '2', Three: '3', Four: '4', Five: '5', Six: '6', Seven: '7', Eight: '8',
  Nine: '9', Ten: 'T', Jack: 'J', Queen: 'Q', King: 'K', Ace: 'A',
};
const SUIT_FILE: Record<Suit, string> = { Spade: 'S', Heart: 'H', Diamond: 'D', Club: 'C' };

export function cardFaceUrl(card: Card): string {
  return `/cards/${RANK_FILE[card.rank]}${SUIT_FILE[card.suit]}.svg`;
}
```

(The existing `import type { Card } from '../state/helpers';` line should be merged into the new import that also pulls `Rank, Suit` — keep a single import.)

- [ ] **Step 4: Run, verify PASS** — `pnpm -C web test:unit -- card-face`.

- [ ] **Step 5: Commit**

```bash
git add web/src/cards/card-el.ts web/tests/unit/card-face.spec.ts
git commit -m "feat(web): cardFaceUrl maps a Card to its vendored SVG path"
```

---

### Task 3: Render me.uk faces (card-el + trick-manager) **[TDD]**

**Files:**
- Modify: `web/src/cards/card-el.ts` (add `setCardFace`; use it in `createFront`/`setFront`; drop dead text-render code)
- Modify: `web/src/cards/trick-manager.ts` (use `setCardFace`; drop its `SUIT_COLOR`/`cardText`)
- Modify: `web/src/ui/design.css` (`.card-face` img; remove `.card-front::before`; drop dead `.card-red`/`.card-black`)
- Test: `web/tests/component/card-el.spec.ts` (replace text assertions), `web/tests/component/trick-manager.spec.ts` (if it asserts text)

- [ ] **Step 1: Rewrite the card-el tests for image faces** — replace the body of `web/tests/component/card-el.spec.ts` with:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { createFront, createBack, setFront } from '../../src/cards/card-el';

describe('card-el', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders a front card as an image face with the right src + aria-label', () => {
    const el = createFront({ suit: 'Heart', rank: 'Ace' });
    const img = el.querySelector('img.card-face') as HTMLImageElement;
    expect(img).not.toBeNull();
    expect(img.getAttribute('src')).toBe('/cards/AH.svg');
    expect(el.className).toContain('card-front');
    expect(el.getAttribute('aria-label')).toBe('Ace of Hearts');
    expect(el.getAttribute('role')).toBe('button');
  });

  it('back card has card-back class and no face image', () => {
    const el = createBack();
    expect(el.className).toContain('card-back');
    expect(el.querySelector('img')).toBeNull();
  });

  it('setFront swaps the face image + aria-label on an existing element', () => {
    const el = createBack();
    setFront(el, { suit: 'Diamond', rank: 'Two' });
    expect((el.querySelector('img.card-face') as HTMLImageElement).getAttribute('src')).toBe(
      '/cards/2D.svg',
    );
    expect(el.getAttribute('aria-label')).toBe('Two of Diamonds');
  });
});
```

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:component -- card-el` (no `img.card-face` yet).

- [ ] **Step 3: Implement face rendering in `card-el.ts`** — replace the suit/rank display maps and `cardText`/`createFront`/`setFront` with a shared `setCardFace`. The file should keep `CardPos`/`CardEl` types, `cardFaceUrl` (Task 2), `createBack`, `setPos`. New code:

```ts
function faceImg(card: Card): HTMLImageElement {
  const img = document.createElement('img');
  img.className = 'card-face';
  img.src = cardFaceUrl(card);
  img.alt = '';
  img.loading = 'lazy';
  img.draggable = false;
  return img;
}

/** Turn any `.card` element into a face-up card for `card`. Shared by the hand and the trick slots. */
export function setCardFace(el: CardEl, card: Card): void {
  el.className = 'card card-front';
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);
  el.replaceChildren(faceImg(card));
}

export function createFront(card: Card): CardEl {
  const el = document.createElement('div') as CardEl;
  el.setAttribute('role', 'button');
  setCardFace(el, card);
  el._cm = { x: 0, y: 0 };
  return el;
}

export function setFront(el: CardEl, card: Card): void {
  setCardFace(el, card);
}
```

Delete the now-unused `SUIT_SYMBOL`, `RANK_DISPLAY`, `SUIT_COLOR`, and `cardText` (verify with `grep -n 'cardText\|SUIT_COLOR\|RANK_DISPLAY\|SUIT_SYMBOL' web/src/cards/card-el.ts` → no matches). `createBack` and `setPos` are unchanged.

- [ ] **Step 4: Route trick slots through `setCardFace`** — in `web/src/cards/trick-manager.ts`: change the import on line 3 to `import { setCardFace, type CardEl } from './card-el';`, delete the `SUIT_COLOR` const (line 7), and replace the body of `fillNextSlot` after the `if (!slot) return null;` guard with:

```ts
    setCardFace(slot, card);
    const entry: TrickSlot = { card, seat, el: slot };
    this.filled.push(entry);
    return entry;
```

- [ ] **Step 5: CSS — image face fills the card; remove the old text corner** — in `web/src/ui/design.css`:
  - In the `.card` rule add `overflow: hidden;` (so the face clips to the radius).
  - **Delete** the entire `.card-front::before { … }` rule.
  - **Delete** the `.card-red { … }` and `.card-black { … }` rules (faces are colored by the SVG now; these classes are no longer applied).
  - Add:

```css
.card-face {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: fill;
  display: block;
  pointer-events: none;
}
```

- [ ] **Step 6: Update trick-manager test if needed** — open `web/tests/component/trick-manager.spec.ts`; if any assertion checks `textContent` / `card-red` / `card-black` on a filled slot, change it to assert the slot has an `img.card-face` with the expected `src` (mirror the card-el test). If it only checks slot counts/placeholders, leave it.

- [ ] **Step 7: Verify FAIL→PASS + full suite + lint** — `pnpm -C web test:component -- card-el` and `-- trick-manager` pass; then `pnpm -C web build && pnpm -C web test && pnpm -C web lint`. Build clean (no unused-symbol lint errors), all tests pass.

- [ ] **Step 8: Commit**

```bash
git add web/src/cards/card-el.ts web/src/cards/trick-manager.ts web/src/ui/design.css web/tests/component/card-el.spec.ts web/tests/component/trick-manager.spec.ts
git commit -m "feat(web): render me.uk SVG card faces in hand + trick"
```

---

### Task 4: Felt table surface + `--felt-edge` token **[VERIFY]**

**Files:**
- Modify: `web/src/ui/tokens.css` (add `--felt-edge`)
- Modify: `web/src/ui/design.css` (`.spades-table` felt)

- [ ] **Step 1: Add the `--felt-edge` token** — in `web/src/ui/tokens.css`, add to the light `:root` block (next to `--felt`/`--felt-ink`): `--felt-edge: #25564c;` and to the `[data-theme='dark']` block: `--felt-edge: #0d211c;`.

- [ ] **Step 2: Apply the felt to `.spades-table`** — in `web/src/ui/design.css`, replace the `.spades-table { … }` rule with (keeps the existing grid; adds felt bg, padding, radius, shadow, ink color):

```css
.spades-table {
  display: grid;
  grid-template-columns: 1fr 2fr 1fr;
  grid-template-rows: auto 1fr auto;
  grid-template-areas: 'north north north' 'west center east' 'south south south';
  gap: var(--space-3);
  width: 100%;
  max-width: 720px;
  min-height: 480px;
  padding: var(--space-4);
  border-radius: var(--radius-lg);
  color: var(--felt-ink);
  background:
    radial-gradient(120% 90% at 50% 38%, color-mix(in oklab, var(--felt) 78%, #fff 8%), var(--felt) 70%, var(--felt-edge));
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 0.07),
    inset 0 0 60px rgb(0 0 0 / 0.28),
    var(--shadow-3);
}
```

Also add `color: var(--felt-ink);` to `.spades-table-center` (so the center turn/status text reads on felt):

```css
.spades-table-center {
  grid-area: center;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--felt-ink);
}
```

- [ ] **Step 3: Verify build + suite + visual** — `pnpm -C web build && pnpm -C web test` (green). Then `pnpm -C web dev`, open a game view (or compare against `web/table-preview.html` at `http://localhost:8099/...` if still served): confirm the felt renders in light + dark, seat chips/cards read on it, and the center text is legible. Stop the dev server.

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/tokens.css web/src/ui/design.css
git commit -m "feat(web): felt table surface (light + dark)"
```

---

### Task 5: Live clocks **[TDD]**

**Files:**
- Create: `web/src/state/clocks.ts`
- Test: `web/tests/unit/clocks.spec.ts`
- Modify: `web/src/ui/components/game-table.ts` (SeatProps `+ low` + `+ clockFrac`; render clock + bar + `.low`)
- Modify: `web/src/routes/game-view.ts` (wire snapshot/ticker; pass clock fields)
- Modify: `web/src/ui/design.css` (`.spades-clock.low`, `.spades-clock-bar`)

- [ ] **Step 1: Write the failing test** — create `web/tests/unit/clocks.spec.ts`:

```ts
import { describe, it, expect, afterEach, vi } from 'vitest';
import { captureActiveClock, liveActiveMs } from '../../src/state/clocks';

describe('clocks', () => {
  afterEach(() => vi.restoreAllMocks());

  it('counts the active clock down from the captured snapshot', () => {
    let t = 1000;
    vi.spyOn(performance, 'now').mockImplementation(() => t);
    captureActiveClock(10_000);
    t = 4000; // 3s elapsed
    expect(liveActiveMs()).toBe(7000);
  });

  it('clamps at zero', () => {
    let t = 0;
    vi.spyOn(performance, 'now').mockImplementation(() => t);
    captureActiveClock(2000);
    t = 5000;
    expect(liveActiveMs()).toBe(0);
  });

  it('returns null when there is no active snapshot', () => {
    captureActiveClock(null);
    expect(liveActiveMs()).toBe(null);
  });
});
```

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:unit -- clocks`.

- [ ] **Step 3: Implement** — create `web/src/state/clocks.ts`:

```ts
import { signal } from '@preact/signals-core';

/** Active player's clock is shown in warning color at/below this. */
export const LOW_CLOCK_MS = 15_000;

/** Bumped by the ticker so subscribed renders refresh while a clock runs. */
export const clockTick = signal(0);

let snapshotMs: number | null = null;
let capturedAt = 0;
let timer: ReturnType<typeof setInterval> | null = null;

/** Record the server's active-clock value and when we received it. */
export function captureActiveClock(ms: number | null): void {
  snapshotMs = ms;
  capturedAt = performance.now();
}

/** Active player's remaining ms right now (null when no timed clock). */
export function liveActiveMs(): number | null {
  if (snapshotMs == null) return null;
  return Math.max(0, snapshotMs - (performance.now() - capturedAt));
}

export function startClockTicker(): void {
  if (timer != null) return;
  timer = setInterval(() => {
    clockTick.value = clockTick.value + 1;
  }, 250);
}

export function stopClockTicker(): void {
  if (timer != null) {
    clearInterval(timer);
    timer = null;
  }
}
```

- [ ] **Step 4: Run, verify PASS** — `pnpm -C web test:unit -- clocks`.

- [ ] **Step 5: Extend `SeatProps` + render clock/bar in `game-table.ts`** — change the `SeatProps` type to add two fields after `clockText`:

```ts
  clockText: string | null;
  low: boolean;
  clockFrac: number | null; // 0..1 for the active seat's bar; null = no bar
```

In the `chip` function, replace the clock line with the clock + low class + an optional progress bar:

```ts
    return html`<div class=${chipCls}>
      <span class="spades-seat-label">${p.name}</span>
      ${p.clockText
        ? html`<span class="spades-clock${p.low ? ' low' : ''}">${p.clockText}</span>`
        : null}
      ${p.clockFrac != null
        ? html`<span class="spades-clock-bar"><i style=${`width:${Math.round(p.clockFrac * 100)}%`}></i></span>`
        : null}
      <span class="spades-seat-info">${p.betInfo}</span>
    </div>`;
```

- [ ] **Step 6: Wire clocks in `game-view.ts`** — add imports:

```ts
import { formatClock } from '../state/helpers';
import { clockTick, captureActiveClock, liveActiveMs, startClockTicker, stopClockTicker, LOW_CLOCK_MS } from '../state/clocks';
```

(`formatClock` already exists in `helpers.ts` — do not re-implement it. Add it to the existing `../state/helpers` import.)

Inside `renderInGame`, before building `template`, add helpers and start the ticker:

```ts
  const timed = (): boolean => store.timerConfig.value != null;
  const clockFor = (absIdx: number): string | null => {
    if (!timed()) return null;
    if (store.playerIds.value[absIdx] === store.currentPlayerId.value) return formatClock(liveActiveMs());
    return formatClock(store.playerClocksMs.value?.[absIdx] ?? null);
  };
  const lowFor = (absIdx: number): boolean =>
    timed() &&
    store.playerIds.value[absIdx] === store.currentPlayerId.value &&
    (liveActiveMs() ?? Infinity) <= LOW_CLOCK_MS;
  const fracFor = (absIdx: number): number | null => {
    if (!timed() || store.playerIds.value[absIdx] !== store.currentPlayerId.value) return null;
    const initialMs = (store.timerConfig.value?.initial_time_secs ?? 0) * 1000;
    if (initialMs <= 0) return null;
    return Math.max(0, Math.min(1, (liveActiveMs() ?? 0) / initialMs));
  };
```

In `template()`, read the tick so the render effect refreshes each interval — add `void clockTick.value;` as the first line of `template`. Then set each seat's `clockText`/`low`/`clockFrac` (north/west/east/south) via `clockFor(idx)`, `lowFor(idx)`, `fracFor(idx)` — e.g. for north: `clockText: clockFor(north), low: lowFor(north), clockFrac: fracFor(north),` (replace the existing `clockText: null,`). Do the same for `west`, `east`, and `south` (use `i` for south).

After the `disposeCards` effect is created, capture the active clock when it changes and run the ticker:

```ts
  const disposeClock = effect(() => {
    captureActiveClock(store.activePlayerClockMs.value);
  });
  startClockTicker();
  args.resources.cleanups.push(disposeClock);
  args.resources.cleanups.push(stopClockTicker);
```

- [ ] **Step 7: CSS for the low state + bar** — append to `web/src/ui/design.css`:

```css
.spades-clock.low {
  color: var(--accent-2);
}
.spades-clock-bar {
  width: 100%;
  height: 3px;
  margin-top: 2px;
  border-radius: 2px;
  background: color-mix(in oklab, var(--fg) 14%, transparent);
  overflow: hidden;
}
.spades-clock-bar > i {
  display: block;
  height: 100%;
  background: var(--accent);
}
.spades-seat.active .spades-clock-bar > i {
  background: var(--accent);
}
```

- [ ] **Step 8: Verify build + suite + lint** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint`. All green (the existing `game-view`/`game-table` consumers compile with the new required `SeatProps` fields — confirm no other caller of `gameTable`/`SeatProps` is missing them; `grep -rn "clockText" web/src` to check).

- [ ] **Step 9: Commit**

```bash
git add web/src/state/clocks.ts web/tests/unit/clocks.spec.ts web/src/ui/components/game-table.ts web/src/routes/game-view.ts web/src/ui/design.css
git commit -m "feat(web): show per-seat clocks with active-player countdown + low warning"
```

---

### Task 6: Token-styled bid bar (Nil + 1–13) **[TDD]**

**Files:**
- Create: `web/src/ui/components/bid-bar.ts`
- Test: `web/tests/component/bid-bar.spec.ts`
- Modify: `web/src/routes/game-view.ts` (use `bidBar`)
- Modify: `web/src/ui/design.css` (`.spades-bet` button styling + `--nil`)

- [ ] **Step 1: Write the failing test** — create `web/tests/component/bid-bar.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from 'lit-html';
import { bidBar } from '../../src/ui/components/bid-bar';

describe('bidBar', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders Nil + 1..13 (14 buttons)', () => {
    render(bidBar({ onBet: () => {} }), document.getElementById('root')!);
    const btns = document.querySelectorAll('.spades-bet');
    expect(btns.length).toBe(14);
    expect(btns[0]!.textContent?.trim()).toBe('Nil');
    expect(btns[13]!.textContent?.trim()).toBe('13');
  });

  it('calls onBet with the chosen amount (Nil = 0)', () => {
    const onBet = vi.fn();
    render(bidBar({ onBet }), document.getElementById('root')!);
    (document.querySelector('.spades-bet') as HTMLButtonElement).click();
    expect(onBet).toHaveBeenCalledWith(0);
  });
});
```

- [ ] **Step 2: Run, verify FAIL** — `pnpm -C web test:component -- bid-bar`.

- [ ] **Step 3: Implement** — create `web/src/ui/components/bid-bar.ts`:

```ts
import { html, type TemplateResult } from 'lit-html';

export function bidBar(opts: { onBet: (amount: number) => void }): TemplateResult {
  return html`<div class="spades-bets">
    ${Array.from({ length: 14 }, (_, n) =>
      html`<button
        type="button"
        class="spades-bet${n === 0 ? ' spades-bet--nil' : ''}"
        @click=${() => opts.onBet(n)}
      >
        ${n === 0 ? 'Nil' : n}
      </button>`,
    )}
  </div>`;
}
```

- [ ] **Step 4: Run, verify PASS** — `pnpm -C web test:component -- bid-bar`.

- [ ] **Step 5: Use it in `game-view.ts`** — in `betButtons()`, replace the returned `html\`<div class="spades-bets">…</div>\`` (the `Array.from({ length: 14 }, …button(…))` block) with a call to the component. Add `import { bidBar } from '../ui/components/bid-bar';` and return:

```ts
      return bidBar({ onBet: (amount) => void onBet(amount) });
```

The `button` import in `game-view.ts` may now be unused (it's still used for "Play Again") — leave it if so; otherwise lint will flag it (remove only if `grep -n 'button(' web/src/routes/game-view.ts` shows no remaining uses).

- [ ] **Step 6: Style the bid buttons** — in `web/src/ui/design.css`, replace the `.spades-bet { padding: var(--space-2); }` rule with:

```css
.spades-bet {
  appearance: none;
  font: inherit;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  cursor: pointer;
  padding: var(--space-2) 0;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background: var(--surface-raised);
  color: var(--fg);
  transition:
    transform var(--dur) var(--ease),
    border-color var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.spades-bet:hover {
  border-color: var(--accent);
  color: var(--accent);
  transform: translateY(-2px);
}
.spades-bet--nil {
  color: var(--accent-2);
}
.spades-bet--nil:hover {
  border-color: var(--accent-2);
  color: var(--accent-2);
}
```

- [ ] **Step 7: Verify build + suite + lint** — `pnpm -C web build && pnpm -C web test && pnpm -C web lint`. Green.

- [ ] **Step 8: Commit**

```bash
git add web/src/ui/components/bid-bar.ts web/tests/component/bid-bar.spec.ts web/src/routes/game-view.ts web/src/ui/design.css
git commit -m "feat(web): token-styled bid bar with Nil"
```

---

### Task 7: Hand fan (index corners) + responsive **[VERIFY]**

**Files:**
- Modify: `web/src/ui/design.css`

- [ ] **Step 1: Overlap the hand to show index corners** — in `web/src/ui/design.css`, replace the `.hand-container { gap: 4px; }` rule with an overlap fan (later cards paint over earlier, so each card's top-left index stays visible; the last card shows fully):

```css
.hand-container {
  gap: 0;
  padding-left: 22px;
}
.hand-container .card {
  margin-left: -22px;
  transition: transform var(--dur) var(--ease);
}
.hand-container .card:first-child {
  margin-left: 0;
}
.hand-container .card.cm-clickable:hover {
  transform: translateY(-16px);
  z-index: 5;
}
```

- [ ] **Step 2: Tighten the fan + cards on small screens** — in the existing `@media (max-width: 600px)` block, add a tighter overlap so a 13-card hand fits (the `.card` size already steps down there):

```css
  .hand-container {
    padding-left: 16px;
  }
  .hand-container .card {
    margin-left: -16px;
  }
```

- [ ] **Step 3: Verify build + suite + visual** — `pnpm -C web build && pnpm -C web test` green. Then `pnpm -C web dev`: in a game, confirm the hand fans with readable top-left indices, the hovered card lifts above its neighbors, and a full 13-card hand fits at desktop and ~375px widths. Stop the dev server.

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/design.css
git commit -m "feat(web): index-corner hand fan + tighter mobile overlap"
```

---

### Task 8: Final verification & cleanup **[VERIFY]**

**Files:**
- Remove: `web/table-preview.html`, `web/cards-mockup/` (throwaway mockup artifacts)

- [ ] **Step 1: Full gate**

```bash
pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check
```
Expected: all green (60+ unit, 60+ component).

- [ ] **Step 2: e2e selector check** — the cards no longer contain text (faces are `<img alt="">`). Confirm no Playwright page object selects a card by text content:

```bash
grep -rnE "getByText\(|text=|textContent" web/tests/e2e || echo "no text-based card selectors"
```
If any select a card by its rank/suit text, update them to use the card's `aria-label` (e.g. `getByRole('button', { name: 'Ace of Spades' })`). Run `pnpm -C web test:e2e` with the Rust backend available (CI), or note it for CI if no backend here.

- [ ] **Step 3: Remove throwaway mockup artifacts**

```bash
rm -f web/table-preview.html
rm -rf web/cards-mockup
```

- [ ] **Step 4: Commit**

```bash
git add -A web/
git commit -m "chore(web): remove throwaway table mockup artifacts"
```

---

## Self-review

**Spec coverage:**
- §3/§4.1 vendor me.uk faces via the confirmed `curl -F "zip=…"`, `T`=10, svgo, `public/cards/` → Task 1.
- §4.1 `cardFaceUrl` mapping → Task 2.
- §4.2 `<img>` faces with preserved element contract (classes/role/aria/`._cm`), back unchanged, `.card-front::before` removed → Task 3 (covers **both** card-el AND trick-manager, which the spec's "rendered into the existing card element" implies and which the code requires).
- §4.3 felt + `--felt-edge` → Task 4.
- §4.4 clocks (`state/clocks.ts`, capture/live/ticker, `formatClock` reuse, low warning, wired into seats) → Task 5. (Spec said `fmtClock`; corrected to reuse existing `formatClock`.)
- §4.5 bid bar Nil + 1–13 → Task 6.
- §4.6 index-corner hand fan + responsive → Task 7.
- §4.7 animations untouched → guaranteed by routing faces through `setCardFace` on the same `.card` elements; the orchestrator/drag/animation/keyboard files are not modified.
- §5 token addition → Task 4 Step 1.
- §6 tests (cardFaceUrl, card-el image faces, clocks formatting, bid bar) + e2e selector check → Tasks 2,3,5,6,8.
- §7 risks: svgo sanity (T1 S2), bundle via `public/` + lazy `<img>` (T3), e2e card-text (T8).

**Placeholder scan:** No TBD/"handle errors"/"similar to". Every code/CSS step shows full content; commands have expected output. The two "remove only if lint flags it" notes (unused `button` import in game-view; dead symbols in card-el) are guarded by explicit `grep` checks, not guesses.

**Type consistency:** `cardFaceUrl(card)` and `setCardFace(el, card)` signatures consistent across Tasks 2/3 and trick-manager. `SeatProps` gains `low: boolean` + `clockFrac: number | null` in Task 5 and every `gameTable(...)` seat object in `game-view.ts` is updated to supply them. `captureActiveClock`/`liveActiveMs`/`startClockTicker`/`stopClockTicker`/`clockTick`/`LOW_CLOCK_MS` names consistent across Task 5 and the clocks test. `bidBar({ onBet })` consistent across Task 6.

**Note for executor:** Task 5 changes the `SeatProps` interface — after editing the type, the TypeScript build will error until all four seat objects in `game-view.ts` supply `low`/`clockFrac`; that's the intended compiler-driven checklist.
