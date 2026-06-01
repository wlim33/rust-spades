# Phase 3b — Setup & Account Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the four remaining un-redesigned `web/` routes — create, lobby, profile, settings — onto the Phase-0 design system (tokens, the card surface, a new segmented control), with zero behavior change.

**Architecture:** Add two shared CSS primitives (`.seg` segmented control, `.panel` card surface) and one token (`--accent-fg`), then consume them route-by-route. Every change is CSS + markup only; all SSE calls, signals, route signatures, and `data-testid`/class selectors that tests pin are preserved.

**Tech Stack:** TypeScript, lit-html, @preact/signals-core, Vite; vitest (component project = happy-dom); Playwright e2e. Always run pnpm via `pnpm -C web …`.

**Spec:** `docs/superpowers/specs/2026-06-01-setup-account-design.md`

---

## Critical constraints (read before starting)

These selectors are pinned by existing tests — **do not change them**:

- **create** (e2e `create-page.ts`): the submit button's accessible name stays exactly `Create`.
- **lobby** (e2e `lobby-page.ts`): the joinable open seat stays `<button class="seat-open …">`; the join modal stays `.join-modal` with an `input`; the join button's name stays exactly `Join`.
- **settings** (`settings.spec.ts`): inputs keep ids `#email`, `#current_password`, `#new_password`; the save button keeps `data-testid="save"`.
- **profile** (`profile.spec.ts`): the username text and the first 8 chars of the game id (`g1abcdef`) must remain in the rendered text.

Neither create nor lobby uses a `<form>` element, so the happy-dom "binding dropped after `</form>`" gotcha does **not** apply here.

**Verification commands:**

- Single component spec: `pnpm -C web exec vitest run --project=component tests/component/<file>.spec.ts`
- Full gate (run before every commit): `pnpm -C web format && pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
  - `format` first normalizes lit-html/TS formatting so the final `format:check` passes.

**Visual checks (no backend):** `main.ts` aborts boot without a backend, so to eyeball a route create a throwaway `web/_check.html` that imports `./src/ui/design.css` + `./src/ui/tokens.css` and calls the route's `render()` (or `renderLobby(...)`) directly, serve with `pnpm -C web dev` (port 5173), view in a real browser, then **delete the throwaway file** before committing.

---

## File Structure

- `web/src/ui/tokens.css` — add `--accent-fg` (both themes).
- `web/src/ui/design.css` — add `.seg`, `.panel`/`.auth-card` shared surface, `.field-success`; refactor `.btn--primary`; modify seat-grid + `.join-modal` + `.profile-games`.
- `web/src/routes/create.ts` — three fieldsets become `.seg` controls.
- `web/src/routes/lobby.ts` — `data-team` on seats, copy feedback signal.
- `web/src/routes/profile.ts` — `.panel` class + games row markup.
- `web/src/routes/settings.ts` — `.panel` class + token-ized "Saved." line.
- `web/tests/component/create.spec.ts` — **new**.
- `web/tests/component/lobby.spec.ts` — **new**.
- `web/tests/component/profile.spec.ts` — extend.
- `web/tests/component/settings.spec.ts` — extend.

---

## Task 1: Design-system primitives (token + button refactor + `.panel` surface)

CSS/tokens only. There is no CSS unit-test harness in this stack, so the gate is: existing tests stay green + build/lint/format pass + a visual check that primary buttons and the login card are unchanged.

**Files:**
- Modify: `web/src/ui/tokens.css` (light block ~line 14–21, dark block ~line 97–103)
- Modify: `web/src/ui/design.css` (`.btn--primary` ~line 95–106; `.auth-card` ~line 1058–1069)

- [ ] **Step 1: Add the `--accent-fg` token to both themes**

In `web/src/ui/tokens.css`, in the light `:root` block, add `--accent-fg` right after the `--accent-hover` line:

```css
  --accent: #1f8f80;
  --accent-hover: #1a796c;
  --accent-fg: #fff;
```

In the `[data-theme='dark']` block, add it after that block's `--accent-hover`:

```css
  --accent: #3cc3b0;
  --accent-hover: #54d0be;
  --accent-fg: #06231f;
```

- [ ] **Step 2: Refactor `.btn--primary` onto the token**

In `web/src/ui/design.css`, replace this block:

```css
.btn--primary {
  background: var(--accent);
  color: #fff;
  box-shadow: var(--shadow-1);
}
.btn--primary:hover {
  background: var(--accent-hover);
  box-shadow: var(--shadow-2);
}
[data-theme='dark'] .btn--primary {
  color: #06231f;
}
```

with this (note the `[data-theme='dark'] .btn--primary` rule is **deleted** — the token now carries the per-theme value):

```css
.btn--primary {
  background: var(--accent);
  color: var(--accent-fg);
  box-shadow: var(--shadow-1);
}
.btn--primary:hover {
  background: var(--accent-hover);
  box-shadow: var(--shadow-2);
}
```

- [ ] **Step 3: Factor the card surface into a shared `.panel`/`.auth-card` rule**

In `web/src/ui/design.css`, replace the `.auth-card` block:

```css
.auth-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  max-width: 24rem;
  padding: var(--space-6);
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-2);
}
```

with a shared surface rule plus the auth-card's own layout (surface values are identical, so the two can never drift):

```css
.panel,
.auth-card {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-2);
}
.panel {
  padding: var(--space-6);
}
.auth-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  max-width: 24rem;
  padding: var(--space-6);
}
```

- [ ] **Step 4: Run the full gate**

Run: `pnpm -C web format && pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
Expected: all green. The `.btn--primary` refactor resolves to the same colors, so no test changes.

- [ ] **Step 5: Visual check**

Build a throwaway `web/_check.html` importing the CSS and rendering any `button({label:'X',variant:'primary'})`, plus the login route's `authCard`. Confirm primary buttons look identical in light **and** dark (toggle `<html data-theme="dark">`) and the login card surface is unchanged. Delete `web/_check.html`.

- [ ] **Step 6: Commit**

```bash
git add web/src/ui/tokens.css web/src/ui/design.css
git commit -m "feat(web): add --accent-fg token and shared .panel surface" -- web/src/ui/tokens.css web/src/ui/design.css
```

---

## Task 2: create — segmented controls

Replace the three `<fieldset>` button-groups with `.seg` controls; add the `.seg` CSS. New component spec.

**Files:**
- Create: `web/tests/component/create.spec.ts`
- Modify: `web/src/routes/create.ts:98-133`
- Modify: `web/src/ui/design.css` (add `.seg` rules)

- [ ] **Step 1: Write the failing test**

Create `web/tests/component/create.spec.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { create } from '../../src/routes/create';

describe('create route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders three segmented-control groups', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    expect(document.querySelectorAll('.seg')).toHaveLength(3);
    cleanup();
  });

  it('marks the default points (500) and timer (None) segments pressed', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    const pressed = [...document.querySelectorAll('.seg button[aria-pressed="true"]')].map((b) =>
      b.textContent?.trim(),
    );
    expect(pressed).toContain('500');
    expect(pressed).toContain('None');
    cleanup();
  });

  it('clicking a seat segment moves aria-pressed to it', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    const seatSeg = document.querySelectorAll('.seg')[0]!;
    seatSeg.querySelectorAll('button')[0]!.click(); // 'A'
    expect(seatSeg.querySelector('button[aria-pressed="true"]')?.textContent?.trim()).toBe('A');
    cleanup();
  });

  it('keeps a button named exactly "Create"', () => {
    const cleanup = create.render({}, { path: '/create', search: new URLSearchParams() });
    const createBtn = [...document.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Create',
    );
    expect(createBtn).toBeTruthy();
    cleanup();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/create.spec.ts`
Expected: FAIL — first test finds 0 `.seg` elements (current code uses `<fieldset>` + `button()` groups, not `.seg`).

- [ ] **Step 3: Replace the three fieldsets in `create.ts`**

In `web/src/routes/create.ts`, replace lines 98–133 (the three `<fieldset>` blocks) with:

```ts
          <fieldset>
            <legend>Pick seat</legend>
            <div class="seg" role="group" aria-label="Pick seat">
              ${(['A', 'B', 'C', 'D'] as const).map(
                (s) => html`<button
                  type="button"
                  aria-pressed=${seat.value === s}
                  @click=${() => {
                    seat.value = seat.value === s ? null : s;
                  }}
                >
                  ${s}
                </button>`,
              )}
            </div>
          </fieldset>
          <fieldset>
            <legend>Points</legend>
            <div class="seg" role="group" aria-label="Points">
              ${([200, 300, 500] as const).map(
                (p) => html`<button
                  type="button"
                  aria-pressed=${points.value === p}
                  @click=${() => {
                    points.value = p;
                  }}
                >
                  ${p}
                </button>`,
              )}
            </div>
          </fieldset>
          <fieldset>
            <legend>Timer</legend>
            <div class="seg" role="group" aria-label="Timer">
              ${TIMER_PRESETS.map(
                (t, i) => html`<button
                  type="button"
                  aria-pressed=${timerIdx.value === i}
                  @click=${() => {
                    timerIdx.value = i;
                  }}
                >
                  ${t.label}
                </button>`,
              )}
            </div>
          </fieldset>
```

The `button` import stays (Create/Back still use it). `html` is already imported.

- [ ] **Step 4: Add the `.seg` CSS**

In `web/src/ui/design.css`, add after the `.form-page fieldset { … }` block (around line 473):

```css
.seg {
  display: flex;
  flex-wrap: wrap;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  overflow: hidden;
  background: var(--surface-raised);
}
.seg button {
  flex: 1 1 auto;
  min-width: 3rem;
  padding: var(--space-2) var(--space-3);
  border: none;
  border-left: 1px solid var(--border);
  background: transparent;
  color: var(--fg-muted);
  font: inherit;
  cursor: pointer;
}
.seg button:first-child {
  border-left: none;
}
.seg button:hover:not([aria-pressed='true']) {
  color: var(--fg);
}
.seg button[aria-pressed='true'] {
  background: var(--accent);
  color: var(--accent-fg);
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/create.spec.ts`
Expected: PASS (4/4).

- [ ] **Step 6: Visual check**

Throwaway `web/_check.html` rendering `create.render(...)`; confirm three segmented controls, accent-filled selection, light + dark. Delete the file.

- [ ] **Step 7: Full gate + commit**

Run: `pnpm -C web format && pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
Expected: all green.

```bash
git add web/src/routes/create.ts web/src/ui/design.css web/tests/component/create.spec.ts
git commit -m "feat(web): segmented controls on the create form" -- web/src/routes/create.ts web/src/ui/design.css web/tests/component/create.spec.ts
```

---

## Task 3: lobby — team colors, copy feedback, join-modal surface

Add `data-team` to seats (CSS drives the team-colored left border), add transient "Copied!" feedback to the share button, and align the join modal to the panel surface. New component spec for the copy behavior.

**Files:**
- Create: `web/tests/component/lobby.spec.ts`
- Modify: `web/src/routes/lobby.ts`
- Modify: `web/src/ui/design.css` (seat-grid + `.join-modal`)

- [ ] **Step 1: Write the failing test**

Create `web/tests/component/lobby.spec.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderLobby } from '../../src/routes/lobby';
import type { Resources } from '../../src/routes/play-resources';
import type { ChallengeStatus } from '../../src/routes/boot';

function makeArgs() {
  const resources: Resources = { cleanups: [], ws: null, pollTimer: null, orchestrator: null };
  const initialStatus: ChallengeStatus = {
    challenge_id: 'chal-1',
    max_points: 500,
    seats: [],
    status: 'open',
    expires_at_epoch_secs: 0,
  };
  return {
    root: document.getElementById('root')!,
    resources,
    shortId: 'abc123',
    challengeId: 'chal-1',
    initialStatus,
  };
}

describe('lobby route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('tags every seat with its team', () => {
    renderLobby(makeArgs());
    // empty seats with no session render as joinable buttons, one per seat
    expect(document.querySelectorAll('.seat-grid [data-team]')).toHaveLength(4);
    expect(document.querySelectorAll('.seat-grid [data-team="1"]')).toHaveLength(2);
    expect(document.querySelectorAll('.seat-grid [data-team="2"]')).toHaveLength(2);
  });

  it('shows "Copied!" after a successful copy', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    renderLobby(makeArgs());
    const copyBtn = [...document.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Copy',
    )!;
    copyBtn.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('/play/abc123'));
    const copied = [...document.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Copied!',
    );
    expect(copied).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/lobby.spec.ts`
Expected: FAIL — `[data-team]` not present yet, and the button label never changes to "Copied!".

- [ ] **Step 3: Add the `copied` signal**

In `web/src/routes/lobby.ts`, add after the `errorMsg` signal (line 30):

```ts
  const copied = signal(false);
```

- [ ] **Step 4: Add copy feedback to `copyShareLink`**

Replace the `copyShareLink` function (lines 100–107) with:

```ts
  const copyShareLink = async (): Promise<void> => {
    const url = `${location.origin}/play/${args.shortId}`;
    try {
      await navigator.clipboard.writeText(url);
      copied.value = true;
      setTimeout(() => {
        copied.value = false;
      }, 1500);
    } catch {
      // ignore
    }
  };
```

- [ ] **Step 5: Add `data-team` to the three seat branches**

In the `.seat-grid` map (lines 124–147), add `data-team=${SEAT_TEAMS[s]}` to each rendered seat element.

Occupant branch:

```ts
              return html`<div
                class="seat-taken ${occupant.player_id === myPlayerId.value ? 'mine' : ''}"
                data-team=${SEAT_TEAMS[s]}
              >
                <strong>Seat ${s}</strong>
                <span>Team ${SEAT_TEAMS[s]}</span>
                <span>${occupant.name ?? 'Player'}</span>
                ${occupant.player_id === myPlayerId.value ? html`<small>(You)</small>` : null}
              </div>`;
```

Open, non-clickable branch:

```ts
              return html`<div class="seat-open" data-team=${SEAT_TEAMS[s]}>
                <strong>Seat ${s}</strong>
                <span>Team ${SEAT_TEAMS[s]}</span>
                <span>Open</span>
              </div>`;
```

Open, clickable (join) branch — keep `class="seat-open btn btn--primary"` (e2e pins `button.seat-open`):

```ts
            return html`<button
              class="seat-open btn btn--primary"
              data-team=${SEAT_TEAMS[s]}
              @click=${() => onJoinClick(s)}
            >
              <strong>Seat ${s}</strong>
              <span>Team ${SEAT_TEAMS[s]}</span>
            </button>`;
```

- [ ] **Step 6: Make the Copy button reactive**

Replace the share-link Copy button (line 179) with:

```ts
          ${button({
            label: copied.value ? 'Copied!' : 'Copy',
            onClick: () => void copyShareLink(),
            variant: 'secondary',
          })}
```

- [ ] **Step 7: Add the seat-grid + join-modal CSS**

In `web/src/ui/design.css`, add the team-color mapping and a left border. Add after the `.seat-grid { … }` block (around line 400):

```css
.seat-grid [data-team='1'] {
  --team: var(--accent);
}
.seat-grid [data-team='2'] {
  --team: var(--accent-2);
}
```

Then add a `border-left` to the `.seat-taken, .seat-open` rule (insert the one line after its existing `border:` line):

```css
.seat-taken,
.seat-open {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-3);
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  border-left: 3px solid var(--team, var(--border));
  background: var(--surface-raised);
}
```

And replace the `.join-modal` block (lines 436–445) — swap the `color-mix` background for the panel surface:

```css
.join-modal {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: var(--space-2);
  padding: var(--space-3);
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--surface-raised);
  box-shadow: var(--shadow-1);
  width: 100%;
}
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/lobby.spec.ts`
Expected: PASS (2/2).

- [ ] **Step 9: Visual check**

Throwaway `web/_check.html` calling `renderLobby({...})` with a couple of seats filled; confirm team-colored left borders (teal A/C, orange B/D), `.mine` accent highlight, "Copied!" feedback on click, panel-surfaced join modal. Delete the file.

- [ ] **Step 10: Full gate + commit**

Run: `pnpm -C web format && pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
Expected: all green (e2e is not part of this gate; `button.seat-open`, `.join-modal input`, and the "Join" button name are unchanged).

```bash
git add web/src/routes/lobby.ts web/src/ui/design.css web/tests/component/lobby.spec.ts
git commit -m "feat(web): team-colored lobby seats and copy feedback" -- web/src/routes/lobby.ts web/src/ui/design.css web/tests/component/lobby.spec.ts
```

---

## Task 4: profile — panel surface + games row

Put the profile page on a `.panel` and clean up the games list rows.

**Files:**
- Modify: `web/src/routes/profile.ts:36` and `:55-67`
- Modify: `web/src/ui/design.css` (`.profile-games`)
- Modify: `web/tests/component/profile.spec.ts` (add one assertion)

- [ ] **Step 1: Extend the test (failing)**

In `web/tests/component/profile.spec.ts`, inside the existing `'renders the username and games list on success'` test, add this assertion just before `cleanup();`:

```ts
    expect(document.querySelector('.profile-page.panel')).not.toBeNull();
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C web exec vitest run --project=component tests/component/profile.spec.ts`
Expected: FAIL — the section is `class="profile-page"` (no `panel`).

- [ ] **Step 3: Add the `panel` class to the section**

In `web/src/routes/profile.ts`, change line 36:

```ts
          <section class="profile-page panel">
```

(Applying `.panel` as an extra class — rather than a wrapper `<div>` — keeps the DOM/state structure and the deliberate eager-signal-read flow untouched.)

- [ ] **Step 4: Polish the games row markup**

Replace the `showList` `<ul>` (lines 55–67) with:

```ts
            ${showList
              ? html`<ul class="profile-games">
                  ${g.map(
                    (entry) =>
                      html`<li>
                        <code>${entry.game_id.slice(0, 8)}</code>
                        <span class="profile-games__seat">Seat ${entry.seat_index}</span>
                      </li>`,
                  )}
                </ul>`
              : nothing}
```

(The `.slice(0, 8)` game id stays inside `<code>`, so the `g1abcdef` assertion still holds.)

- [ ] **Step 5: Add the games-row CSS**

In `web/src/ui/design.css`, replace the `.profile-games` block (lines 620–627):

```css
.profile-games {
  list-style: none;
  padding: 0;
  margin: 0;
}
.profile-games li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2) 0;
  border-bottom: 1px solid var(--border);
}
.profile-games code {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--fg);
}
.profile-games__seat {
  font-size: var(--text-sm);
  color: var(--fg-muted);
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `pnpm -C web exec vitest run --project=component tests/component/profile.spec.ts`
Expected: PASS (2/2).

- [ ] **Step 7: Visual check**

Throwaway `web/_check.html` calling `profile.render({username:'alice'}, …)` with `fetch` stubbed (copy the stub from the spec) — confirm the page sits on a card and the games rows read cleanly. Delete the file.

- [ ] **Step 8: Full gate + commit**

Run: `pnpm -C web format && pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
Expected: all green.

```bash
git add web/src/routes/profile.ts web/src/ui/design.css web/tests/component/profile.spec.ts
git commit -m "feat(web): profile on panel surface with cleaner game rows" -- web/src/routes/profile.ts web/src/ui/design.css web/tests/component/profile.spec.ts
```

---

## Task 5: settings — panel surface + token-ized "Saved." line

Put the settings form on a `.panel` and replace the broken inline `var(--color-accent)` (an undefined custom property — a silent no-op today) with a `.field-success` class.

**Files:**
- Modify: `web/src/routes/settings.ts:65` and `:71`
- Modify: `web/src/ui/design.css` (add `.field-success`)
- Modify: `web/tests/component/settings.spec.ts` (two assertions)

- [ ] **Step 1: Extend the tests (failing)**

In `web/tests/component/settings.spec.ts`:

In `'renders email, current_password, and new_password fields'`, add before `cleanup();`:

```ts
    expect(document.querySelector('.form-page.panel')).not.toBeNull();
```

In `'save calls updateEmail when email changed'`, add after the `expect(upd)…` line and before `cleanup();`:

```ts
    expect(document.querySelector('.field-success')?.textContent).toContain('Saved');
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm -C web exec vitest run --project=component tests/component/settings.spec.ts`
Expected: FAIL — section lacks `panel`, and the saved line uses an inline `style`, not `.field-success`.

- [ ] **Step 3: Add the `panel` class to the section**

In `web/src/routes/settings.ts`, change line 65:

```ts
        <section class="form-page panel">
```

- [ ] **Step 4: Token-ize the "Saved." line**

In `web/src/routes/settings.ts`, change line 71 from:

```ts
          ${saved.value ? html`<p style="color: var(--color-accent)">Saved.</p>` : nothing}
```

to:

```ts
          ${saved.value ? html`<p class="field-success">Saved.</p>` : nothing}
```

- [ ] **Step 5: Add the `.field-success` CSS**

In `web/src/ui/design.css`, add right after the `.field-error { … }` block (around line 476):

```css
.field-success {
  color: var(--success);
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `pnpm -C web exec vitest run --project=component tests/component/settings.spec.ts`
Expected: PASS (4/4).

- [ ] **Step 7: Visual check**

Throwaway `web/_check.html` that sets `session.currentUser.value = {…}` then calls `settings.render(…)`; confirm the form sits on a card and (after a mocked save) "Saved." renders in the success color. Delete the file.

- [ ] **Step 8: Full gate + commit**

Run: `pnpm -C web format && pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check`
Expected: all green.

```bash
git add web/src/routes/settings.ts web/src/ui/design.css web/tests/component/settings.spec.ts
git commit -m "feat(web): settings on panel surface, token-ize saved message" -- web/src/routes/settings.ts web/src/ui/design.css web/tests/component/settings.spec.ts
```

---

## Final verification (after all tasks)

- [ ] Run the full gate once more: `pnpm -C web format:check && pnpm -C web build && pnpm -C web test && pnpm -C web lint`
- [ ] Confirm no throwaway `web/_check.html` (or similar) remains: `git status --porcelain` is clean.
- [ ] Optional but recommended — run the e2e challenge flow to confirm create→lobby→game still works: `pnpm -C web test:e2e tests/e2e/flows/challenge.spec.ts` (requires the backend/dev harness the e2e suite normally boots; skip if unavailable and note it).
- [ ] Hand off to `superpowers:finishing-a-development-branch`.

## Self-review notes (addressed)

- **Spec coverage:** `.seg` (Task 2), `.panel`+`--accent-fg`+`.btn--primary` (Task 1), team-colored seats + copy feedback + join-modal (Task 3), profile panel+rows (Task 4), settings panel + token-ized saved line (Task 5). All spec sections covered.
- **Selector preservation:** `Create`/`Join` button names, `button.seat-open`, `.join-modal input`, `#email`/`#current_password`/`#new_password`, `data-testid=save`, profile `g1abcdef` text — all retained (called out per task).
- **Type consistency:** `SEAT_TEAMS` values are `'1'`/`'2'` (strings) → `data-team` and CSS `[data-team='1']` match. `aria-pressed=${boolean}` renders the strings `"true"`/`"false"` (lit-html), matching the `[aria-pressed='true']` selector and the test queries.
