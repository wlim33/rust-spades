# Landing-page Leaderboard Preview — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a compact "Top players" leaderboard section below the landing-page menu — top 5, with all-time/this-month tabs and a link to the full board — without displacing the gameplay join buttons.

**Architecture:** A new self-contained component (`ui/components/leaderboard-preview.ts`) owns module-level signals + an epoch-guarded fetch (reusing `request` and the `Leaderboard` types), exposing `leaderboardPreview()` (lit-html fragment), `startLeaderboardPreview()`, and `stopLeaderboardPreview()`. `home.ts` embeds the fragment as a sibling section *after* `.menu` and drives its lifecycle. The full `/leaderboard` route is untouched; the server caps responses at 10 rows, so the widget slices to 5 client-side. No backend changes.

**Tech Stack:** TypeScript, lit-html, `@preact/signals-core`, Vite, Vitest + happy-dom (component tests), pnpm.

**Spec:** `docs/superpowers/specs/2026-06-02-landing-leaderboard-design.md`

---

## File Structure

- **Create** `web/src/ui/components/leaderboard-preview.ts` — the widget: signals, `load`, `leaderboardPreview()`, `start`/`stop`. One responsibility: the landing preview's state + view.
- **Modify** `web/src/routes/home.ts` — import the widget; embed `${leaderboardPreview()}` after `.menu` in the non-searching branch; `startLeaderboardPreview()` before the render effect; `stopLeaderboardPreview()` in dispose.
- **Modify** `web/src/ui/design.css` — append a `.home-leaderboard*` block (reuses existing `.leaderboard__*` rules).
- **Create** `web/tests/component/leaderboard-preview.spec.ts` — drives behavior through `home.render`, mirroring `tests/component/leaderboard.spec.ts`.

> **Refinement vs. spec:** `stopLeaderboardPreview()` resets state to initial (not just invalidating the epoch). This guarantees clean test isolation and predictable fresh state on home re-entry, at the cost of the spec's "cached top-5 on re-entry" micro-optimization. The "no flicker on period switch" behavior is preserved (the cached list stays visible while a new period loads). This is the only deviation from the approved spec.

---

## Pre-flight

- [ ] **Confirm a green baseline**

Run: `pnpm -C web test:component`
Expected: PASS (all existing component specs green, including `home.spec.ts` and `leaderboard.spec.ts`). If anything fails here, stop and investigate before starting.

---

## Task 1: Component (all-time top-5 + links) wired into home

**Files:**
- Create: `web/src/ui/components/leaderboard-preview.ts`
- Modify: `web/src/routes/home.ts` (imports; `template()`; `render()`; dispose)
- Test: `web/tests/component/leaderboard-preview.spec.ts`

- [ ] **Step 1: Write the failing test**

Create `web/tests/component/leaderboard-preview.spec.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { home, quickplay } from '../../src/routes/home';

type Entry = {
  rank: number;
  username: string;
  rating: number;
  rd: number;
  games_played: number;
  score: number;
};

function entry(rank: number, username: string, rating: number): Entry {
  return { rank, username, rating, rd: 50, games_played: 10, score: rating - 100 };
}

// Leaderboard JSON for /leaderboard; empty array for the queue poll
// (refreshQueueSizes ignores non-arrays, but [] keeps the stub explicit).
function stubLeaderboardFetch(entries: Entry[], period = 'all-time'): ReturnType<typeof vi.fn> {
  return vi.fn(async (url: string) => {
    if (typeof url === 'string' && url.includes('/leaderboard')) {
      return new Response(JSON.stringify({ period, entries }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }
    return new Response(JSON.stringify([]), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
}

async function flush(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

function renderHome(): () => void {
  return home.render({}, { path: '/', search: new URLSearchParams() });
}

describe('home leaderboard preview', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    quickplay.value = null;
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    quickplay.value = null;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('shows at most five rows even when the API returns ten', async () => {
    const tens = Array.from({ length: 10 }, (_, i) => entry(i + 1, `player${i + 1}`, 1900 - i * 10));
    vi.stubGlobal('fetch', stubLeaderboardFetch(tens));
    const cleanup = renderHome();
    await flush();
    expect(document.querySelector('[data-testid="home-leaderboard"]')).not.toBeNull();
    expect(document.querySelectorAll('.home-leaderboard .leaderboard__row').length).toBe(5);
    expect(document.body.textContent).toContain('player1');
    expect(document.body.textContent).not.toContain('player6');
    cleanup();
  });

  it('links rows to profiles and the header to the full board', async () => {
    vi.stubGlobal('fetch', stubLeaderboardFetch([entry(1, 'alice', 1800)]));
    const cleanup = renderHome();
    await flush();
    const nameLink = document.querySelector(
      '.home-leaderboard .leaderboard__name',
    ) as HTMLAnchorElement;
    expect(nameLink.getAttribute('href')).toBe('/u/alice');
    const moreLink = document.querySelector('.home-leaderboard__more') as HTMLAnchorElement;
    expect(moreLink.getAttribute('href')).toBe('/leaderboard');
    cleanup();
  });

  it('renders below the menu without removing the join buttons', async () => {
    vi.stubGlobal('fetch', stubLeaderboardFetch([entry(1, 'alice', 1800)]));
    const cleanup = renderHome();
    await flush();
    expect(document.querySelector('[data-testid="home-menu"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="play-friends"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="home-leaderboard"]')).not.toBeNull();
    cleanup();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts`
Expected: FAIL — `[data-testid="home-leaderboard"]` is null and `.home-leaderboard .leaderboard__row` count is 0 (the widget isn't created or wired yet).

- [ ] **Step 3: Create the component**

Create `web/src/ui/components/leaderboard-preview.ts`:

```ts
import { html, type TemplateResult } from 'lit-html';
import { signal } from '@preact/signals-core';
import { request } from '../../api/client';
import { icon } from '../icon';
import type { Leaderboard, LeaderboardPeriod } from '../../state/user-types';

// The landing preview shows the top few; the API already caps at 10.
const PREVIEW_SIZE = 5;

// Module-level signals (mirrors home.ts's `quickplay` pattern).
const period = signal<LeaderboardPeriod>('all-time');
const board = signal<Leaderboard | null>(null);
const loading = signal(true);
const error = signal<string | null>(null);

// Epoch guard: a slow response must not overwrite a newer one
// (same technique as routes/leaderboard.ts).
let loadEpoch = 0;

async function load(p: LeaderboardPeriod): Promise<void> {
  const epoch = ++loadEpoch;
  loading.value = true;
  error.value = null;
  try {
    const data = await request<Leaderboard>(`/leaderboard?period=${p}`, { method: 'GET' });
    if (epoch !== loadEpoch) return; // a newer load superseded this one
    board.value = data;
  } catch (e) {
    if (epoch !== loadEpoch) return;
    error.value = e instanceof Error ? e.message : 'Failed to load leaderboard.';
  } finally {
    if (epoch === loadEpoch) loading.value = false;
  }
}

/** Begin loading. Call BEFORE the host's render effect runs so the first paint
 *  is in a loading posture (no empty-state flash). */
export function startLeaderboardPreview(): void {
  void load(period.value);
}

/** Tear down: invalidate any in-flight load and reset to initial state, so a
 *  late response can't write into a torn-down root and the next mount is clean. */
export function stopLeaderboardPreview(): void {
  loadEpoch++;
  period.value = 'all-time';
  board.value = null;
  loading.value = true;
  error.value = null;
}

export function leaderboardPreview(): TemplateResult {
  const entries = (board.value?.entries ?? []).slice(0, PREVIEW_SIZE);
  return html`
    <section
      class="home-leaderboard panel"
      aria-labelledby="home-lb-title"
      data-testid="home-leaderboard"
    >
      <div class="home-leaderboard__head">
        <h2 id="home-lb-title" class="home-leaderboard__title">Top players</h2>
        <a class="home-leaderboard__more" href="/leaderboard" data-link
          >View full leaderboard ${icon('arrow-right-s-line')}</a
        >
      </div>
      <ol class="leaderboard__list">
        ${entries.map(
          (e) =>
            html`<li class="leaderboard__row">
              <span class="leaderboard__rank">${e.rank}</span>
              <a class="leaderboard__name" href="/u/${encodeURIComponent(e.username)}" data-link
                >${e.username}</a
              >
              <span class="leaderboard__rating">${e.rating}</span>
            </li>`,
        )}
      </ol>
    </section>
  `;
}
```

- [ ] **Step 4: Wire the component into `home.ts` — add the import**

In `web/src/routes/home.ts`, after the existing `import { icon } from '../ui/icon';` line, add:

```ts
import {
  leaderboardPreview,
  startLeaderboardPreview,
  stopLeaderboardPreview,
} from '../ui/components/leaderboard-preview';
```

- [ ] **Step 5: Embed the fragment after `.menu`**

In `home.ts`, in the non-searching `return appShell(html\`…\`)` branch, find the end of the menu block (the computers button followed by the menu's closing `</div>`) and insert `${leaderboardPreview()}` after it:

Find:
```ts
        <span class="menu__row-go">${icon('arrow-right-s-line')}</span>
      </button>
    </div>
  `);
}
```
Replace with:
```ts
        <span class="menu__row-go">${icon('arrow-right-s-line')}</span>
      </button>
    </div>
    ${leaderboardPreview()}
  `);
}
```

- [ ] **Step 6: Start the preview before the render effect**

In `home.ts`'s `render`, find:
```ts
    startQueuePoll();
    const dispose = effect(() => {
```
Replace with:
```ts
    startQueuePoll();
    startLeaderboardPreview();
    const dispose = effect(() => {
```

- [ ] **Step 7: Stop the preview in dispose**

In `home.ts`'s returned cleanup, find:
```ts
      dispose();
      stopQueuePoll();
      render(nothing, root);
```
Replace with:
```ts
      dispose();
      stopQueuePoll();
      stopLeaderboardPreview();
      render(nothing, root);
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts`
Expected: PASS (3 tests).

- [ ] **Step 9: Commit**

```bash
git add web/src/ui/components/leaderboard-preview.ts web/src/routes/home.ts web/tests/component/leaderboard-preview.spec.ts
git commit -m "feat(home): landing-page leaderboard preview (top 5, all-time)"
```

---

## Task 2: Period tabs (all-time / this-month)

**Files:**
- Modify: `web/src/ui/components/leaderboard-preview.ts`
- Test: `web/tests/component/leaderboard-preview.spec.ts`

- [ ] **Step 1: Write the failing test**

Add this `it` block inside the `describe('home leaderboard preview', …)` in `web/tests/component/leaderboard-preview.spec.ts`:

```ts
  it('switches to this-month and refetches with that period', async () => {
    const periods: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (typeof url === 'string' && url.includes('/leaderboard')) {
          periods.push(url.includes('this-month') ? 'this-month' : 'all-time');
          return new Response(JSON.stringify({ period: 'x', entries: [] }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          });
        }
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    const cleanup = renderHome();
    await flush();
    const tab = document.querySelector('[data-testid="home-tab-this-month"]') as HTMLButtonElement;
    expect(tab).not.toBeNull();
    tab.click();
    await flush();
    expect(periods).toContain('this-month');
    expect(tab.getAttribute('aria-pressed')).toBe('true');
    cleanup();
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts -t "switches to this-month"`
Expected: FAIL — `[data-testid="home-tab-this-month"]` is null, so `tab.click()` throws a TypeError.

- [ ] **Step 3: Add the tab handler and tabs markup**

In `web/src/ui/components/leaderboard-preview.ts`, add the handler just above `startLeaderboardPreview`:

```ts
function selectPreviewPeriod(p: LeaderboardPeriod): void {
  if (period.value === p) return;
  period.value = p;
  void load(p);
}
```

Then replace the whole `leaderboardPreview()` function with this version (adds the tabs row; reads `period`):

```ts
export function leaderboardPreview(): TemplateResult {
  const cur = period.value;
  const entries = (board.value?.entries ?? []).slice(0, PREVIEW_SIZE);
  return html`
    <section
      class="home-leaderboard panel"
      aria-labelledby="home-lb-title"
      data-testid="home-leaderboard"
    >
      <div class="home-leaderboard__head">
        <h2 id="home-lb-title" class="home-leaderboard__title">Top players</h2>
        <a class="home-leaderboard__more" href="/leaderboard" data-link
          >View full leaderboard ${icon('arrow-right-s-line')}</a
        >
      </div>
      <div class="leaderboard__tabs" role="group" aria-label="Leaderboard period">
        <button
          class="leaderboard__tab ${cur === 'all-time' ? 'is-active' : ''}"
          data-testid="home-tab-all-time"
          aria-pressed=${cur === 'all-time'}
          @click=${() => selectPreviewPeriod('all-time')}
        >
          All-time
        </button>
        <button
          class="leaderboard__tab ${cur === 'this-month' ? 'is-active' : ''}"
          data-testid="home-tab-this-month"
          aria-pressed=${cur === 'this-month'}
          @click=${() => selectPreviewPeriod('this-month')}
        >
          This month
        </button>
      </div>
      <ol class="leaderboard__list">
        ${entries.map(
          (e) =>
            html`<li class="leaderboard__row">
              <span class="leaderboard__rank">${e.rank}</span>
              <a class="leaderboard__name" href="/u/${encodeURIComponent(e.username)}" data-link
                >${e.username}</a
              >
              <span class="leaderboard__rating">${e.rating}</span>
            </li>`,
        )}
      </ol>
    </section>
  `;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/components/leaderboard-preview.ts web/tests/component/leaderboard-preview.spec.ts
git commit -m "feat(home): period tabs on the landing leaderboard preview"
```

---

## Task 3: Graceful states (loading-first, muted error, empty)

**Files:**
- Modify: `web/src/ui/components/leaderboard-preview.ts` (import `nothing`; state predicates + status lines)
- Test: `web/tests/component/leaderboard-preview.spec.ts`

- [ ] **Step 1: Write the failing tests**

Add these two `it` blocks inside the `describe` in `web/tests/component/leaderboard-preview.spec.ts`:

```ts
  it('shows a quiet unavailable message on failure and keeps the join menu', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new Error('network down');
      }),
    );
    const cleanup = renderHome();
    await flush();
    expect(document.body.textContent).toContain('Leaderboard unavailable.');
    // Not the loud red field-error treatment the full page uses.
    expect(document.querySelector('.home-leaderboard .field-error')).toBeNull();
    // The core guarantee: a leaderboard failure never removes the join buttons.
    expect(document.querySelector('[data-testid="home-menu"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="play-friends"]')).not.toBeNull();
    cleanup();
  });

  it('shows an empty state when no players are ranked', async () => {
    vi.stubGlobal('fetch', stubLeaderboardFetch([]));
    const cleanup = renderHome();
    await flush();
    expect(document.body.textContent?.toLowerCase()).toContain('no ranked players');
    cleanup();
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts -t "unavailable"`
Then: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts -t "empty state"`
Expected: BOTH FAIL — the component renders neither "Leaderboard unavailable." nor "No ranked players yet." yet (it always renders a bare `<ol>`).

- [ ] **Step 3: Import `nothing`**

In `web/src/ui/components/leaderboard-preview.ts`, change the first import line:

```ts
import { html, type TemplateResult } from 'lit-html';
```
to:
```ts
import { html, nothing, type TemplateResult } from 'lit-html';
```

- [ ] **Step 4: Add state predicates and conditional rendering**

Replace the whole `leaderboardPreview()` function with this version:

```ts
export function leaderboardPreview(): TemplateResult {
  // Read all signals eagerly before building the template (see
  // routes/leaderboard.ts: happy-dom/lit-html nested-conditional quirk).
  const l = loading.value;
  const err = error.value;
  const b = board.value;
  const cur = period.value;
  const entries = (b?.entries ?? []).slice(0, PREVIEW_SIZE);
  const showLoading = l && !b; // first load only; keep cached list during refetch
  const showEmpty = !l && !err && entries.length === 0;
  const showList = !err && entries.length > 0;

  return html`
    <section
      class="home-leaderboard panel"
      aria-labelledby="home-lb-title"
      data-testid="home-leaderboard"
    >
      <div class="home-leaderboard__head">
        <h2 id="home-lb-title" class="home-leaderboard__title">Top players</h2>
        <a class="home-leaderboard__more" href="/leaderboard" data-link
          >View full leaderboard ${icon('arrow-right-s-line')}</a
        >
      </div>
      <div class="leaderboard__tabs" role="group" aria-label="Leaderboard period">
        <button
          class="leaderboard__tab ${cur === 'all-time' ? 'is-active' : ''}"
          data-testid="home-tab-all-time"
          aria-pressed=${cur === 'all-time'}
          @click=${() => selectPreviewPeriod('all-time')}
        >
          All-time
        </button>
        <button
          class="leaderboard__tab ${cur === 'this-month' ? 'is-active' : ''}"
          data-testid="home-tab-this-month"
          aria-pressed=${cur === 'this-month'}
          @click=${() => selectPreviewPeriod('this-month')}
        >
          This month
        </button>
      </div>
      ${showLoading ? html`<p class="home-leaderboard__status">Loading…</p>` : nothing}
      ${err ? html`<p class="home-leaderboard__status">Leaderboard unavailable.</p>` : nothing}
      ${showEmpty ? html`<p class="home-leaderboard__status">No ranked players yet.</p>` : nothing}
      ${showList
        ? html`<ol class="leaderboard__list">
            ${entries.map(
              (e) =>
                html`<li class="leaderboard__row">
                  <span class="leaderboard__rank">${e.rank}</span>
                  <a
                    class="leaderboard__name"
                    href="/u/${encodeURIComponent(e.username)}"
                    data-link
                    >${e.username}</a
                  >
                  <span class="leaderboard__rating">${e.rating}</span>
                </li>`,
            )}
          </ol>`
        : nothing}
    </section>
  `;
}
```

- [ ] **Step 5: Run the full spec to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/leaderboard-preview.spec.ts`
Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git add web/src/ui/components/leaderboard-preview.ts web/tests/component/leaderboard-preview.spec.ts
git commit -m "feat(home): graceful loading/error/empty states for leaderboard preview"
```

---

## Task 4: Styling

**Files:**
- Modify: `web/src/ui/design.css`

No unit test (happy-dom doesn't compute layout); verified by build + a manual visual check.

- [ ] **Step 1: Append the preview styles**

At the **end of** `web/src/ui/design.css`, append:

```css

/* Landing-page leaderboard preview */
.home-leaderboard {
  width: 100%;
  max-width: 360px;
  margin-top: var(--space-4);
}
.home-leaderboard__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}
.home-leaderboard__title {
  font-size: var(--text-lg);
  margin: 0;
}
.home-leaderboard__more {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: var(--text-sm);
  color: var(--accent);
  white-space: nowrap;
}
.home-leaderboard__status {
  margin: var(--space-2) 0 0;
  color: var(--fg-muted);
  font-size: var(--text-sm);
}
```

- [ ] **Step 2: Verify the build and formatting are clean**

Run: `pnpm -C web build && pnpm -C web format:check`
Expected: PASS (no TS/Vite errors; prettier reports the CSS as formatted).

- [ ] **Step 3: Visual check (manual)**

Run: `pnpm -C web dev`, open the home page. Confirm:
- The "Top players" panel sits centered directly **below** the menu, same ~360px width, with the join tiles/rows clearly primary above it.
- Light **and** dark themes look right (tokens only — toggle via the header).
- Mobile width: the panel stacks under the menu, no overflow.
- The "View full leaderboard" link navigates to `/leaderboard`; a row navigates to `/u/<name>`.

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/design.css
git commit -m "style(home): leaderboard preview panel styles"
```

---

## Task 5: Full gate + regression

**Files:** none (verification; commit only if a touch-up is needed).

- [ ] **Step 1: Run the full gate**

Run: `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
Expected: ALL PASS. In particular, `tests/component/home.spec.ts` stays green:
- "renders the menu with five action buttons" — the widget's tab buttons live in `.home-leaderboard` (outside `[data-testid="home-menu"]`), so the count is still 5.
- "cleanup empties the root" — `stopLeaderboardPreview()` runs after `dispose()` (effect already torn down), then `render(nothing, root)` empties the root.

- [ ] **Step 2: Contingency — only if `home.spec.ts` errors on the unstubbed fetch**

The existing home tests don't stub `fetch`; `startLeaderboardPreview()` issues a best-effort request that rejects harmlessly (caught by `load`, and the epoch is invalidated on cleanup before it resolves). If the run surfaces an unhandled-rejection error from it, add a benign stub to `tests/component/home.spec.ts`'s `beforeEach`:

```ts
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('[]', { status: 200, headers: { 'content-type': 'application/json' } })),
    );
```
and `vi.unstubAllGlobals()` in `afterEach`. Re-run the gate. (Expected: not needed.)

- [ ] **Step 3: Commit any touch-up (skip if clean)**

```bash
git add -A web/tests/component/home.spec.ts
git commit -m "test(home): stub fetch so leaderboard preview load is inert"
```

---

## Self-Review

**Spec coverage:**
- Placement below the menu → Task 1 Step 5 (sibling after `.menu`) + Task 4 (360px width, centered by `.page`).
- Top 5, slice client-side → `PREVIEW_SIZE = 5` (Task 1); verified by the "at most five rows" test.
- All-time / this-month tabs → Task 2.
- New component reusing data layer (types + `request` + CSS), full route untouched → Task 1 imports; no edit to `routes/leaderboard.ts`.
- Epoch-guarded fetch → `load` (Task 1).
- Links: rows → `/u/:username`, header → `/leaderboard` → Task 1; verified by the links test.
- Graceful states (loading-first, muted non-red error, empty) → Task 3; error test asserts no `.field-error` and that the menu survives.
- Lifecycle wired into home; in-flight load invalidated on teardown → Task 1 Steps 6–7 + `stopLeaderboardPreview`.
- Accessibility (`role="group"`, `aria-pressed`, `aria-labelledby`, tabular-nums via reused classes) → Tasks 2–3 + reused CSS.
- Testing mirrors `leaderboard.spec.ts`, driven through `home.render`; existing home spec stays green → Tasks 1–3 + Task 5.
- No backend changes, no polling, no extra columns → nothing in the plan adds them.

**Placeholder scan:** none — every code step contains complete content.

**Type consistency:** `leaderboardPreview()` / `startLeaderboardPreview()` / `stopLeaderboardPreview()` / `selectPreviewPeriod()` and the signals `period` / `board` / `loading` / `error` / `loadEpoch` are named identically across Tasks 1–3. `LeaderboardPeriod` and `Leaderboard` come from `state/user-types.ts`. `request<Leaderboard>(path, { method: 'GET' })` matches `api/client.ts`. `icon('arrow-right-s-line')` matches `ui/icon.ts`. The `home.ts` edits match the file's current text.

One deviation from the spec is documented above (File Structure note): `stopLeaderboardPreview()` resets state rather than only invalidating the epoch.
