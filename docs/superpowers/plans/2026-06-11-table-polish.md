# Game-Table Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adaptive hand-fan spacing plus a layered "your turn" cue (visual + center text + seat pulse + audio chime with a settings toggle).

**Architecture:** A pure layout function computes per-card overlap; `HandManager` writes it to a `--hand-ml` CSS variable via a `ResizeObserver`. Turn cues are CSS keyed off the existing `.cm-clickable` / `.active` classes plus a center-text change, and a WebAudio chime fired from a signal effect on the "became my turn" edge. Spec: `docs/superpowers/specs/2026-06-11-table-polish-design.md`.

**Tech Stack:** TypeScript, lit-html, @preact/signals-core, vitest (unit = node env, component = happy-dom), plain CSS with tokens from `web/src/ui/tokens.css`.

**Conventions for every task:**
- All commands run from the repo root. Web tests: `pnpm -C web exec vitest run --project=<unit|component> <file>`.
- Commit with explicit pathspecs (`git commit -m "…" -- <paths>`) — the repo often has unrelated WIP staged; a bare `git commit` would sweep it in.
- Styling uses only `tokens.css` variables — no raw hex. Accent color marks interactive elements only (playable cards qualify).
- Never set inline `transform`/`top` on hand cards from CSS-adjacent code: the animation system owns inline styles (`detachInteraction` clears them). All new styling is class/variable driven.

---

### Task 1: Pure hand-overlap function

**Files:**
- Create: `web/src/cards/hand-layout.ts`
- Test: `web/tests/unit/hand-layout.spec.ts`

- [ ] **Step 1: Write the failing test**

Create `web/tests/unit/hand-layout.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { computeHandOverlap } from '../../src/cards/hand-layout';

describe('computeHandOverlap', () => {
  it('caps the spread at a 4px gap on wide containers', () => {
    // ideal = (900 - 13*46) / 12 = 25.2 -> capped
    expect(computeHandOverlap(900, 46, 13)).toBe(4);
  });

  it('uses the exact fit when between the clamps', () => {
    expect(computeHandOverlap(500, 46, 13)).toBeCloseTo((500 - 13 * 46) / 12, 5);
  });

  it('never compresses below a 24px visible strip', () => {
    expect(computeHandOverlap(200, 46, 13)).toBe(-22); // -(46 - 24)
    expect(computeHandOverlap(0, 40, 13)).toBe(-16); // -(40 - 24), mobile card width
  });

  it('returns 0 for empty and single-card hands', () => {
    expect(computeHandOverlap(670, 46, 0)).toBe(0);
    expect(computeHandOverlap(670, 46, 1)).toBe(0);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/hand-layout.spec.ts`
Expected: FAIL — cannot resolve `../../src/cards/hand-layout`.

- [ ] **Step 3: Write the implementation**

Create `web/src/cards/hand-layout.ts`:

```ts
/** Layout math for the south hand fan. Pure — unit-tested in isolation. */

/** Minimum visible strip of an overlapped card, px (today's fixed overlap). */
const MIN_STRIP = 24;
/** Maximum air between fully spread cards, px — keeps endgame hands fan-like. */
const MAX_GAP = 4;

/**
 * Per-card margin-left for the hand fan: spread to fill the container,
 * clamped between full compression (24px strip) and a small positive gap.
 */
export function computeHandOverlap(
  containerWidth: number,
  cardWidth: number,
  count: number,
): number {
  if (count <= 1) return 0;
  const ideal = (containerWidth - count * cardWidth) / (count - 1);
  return Math.min(MAX_GAP, Math.max(-(cardWidth - MIN_STRIP), ideal));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/hand-layout.spec.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add web/src/cards/hand-layout.ts web/tests/unit/hand-layout.spec.ts
git commit -m "feat(web): pure hand-fan overlap computation" -- web/src/cards/hand-layout.ts web/tests/unit/hand-layout.spec.ts
```

---

### Task 2: Wire adaptive spacing into HandManager + CSS

**Files:**
- Modify: `web/src/cards/hand-manager.ts`
- Modify: `web/src/ui/design.css` (hand-container rules ~lines 346-356; mobile overrides ~lines 921-926)
- Test: `web/tests/component/hand-manager.spec.ts`

- [ ] **Step 1: Write the failing test**

Append to the `describe('HandManager', …)` block in `web/tests/component/hand-manager.spec.ts`:

```ts
  it('sets --hand-ml on the south container as the hand changes', () => {
    hm.setPlayerHand([c('Spade', 'Ace'), c('Spade', 'King')]);
    // happy-dom reports zero widths: 2 cards clamp to full compression (-22px)
    expect(south.style.getPropertyValue('--hand-ml')).toBe('-22px');
    hm.setPlayerHand([c('Spade', 'Ace')]);
    expect(south.style.getPropertyValue('--hand-ml')).toBe('0px');
  });

  it('updates --hand-ml when a card is removed via removeCard', () => {
    hm.setPlayerHand([c('Spade', 'Ace'), c('Spade', 'King')]);
    hm.removeCard(c('Spade', 'Ace'));
    expect(south.style.getPropertyValue('--hand-ml')).toBe('0px');
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/hand-manager.spec.ts`
Expected: FAIL — `--hand-ml` is `''`.

- [ ] **Step 3: Implement in HandManager**

In `web/src/cards/hand-manager.ts`:

Add the import at the top:

```ts
import { computeHandOverlap } from './hand-layout';
```

Add a field after `private hands: …`:

```ts
  private resizeObs: ResizeObserver | null = null;
```

Replace `setContainers` with:

```ts
  setContainers(containers: Containers): void {
    this.containers = containers;
    this.resizeObs?.disconnect();
    // happy-dom (component tests) has no ResizeObserver; spacing still updates
    // on every hand change, so only live-resize reactivity is lost there.
    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObs = new ResizeObserver(() => this.updateHandSpacing());
      this.resizeObs.observe(containers.south);
    }
  }
```

Add a private method after `setPlayerHand`:

```ts
  /** Measure and publish the per-card overlap as --hand-ml on the south container. */
  private updateHandSpacing(): void {
    if (!this.containers) return;
    const container = this.containers.south;
    const cardW = this.hands.south[0]?.el.offsetWidth || 46;
    const ml = computeHandOverlap(container.clientWidth, cardW, this.hands.south.length);
    container.style.setProperty('--hand-ml', `${ml}px`);
  }
```

At the end of `setPlayerHand` (after `this.hands.south = kept;`) add:

```ts
    this.updateHandSpacing();
```

In `removeCard`, before `return entry!.el;` add:

```ts
    this.updateHandSpacing();
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/hand-manager.spec.ts`
Expected: PASS (all existing + 2 new tests).

- [ ] **Step 5: Switch the CSS to the variable**

In `web/src/ui/design.css`, replace:

```css
.hand-container {
  gap: 0;
  padding-left: 22px;
}
.hand-container .card {
  margin-left: -22px;
  transition: transform var(--dur) var(--ease);
}
```

with:

```css
.hand-container {
  gap: 0;
  /* Balance the fan's overlap so it centers visually. --hand-ml is measured
     by HandManager; negative while cards overlap, small positive when spread. */
  padding-left: max(0px, calc(-1 * var(--hand-ml, -22px)));
}
.hand-container .card {
  margin-left: var(--hand-ml, -22px);
  transition:
    transform var(--dur) var(--ease),
    margin-left var(--dur) var(--ease),
    top var(--dur) var(--ease);
}
```

In the `@media (max-width: 560px)` block (~line 921), DELETE these two rules — the measured variable replaces them (the smaller card width is picked up via `offsetWidth`):

```css
  .hand-container {
    padding-left: 16px;
  }
  .hand-container .card {
    margin-left: -16px;
  }
```

In the `@media (prefers-reduced-motion: reduce)` block (~line 845, the one that sets `.btn { transition: none; }`), add:

```css
  .hand-container .card {
    transition: none;
  }
```

- [ ] **Step 6: Run the web test suites**

Run: `pnpm -C web test`
Expected: PASS (unit + component).

- [ ] **Step 7: Commit**

```bash
git add web/src/cards/hand-manager.ts web/src/ui/design.css web/tests/component/hand-manager.spec.ts
git commit -m "feat(web): adaptive hand-fan spacing via measured --hand-ml" -- web/src/cards/hand-manager.ts web/src/ui/design.css web/tests/component/hand-manager.spec.ts
```

---

### Task 3: Visual turn cue (playable-card lift, center text, seat pulse)

**Files:**
- Modify: `web/src/ui/design.css`
- Modify: `web/src/routes/game-view.ts` (centerText, ~lines 94-105)

- [ ] **Step 1: Add the playable-card cue CSS**

In `web/src/ui/design.css`, directly BEFORE the existing `.hand-container .card.cm-clickable:hover` rule, add:

```css
.hand-container .card.cm-clickable {
  /* Turn cue: playable cards rise and carry an accent edge. `top`, not
     `transform` — inline transforms belong to the animation system. */
  top: -6px;
  box-shadow:
    inset 0 2px 0 var(--accent),
    var(--shadow-card);
}
```

(`.card` is already `position: relative`, so `top` applies. The orchestrator's `detachInteraction` clears inline `top` but this is a class rule, unaffected.)

- [ ] **Step 2: Add the seat-chip pulse**

In `web/src/ui/design.css`, directly after the `.spades-seat.active { outline: … }` rule (~line 1033), add:

```css
@media (prefers-reduced-motion: no-preference) {
  /* Pulse the player's own chip when the turn lands; settles to the solid outline. */
  .seat-south.active {
    animation: seat-pulse 700ms var(--ease) 3;
  }
  @keyframes seat-pulse {
    50% {
      outline-offset: 5px;
      outline-color: color-mix(in oklab, var(--accent) 55%, transparent);
    }
  }
}
```

- [ ] **Step 3: Add the "Your turn" center text**

In `web/src/routes/game-view.ts`, replace the `centerText` declaration:

```ts
    const centerText =
      store.phase.value === 'GAME_OVER'
        ? store.teamAScore.value === store.teamBScore.value
          ? "It's a tie!"
          : store.teamAScore.value > store.teamBScore.value
            ? 'Team A wins!'
            : 'Team B wins!'
        : store.phase.value === 'BETTING'
          ? isMyTurn
            ? 'Place your bet!'
            : `Waiting for ${seatName(store.playerIds.value.indexOf(store.currentPlayerId.value ?? ''))}…`
          : '';
```

with:

```ts
    const centerText =
      store.phase.value === 'GAME_OVER'
        ? store.teamAScore.value === store.teamBScore.value
          ? "It's a tie!"
          : store.teamAScore.value > store.teamBScore.value
            ? 'Team A wins!'
            : 'Team B wins!'
        : store.phase.value === 'BETTING'
          ? isMyTurn
            ? 'Place your bet!'
            : `Waiting for ${seatName(store.playerIds.value.indexOf(store.currentPlayerId.value ?? ''))}…`
          : store.phase.value === 'PLAYING' && isMyTurn
            ? 'Your turn'
            : '';
```

(The center-text element is already `aria-live="polite"`, so screen readers announce the turn.)

- [ ] **Step 4: Run web tests + lint**

Run: `pnpm -C web test && pnpm -C web lint`
Expected: PASS / no errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/design.css web/src/routes/game-view.ts
git commit -m "feat(web): layered visual turn cue (card lift, center text, seat pulse)" -- web/src/ui/design.css web/src/routes/game-view.ts
```

---

### Task 4: Sound preference in storage

**Files:**
- Modify: `web/src/lib/storage.ts`
- Test: `web/tests/unit/storage.spec.ts`

- [ ] **Step 1: Write the failing test**

In `web/tests/unit/storage.spec.ts`, extend the second import line to include the new functions:

```ts
import { getThemePref, setThemePref, clearThemePref } from '../../src/lib/storage';
import { getSoundPref, setSoundPref } from '../../src/lib/storage';
```

Append a new describe block at the end of the file (same localStorage stub pattern as the existing blocks):

```ts
describe('sound preference storage', () => {
  beforeEach(() => {
    const store: Record<string, string> = {};
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => (k in store ? store[k]! : null),
      setItem: (k: string, v: string) => {
        store[k] = v;
      },
      removeItem: (k: string) => {
        delete store[k];
      },
    });
  });

  it('defaults to on', () => {
    expect(getSoundPref()).toBe(true);
  });

  it('round-trips off and on', () => {
    setSoundPref(false);
    expect(getSoundPref()).toBe(false);
    setSoundPref(true);
    expect(getSoundPref()).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/storage.spec.ts`
Expected: FAIL — `getSoundPref` is not exported.

- [ ] **Step 3: Implement**

Append to `web/src/lib/storage.ts`:

```ts
const SOUND_KEY = 'spades_sound';

/** Turn-chime preference; default on. */
export function getSoundPref(): boolean {
  try {
    return localStorage.getItem(SOUND_KEY) !== 'off';
  } catch {
    return true;
  }
}

export function setSoundPref(on: boolean): void {
  try {
    localStorage.setItem(SOUND_KEY, on ? 'on' : 'off');
  } catch {
    // ignore (private mode)
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/storage.spec.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/storage.ts web/tests/unit/storage.spec.ts
git commit -m "feat(web): persisted turn-sound preference (default on)" -- web/src/lib/storage.ts web/tests/unit/storage.spec.ts
```

---

### Task 5: Turn-edge helper

**Files:**
- Modify: `web/src/state/helpers.ts`
- Test: `web/tests/unit/turn-chime.spec.ts`

- [ ] **Step 1: Write the failing test**

Create `web/tests/unit/turn-chime.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { becameMyTurn } from '../../src/state/helpers';

describe('becameMyTurn', () => {
  it('fires on the edge where the turn becomes mine', () => {
    expect(becameMyTurn('p2', 'p1', 'p1', 'PLAYING')).toBe(true);
    expect(becameMyTurn(null, 'p1', 'p1', 'BETTING')).toBe(true);
  });

  it('does not fire while the turn stays mine', () => {
    expect(becameMyTurn('p1', 'p1', 'p1', 'PLAYING')).toBe(false);
  });

  it('does not fire for someone else or outside turn phases', () => {
    expect(becameMyTurn('p1', 'p2', 'p1', 'PLAYING')).toBe(false);
    expect(becameMyTurn('p2', 'p1', 'p1', 'GAME_OVER')).toBe(false);
    expect(becameMyTurn('p2', 'p1', 'p1', 'LOBBY')).toBe(false);
  });

  it('never fires with an empty player id', () => {
    expect(becameMyTurn(null, '', '', 'PLAYING')).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/turn-chime.spec.ts`
Expected: FAIL — `becameMyTurn` is not exported.

- [ ] **Step 3: Implement**

Append to `web/src/state/helpers.ts` (the `Phase` type is already exported at the top of this file):

```ts
/**
 * True when the turn just passed to `myId` — the rising edge that triggers
 * the turn chime. Phases without a turn never chime.
 */
export function becameMyTurn(
  prev: string | null,
  current: string | null,
  myId: string,
  phase: Phase,
): boolean {
  if (!myId) return false;
  if (phase !== 'BETTING' && phase !== 'PLAYING') return false;
  return current === myId && prev !== current;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C web exec vitest run --project=unit tests/unit/turn-chime.spec.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add web/src/state/helpers.ts web/tests/unit/turn-chime.spec.ts
git commit -m "feat(web): pure became-my-turn edge detection" -- web/src/state/helpers.ts web/tests/unit/turn-chime.spec.ts
```

---

### Task 6: Chime module + game-view wiring

**Files:**
- Create: `web/src/lib/sound.ts`
- Modify: `web/src/routes/game-view.ts`

No unit test: the module is a thin WebAudio shim (no AudioContext in node/happy-dom); the pure pieces it depends on (`getSoundPref`, `becameMyTurn`) are already tested.

- [ ] **Step 1: Create the sound module**

Create `web/src/lib/sound.ts`:

```ts
import { getSoundPref } from './storage';

let ctx: AudioContext | null = null;

function getCtx(): AudioContext | null {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === 'suspended') void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

/**
 * Soft two-note turn chime (E5 -> A5). Best-effort: any failure — including
 * autoplay policy before the first user gesture — is silently ignored.
 */
export function chime(): void {
  if (!getSoundPref()) return;
  const ac = getCtx();
  if (!ac) return;
  try {
    const t0 = ac.currentTime;
    for (const [freq, at] of [
      [659.25, 0],
      [880, 0.12],
    ] as const) {
      const osc = ac.createOscillator();
      const gain = ac.createGain();
      osc.type = 'sine';
      osc.frequency.value = freq;
      gain.gain.setValueAtTime(0, t0 + at);
      gain.gain.linearRampToValueAtTime(0.08, t0 + at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, t0 + at + 0.3);
      osc.connect(gain).connect(ac.destination);
      osc.start(t0 + at);
      osc.stop(t0 + at + 0.32);
    }
  } catch {
    // audio is an enhancement, never an error
  }
}
```

- [ ] **Step 2: Wire the edge-detect effect into game-view**

In `web/src/routes/game-view.ts`:

Add `becameMyTurn` to the existing `../state/helpers` import list, and add:

```ts
import { chime } from '../lib/sound';
```

After the `disposeClock` effect (before the `startClockTicker();` line), add:

```ts
  // Turn chime: fire once on the edge where the turn becomes ours.
  let prevTurnPlayer: string | null = null;
  const disposeChime = effect(() => {
    const current = store.currentPlayerId.value;
    if (becameMyTurn(prevTurnPlayer, current, store.playerId.value, store.phase.value)) {
      chime();
    }
    prevTurnPlayer = current;
  });
```

And register the cleanup alongside its siblings (after `args.resources.cleanups.push(disposeClock);`):

```ts
  args.resources.cleanups.push(disposeChime);
```

- [ ] **Step 3: Type-check, lint, test**

Run: `pnpm -C web lint && pnpm -C web test`
Expected: PASS / no errors. (`pnpm -C web lint` includes `tsc --noEmit` if configured; if not, `pnpm -C web build` type-checks.)

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/sound.ts web/src/routes/game-view.ts
git commit -m "feat(web): turn chime on became-my-turn edge" -- web/src/lib/sound.ts web/src/routes/game-view.ts
```

---

### Task 7: Settings toggle

**Files:**
- Modify: `web/src/routes/settings.ts`
- Modify: `web/src/ui/design.css`
- Test: `web/tests/component/settings.spec.ts`

- [ ] **Step 1: Write the failing test**

Append to the `describe('settings route', …)` block in `web/tests/component/settings.spec.ts`:

```ts
  it('renders the turn-sound toggle, default on, and persists changes', () => {
    localStorage.removeItem('spades_sound');
    const cleanup = settings.render({}, { path: '/me', search: new URLSearchParams() });
    const box = document.querySelector<HTMLInputElement>('#turn_sound')!;
    expect(box).not.toBeNull();
    expect(box.checked).toBe(true);
    box.checked = false;
    box.dispatchEvent(new Event('change'));
    expect(localStorage.getItem('spades_sound')).toBe('off');
    cleanup();
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/settings.spec.ts`
Expected: FAIL — `#turn_sound` is null.

- [ ] **Step 3: Implement the toggle**

In `web/src/routes/settings.ts`:

Add the import:

```ts
import { getSoundPref, setSoundPref } from '../lib/storage';
```

Add a signal next to the others (after `const saved = signal(false);`):

```ts
    const soundOn = signal(getSoundPref());
```

In the template, after the `new_password` `formField(...)` call and before `<div class="form-actions">`, add:

```ts
          <label class="field-checkbox">
            <input
              id="turn_sound"
              type="checkbox"
              .checked=${soundOn.value}
              @change=${(e: Event) => {
                const on = (e.target as HTMLInputElement).checked;
                soundOn.value = on;
                setSoundPref(on);
              }}
            />
            Turn sound
          </label>
```

- [ ] **Step 4: Add the checkbox styling**

In `web/src/ui/design.css`, after the existing `.field-error` / `.field-success` rules (search for `.field-success`), add:

```css
.field-checkbox {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  cursor: pointer;
}
.field-checkbox input {
  accent-color: var(--accent);
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/settings.spec.ts`
Expected: PASS (all existing + 1 new).

- [ ] **Step 6: Commit**

```bash
git add web/src/routes/settings.ts web/src/ui/design.css web/tests/component/settings.spec.ts
git commit -m "feat(web): turn-sound toggle in settings" -- web/src/routes/settings.ts web/src/ui/design.css web/tests/component/settings.spec.ts
```

---

### Task 8: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Format + lint**

Run: `pnpm -C web format && pnpm -C web lint`
Expected: no diffs beyond whitespace, no lint errors. If `format` changed files, amend them into the relevant commits or commit as `style(web): format`.

- [ ] **Step 2: Full test suite**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && make test`
Expected: cargo workspace tests + web unit/component all PASS (no server code changed; this is a regression check).

- [ ] **Step 3: E2E**

Run: `make e2e`
Expected: PASS. The e2e page objects use `.hand-container .card` and `.cm-clickable` — both selectors are preserved by this change. If a stability check fails on the hand, the likely culprit is the new `margin-left` transition; fix by checking the failure screenshot before touching code.

- [ ] **Step 4: Pre-push gate**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && make check`
Expected: fmt-check, clippy, and all tests PASS. (No OpenAPI changes — no server endpoints/DTOs touched, so no codegen step needed.)

---

## Manual smoke test (after all tasks)

1. `make dev`, open `http://localhost:5173`, start an AI game.
2. Hand: 13 cards spread across the south row; play a few — the fan re-spaces smoothly; shrink the window — overlap tightens, never below a 24px strip.
3. Turn cue: on your turn the playable cards rise with an accent top edge, the felt reads "Your turn", your chip pulses ~3 beats, and a soft two-note chime plays.
4. Settings (`/me`, signed in): toggle "Turn sound" off → no chime next turn.
