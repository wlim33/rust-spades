# spades-ts — Plan 4: Polish & Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Take the working app from Plan 3 to a shippable product: loading/empty/error UX, mobile-responsive layout, accessibility baseline, footer/meta, CI green on every push, production deploy, and cutover from `personal-site/spades/` to `spades.wlim.dev`.

**Architecture:** All frontend work plugs into existing modules (Toast as a shared signal; route files for skeletons/empty states; CSS-only mobile pass; ARIA in components). Cutover is procedural: deploy script + redirect from personal-site + retire the old static files.

**Tech Stack:** No new runtime deps. Adds GitHub Actions workflow file and a deploy shell script.

**Reference spec:** `/Users/wlim/Projects/spades-ts/docs/superpowers/specs/2026-05-11-spades-ts-design.md` (§ 4 error tiers; § 6 build/deploy/CI; § 7 cutover)

**Deferred items revisited:**

- Plan 2 / Task 17 follow-up: hand in WS payload (server-side optimization; skip if server doesn't expose it).
- Plan 3 / Task 13: E2E profile + history once a server seed is available (Task 11 here decides).
- Plan 3 caveat: OAuth-pending banner on `/` (Task 5 here covers).

---

## Files this plan creates or modifies

| Path                            | Action | Responsibility                                            |
| ------------------------------- | ------ | --------------------------------------------------------- |
| `src/state/toast.ts`            | create | Global toast signal + helpers                             |
| `src/ui/components/toast.ts`    | create | Toast template, auto-dismiss                              |
| `src/ui/templates.ts`           | modify | Mount toast in app shell                                  |
| `src/api/client.ts`             | modify | Surface ApiError to toast for unhandled rejections        |
| `src/routes/play.ts`            | modify | Loading skeleton + error retry; empty in-flight cards     |
| `src/routes/profile.ts`         | modify | Empty state for zero games                                |
| `src/routes/home.ts`            | modify | Empty/loading state for queue sizes; OAuth-pending banner |
| `src/ui/design.css`             | modify | Responsive breakpoints, focus rings, reduced-motion       |
| `index.html`                    | modify | Favicon, theme-color, OG/Twitter meta, viewport-fit       |
| `src/ui/components/footer.ts`   | create | Build version + GitHub link                               |
| `src/ui/templates.ts`           | modify | Mount footer in app shell                                 |
| `.github/workflows/ci.yml`      | create | Lint / type-check / unit / component / e2e                |
| `scripts/deploy.sh`             | create | Build + ship to server                                    |
| `README.md`                     | modify | Document deploy, static-serving expectations              |
| `personal-site/_redirects`      | modify | Redirect `/spades/*` to `https://spades.wlim.dev/*`       |
| `personal-site/spades/`         | delete | After grace period                                        |
| `tests/component/toast.spec.ts` | create | Auto-dismiss, multi-toast                                 |

---

## Task 1: Toast component

**Files:**

- Create: `src/state/toast.ts`, `src/ui/components/toast.ts`, `tests/component/toast.spec.ts`
- Modify: `src/ui/templates.ts`, `src/ui/design.css`

- [ ] **Step 1: Create `src/state/toast.ts`**

```ts
import { signal } from '@preact/signals-core';

export type ToastKind = 'info' | 'error' | 'success';
export type Toast = { id: number; kind: ToastKind; message: string };

const toasts = signal<Toast[]>([]);
let nextId = 1;

function show(kind: ToastKind, message: string, ttlMs = 4000): void {
  const id = nextId++;
  toasts.value = [...toasts.value, { id, kind, message }];
  setTimeout(() => dismiss(id), ttlMs);
}

function dismiss(id: number): void {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

export const toast = {
  toasts,
  info: (m: string) => show('info', m),
  error: (m: string) => show('error', m),
  success: (m: string) => show('success', m),
  dismiss,
};
```

- [ ] **Step 2: Create `src/ui/components/toast.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';
import { toast } from '../../state/toast';

export function toastStack(): TemplateResult {
  return html`<div class="toast-stack" role="status" aria-live="polite">
    ${toast.toasts.value.map(
      (t) =>
        html`<div class=${`toast toast--${t.kind}`} data-testid="toast">
          <span>${t.message}</span>
          <button
            type="button"
            class="toast__close"
            aria-label="Dismiss"
            @click=${() => toast.dismiss(t.id)}
          >
            ×
          </button>
        </div>`,
    )}
  </div>`;
}
```

- [ ] **Step 3: Mount in `src/ui/templates.ts`**

Replace the file contents:

```ts
import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';
import { toastStack } from './components/toast';

export function appShell(children: TemplateResult): TemplateResult {
  return html`${header()}
    <section class="page">${children}</section>
    ${toastStack()}`;
}
```

- [ ] **Step 4: Append toast styles to `src/ui/design.css`**

```css
.toast-stack {
  position: fixed;
  right: var(--space-4);
  bottom: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  z-index: 50;
  max-width: 90vw;
}
.toast {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  background: white;
  color: var(--color-fg);
  min-width: 200px;
}
.toast--error {
  background: #fff1f1;
  color: #911;
  border: 1px solid #f3c8c8;
}
.toast--success {
  background: #effaf3;
  color: #1a6b3c;
  border: 1px solid #c8edd5;
}
.toast--info {
  background: white;
  border: 1px solid rgba(0, 0, 0, 0.08);
}
.toast__close {
  background: none;
  border: none;
  font-size: var(--font-size-lg);
  cursor: pointer;
  color: inherit;
  line-height: 1;
  padding: 0;
}
@media (prefers-reduced-motion: no-preference) {
  .toast {
    animation: toast-in 200ms ease-out;
  }
  @keyframes toast-in {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
}
```

- [ ] **Step 5: Write component test**

`tests/component/toast.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { html, render } from 'lit-html';
import { toast } from '../../src/state/toast';
import { toastStack } from '../../src/ui/components/toast';

describe('toast', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    toast.toasts.value = [];
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it('renders nothing initially', () => {
    render(html`${toastStack()}`, document.getElementById('root')!);
    expect(document.querySelectorAll('[data-testid=toast]').length).toBe(0);
  });

  it('error() pushes a toast', () => {
    toast.error('Boom');
    render(html`${toastStack()}`, document.getElementById('root')!);
    expect(document.querySelectorAll('[data-testid=toast]').length).toBe(1);
    expect(document.querySelector('[data-testid=toast]')?.textContent).toContain('Boom');
  });

  it('auto-dismisses after 4 seconds', () => {
    toast.info('Howdy');
    vi.advanceTimersByTime(4001);
    expect(toast.toasts.value.length).toBe(0);
  });

  it('stacks multiple toasts in order', () => {
    toast.info('A');
    toast.info('B');
    expect(toast.toasts.value.map((t) => t.message)).toEqual(['A', 'B']);
  });
});
```

- [ ] **Step 6: Run test**

Run: `pnpm test:component`
Expected: 4 cases pass.

- [ ] **Step 7: Commit**

```bash
git add src/state/toast.ts src/ui/components/toast.ts src/ui/templates.ts src/ui/design.css tests/component/toast.spec.ts
git commit -m "feat: global toast component"
```

---

## Task 2: Surface caught errors via toast

**Files:**

- Modify: `src/routes/play.ts`, `src/routes/home.ts`, `src/routes/login.ts`, `src/routes/signup.ts`, `src/routes/settings.ts`

Existing `console.error('… failed', e)` calls and silent catches should emit toasts. Auth forms keep their inline error display — toasts are for non-form failures.

- [ ] **Step 1: Replace `console.error('bet failed', e)` and similar in `src/routes/play.ts`**

Find every `console.error(...)` in `play.ts` and replace with:

```ts
import { toast } from '../state/toast';
// ...
toast.error('Bet failed.');
toast.error('Play failed.');
toast.error('Failed to fetch game state.');
toast.error('Connection lost.');
```

Specifically:

- Inside the bet `onBet` handler.
- Inside the play-card handler.
- Inside the polling error path.
- Inside the WS `onError` callback (add one if missing).

- [ ] **Step 2: Same in `src/routes/home.ts` (matchmaking failure)**

Replace the catch in `onSeek`:

```ts
import { toast } from '../state/toast';
// ...
onError: () => {
  toast.error('Failed to find match.');
  activeSeek?.close();
  activeSeek = null;
},
```

- [ ] **Step 3: Type-check**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add src/routes/play.ts src/routes/home.ts
git commit -m "refactor: surface caught errors via toast"
```

---

## Task 3: Loading skeleton for /play boot

**Files:**

- Modify: `src/routes/play.ts`, `src/ui/design.css`

Today the play route shows "Loading game…" while it figures out which boot path to use. Replace with a skeleton table so the layout doesn't jump on success.

- [ ] **Step 1: Add skeleton CSS**

Append to `src/ui/design.css`:

```css
.skeleton {
  background: linear-gradient(
    90deg,
    rgba(0, 0, 0, 0.05) 25%,
    rgba(0, 0, 0, 0.1) 37%,
    rgba(0, 0, 0, 0.05) 63%
  );
  background-size: 400% 100%;
  animation: skeleton-loading 1.4s ease infinite;
  border-radius: var(--radius-sm);
}
@keyframes skeleton-loading {
  0% {
    background-position: 100% 50%;
  }
  100% {
    background-position: 0 50%;
  }
}
.skeleton-card {
  width: 46px;
  height: 64px;
}
.skeleton-row {
  display: flex;
  gap: 4px;
}
.skeleton-game {
  display: grid;
  grid-template-columns: 1fr 2fr 1fr;
  grid-template-rows: auto 1fr auto;
  grid-template-areas: 'north north north' 'west center east' 'south south south';
  gap: var(--space-3);
  width: 100%;
  max-width: 720px;
  min-height: 360px;
}
@media (prefers-reduced-motion: reduce) {
  .skeleton {
    animation: none;
    background: rgba(0, 0, 0, 0.06);
  }
}
```

- [ ] **Step 2: Replace the loading shell in `src/routes/play.ts`**

Find:

```ts
render(appShell(html`<p>Loading game…</p>`), root);
```

Replace with:

```ts
render(
  appShell(html`
    <div class="skeleton-game" aria-busy="true" aria-label="Loading game">
      <div class="skeleton" style="grid-area: north; height: 24px; width: 120px;"></div>
      <div class="skeleton skeleton-card" style="grid-area: west;"></div>
      <div class="skeleton-row" style="grid-area: center; justify-content: center;">
        <div class="skeleton skeleton-card"></div>
        <div class="skeleton skeleton-card"></div>
        <div class="skeleton skeleton-card"></div>
        <div class="skeleton skeleton-card"></div>
      </div>
      <div class="skeleton skeleton-card" style="grid-area: east; justify-self: end;"></div>
      <div class="skeleton-row" style="grid-area: south; justify-content: center;">
        ${Array.from({ length: 13 }, () => html`<div class="skeleton skeleton-card"></div>`)}
      </div>
    </div>
  `),
  root,
);
```

- [ ] **Step 3: Manual smoke**

Run: `pnpm dev`
Throttle to Slow 3G in DevTools; navigate to `/play/some-id`; observe the skeleton table instead of the "Loading…" text.

- [ ] **Step 4: Commit**

```bash
git add src/routes/play.ts src/ui/design.css
git commit -m "feat: skeleton table during play boot"
```

---

## Task 4: Empty states

**Files:**

- Modify: `src/routes/profile.ts`, `src/routes/home.ts`

- [ ] **Step 1: Profile — already handles `games.length === 0` in Plan 3 with "No games yet."**

Verify the message reads well and replace with a tidier card:

Inside `profile.ts` template:

```ts
games.length === 0
  ? html`<div class="empty-state">
      <p><strong>${prof.display_name || prof.username}</strong> hasn't finished any games yet.</p>
    </div>`
  : ...
```

- [ ] **Step 2: Home — show "No one's waiting" for queues that are empty**

The current quickplay row shows the timer label. After Plan 2 Task 13 (queue polling), `queueCountFor(timer)` returns 0 when empty. Add the count line below each button:

In `src/routes/home.ts` template, replace the quickplay map:

```ts
${QUICKPLAY_TIMERS.map((t) => {
  const count = queueCountFor(t.value!); // null timer is filtered out below
  return html`<div class="quickplay-col">
    ${button({ label: t.label, onClick: () => onSeek(t.value), variant: 'primary' })}
    <span class="queue-count">${count > 0 ? `${count} waiting` : 'No one waiting'}</span>
  </div>`;
})}
```

Filter out `null` timers from `QUICKPLAY_TIMERS` (we use non-null timer presets for quickplay).

Add an effect to subscribe to queue sizes on mount:

```ts
import { effect } from '@preact/signals-core';
import { startQueuePoll, stopQueuePoll, queueSizes, queueCountFor } from '../state/menu';

// inside home.render(), before the first render:
startQueuePoll();
const disposeQueue = effect(() => {
  void queueSizes.value; // depend
  rerender();
});

// return cleanup:
return () => {
  stopQueuePoll();
  disposeQueue();
  render(html``, root);
};
```

- [ ] **Step 3: Empty styles**

Append to `src/ui/design.css`:

```css
.empty-state {
  padding: var(--space-6);
  border: 1px dashed rgba(0, 0, 0, 0.12);
  border-radius: var(--radius-md);
  text-align: center;
  color: var(--color-muted);
}
.quickplay-col {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  align-items: center;
}
.queue-count {
  font-size: 0.75rem;
  color: var(--color-muted);
}
```

- [ ] **Step 4: Manual smoke**

With rust-spades running, visit `/`; verify the queue count text. Visit `/u/<some-username-with-no-games>`; verify the empty state.

- [ ] **Step 5: Commit**

```bash
git add src/routes/profile.ts src/routes/home.ts src/ui/design.css
git commit -m "feat: empty states for queues + profile games"
```

---

## Task 5: OAuth-pending banner fallback (from Plan 3 caveat)

**Files:**

- Modify: `src/routes/home.ts`

When the server has set the `__oauth_pending` cookie but `main.ts`'s sentinel didn't fire (cross-tab, cleared storage), the user lands on `/` signed-out with an unusable cookie. Show a banner: "Finish signing in" → navigates to `/auth/oauth/complete`.

The cookie is HttpOnly so we can't read it. The heuristic: if `/auth/me` returned 401 (i.e. `session.currentUser.value === null`) AND a known-good check against `/auth/oauth/complete` reveals pending state. Cleanest is a lightweight server endpoint, but absent that, we can probe by POSTing to `/auth/oauth/complete` with an obviously-invalid username — if the server returns "username invalid" rather than "no pending session", we know a pending cookie is present.

Pragmatically: **don't probe.** Show the banner only when the localStorage sentinel was set (we cleared it in `main.ts`, so re-read storage before that clearing). Cleanup that flow: change `main.ts` to detect the marker and route to `/auth/oauth/complete` _only_; on the home route, look for a small `spades_oauth_lingering` flag we set if the marker was found at boot. If the user then arrives on `/` (e.g. by clicking the home link from `/auth/oauth/complete`), we show a banner offering to resume.

- [ ] **Step 1: In `src/main.ts`, set a "lingering" flag when oauth marker fires**

Where `oauthMarker` is consumed and we redirect to `/auth/oauth/complete`:

```ts
if (oauthMarker && session.currentUser.value === null) {
  try {
    sessionStorage.setItem('spades_oauth_lingering', '1');
  } catch {}
  history.replaceState(null, '', '/auth/oauth/complete');
}
```

In `src/routes/oauth-complete.ts`, on successful completion or explicit cancel, clear it:

```ts
try {
  sessionStorage.removeItem('spades_oauth_lingering');
} catch {}
```

- [ ] **Step 2: Banner in `src/routes/home.ts`**

Inside the home template, above the menu:

```ts
const lingering = (() => {
  try { return sessionStorage.getItem('spades_oauth_lingering') === '1'; } catch { return false; }
})();

// ...
${lingering ? html`<div class="banner">
  <span>Finish signing in to keep your account.</span>
  <a class="btn btn--primary" href="/auth/oauth/complete" data-link>Continue</a>
  <button class="btn btn--secondary" type="button" @click=${() => {
    try { sessionStorage.removeItem('spades_oauth_lingering'); } catch {}
    rerender();
  }}>Dismiss</button>
</div>` : null}
```

- [ ] **Step 3: Banner styles**

Append:

```css
.banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3);
  background: #fff7e6;
  border: 1px solid #f6cf7a;
  border-radius: var(--radius-md);
  width: 100%;
  max-width: 480px;
  margin-bottom: var(--space-4);
}
.banner .btn {
  padding: var(--space-2) var(--space-3);
}
```

- [ ] **Step 4: Commit**

```bash
git add src/main.ts src/routes/home.ts src/routes/oauth-complete.ts src/ui/design.css
git commit -m "feat: OAuth-pending banner fallback on home"
```

---

## Task 6: Mobile-responsive layout

**Files:**

- Modify: `src/ui/design.css`

The game table needs a smaller layout on portrait phones (less than ~600px wide). North seat sits above center; west/east tuck under the trick; south stays full-width below.

- [ ] **Step 1: Append responsive rules**

```css
/* Phones / portrait tablets */
@media (max-width: 600px) {
  main#root {
    padding: var(--space-3) var(--space-2);
  }
  .site-header {
    padding: var(--space-2) var(--space-3);
  }
  .page {
    padding: var(--space-3) var(--space-2);
  }
  .spades-table {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: auto auto auto auto;
    grid-template-areas: 'north north' 'west east' 'center center' 'south south';
    min-height: 0;
  }
  .seat-east {
    justify-self: end;
  }
  .spades-table-center {
    min-height: 160px;
  }
  .spades-bets {
    grid-template-columns: repeat(5, 1fr);
  }
  .toast-stack {
    left: var(--space-3);
    right: var(--space-3);
  }
  .form-page,
  .profile-page {
    padding: 0 var(--space-2);
  }
  .menu {
    gap: var(--space-2);
  }
  .menu__quickplay {
    grid-template-columns: repeat(3, 1fr);
  }
  .card {
    width: 40px;
    height: 56px;
    font-size: 12px;
  }
  .skeleton-card {
    width: 40px;
    height: 56px;
  }
}

/* Very small phones */
@media (max-width: 360px) {
  .card {
    width: 36px;
    height: 50px;
  }
  .skeleton-card {
    width: 36px;
    height: 50px;
  }
  .spades-bets {
    grid-template-columns: repeat(4, 1fr);
  }
}
```

- [ ] **Step 2: Test responsive layout**

Run: `pnpm dev`
In DevTools, toggle device emulation (iPhone SE / iPhone 12 / Pixel 5). Verify:

- Header doesn't overflow.
- Game table is usable; cards readable.
- Forms (login, signup, settings) fit and inputs aren't covered by toast.
- Bet buttons wrap to two rows.

- [ ] **Step 3: Commit**

```bash
git add src/ui/design.css
git commit -m "feat: responsive layout for phones"
```

---

## Task 7: Accessibility pass

**Files:**

- Modify: `src/ui/design.css`, `src/ui/components/header.ts`, `src/ui/components/button.ts`, `src/routes/play.ts`

Focus on the cheap, high-leverage wins: visible focus rings, `aria-live` on the trick area, labels on icon-only controls, keyboard target sizes.

- [ ] **Step 1: Focus styles in design.css**

Append:

```css
:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}
.btn:focus-visible {
  outline-offset: 3px;
}

/* Tap target minimum on touch */
@media (pointer: coarse) {
  .btn {
    min-height: 44px;
  }
  .avatar-menu__btn {
    min-height: 44px;
  }
  .toast__close {
    min-width: 32px;
    min-height: 32px;
  }
}
```

- [ ] **Step 2: aria-live for game state**

In `src/routes/play.ts`'s `centerText` (the "Your turn / Waiting for X / Trick N/13" line in the table center), wrap it:

Find:

```ts
<span class="spades-center-text">${centerText}</span>
```

Replace with:

```ts
<span class="spades-center-text" aria-live="polite" aria-atomic="true">${centerText}</span>
```

- [ ] **Step 3: Header — make Sign in / avatar reachable from keyboard**

Header already uses `<a>` and `<summary>` which are keyboard-accessible by default. No change needed; verify by tabbing through the page.

- [ ] **Step 4: Cards — add aria-label for accessibility (read by screen readers)**

In `src/cards/card-el.ts`, after setting textContent, add `aria-label`:

```ts
export function createFront(card: Card): CardEl {
  const el = document.createElement('div') as CardEl;
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
  el.setAttribute('role', 'button');
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);
  el._cm = { x: 0, y: 0 };
  return el;
}
```

Same in `setFront`:

```ts
export function setFront(el: CardEl, card: Card): void {
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);
}
```

Card backs: in `createBack`, add `aria-hidden="true"` so screen readers don't announce opponent cards.

- [ ] **Step 5: Manual smoke**

Tab through `/` — focus rings on every interactive element. Use VoiceOver / NVDA on a card in PLAYING phase; screen reader announces the card. The trick-area aria-live announces "Your turn" / "Waiting for X" updates.

- [ ] **Step 6: Commit**

```bash
git add src/ui/design.css src/routes/play.ts src/cards/card-el.ts
git commit -m "feat: a11y baseline (focus, aria-live, card labels)"
```

---

## Task 8: Footer + meta tags

**Files:**

- Create: `src/ui/components/footer.ts`
- Modify: `src/ui/templates.ts`, `index.html`, `src/ui/design.css`

- [ ] **Step 1: Create `src/ui/components/footer.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';

declare const __BUILD_VERSION__: string;

export function footer(): TemplateResult {
  return html`<footer class="site-footer">
    <span>spades-ts</span>
    <span>·</span>
    <a href="https://github.com/wlim/spades-ts" target="_blank" rel="noopener noreferrer">source</a>
    <span>·</span>
    <span class="footer-version">${__BUILD_VERSION__}</span>
  </footer>`;
}
```

- [ ] **Step 2: Mount in `src/ui/templates.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';
import { header } from './components/header';
import { footer } from './components/footer';
import { toastStack } from './components/toast';

export function appShell(children: TemplateResult): TemplateResult {
  return html`${header()}
    <section class="page">${children}</section>
    ${footer()}${toastStack()}`;
}
```

- [ ] **Step 3: Footer styles**

```css
.site-footer {
  width: 100%;
  display: flex;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-3);
  color: var(--color-muted);
  font-size: var(--font-size-sm);
  border-top: 1px solid rgba(0, 0, 0, 0.06);
}
.footer-version {
  font-family: ui-monospace, monospace;
  opacity: 0.8;
}
.site-footer a {
  color: inherit;
}
```

- [ ] **Step 4: `index.html` meta + favicon**

Replace the `<head>` section:

```html
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <meta name="theme-color" content="#f7ede2" />
  <title>Spades</title>
  <meta
    name="description"
    content="Play 4-player Spades online — quick match, with friends, or against the computer."
  />
  <meta property="og:title" content="Spades" />
  <meta
    property="og:description"
    content="Play 4-player Spades online — quick match, with friends, or against the computer."
  />
  <meta property="og:type" content="website" />
  <meta property="og:url" content="https://spades.wlim.dev/" />
  <meta name="twitter:card" content="summary" />
  <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
  <link rel="stylesheet" href="/src/ui/design.css" />
</head>
```

- [ ] **Step 5: Add a tiny SVG favicon**

Create `public/favicon.svg` (Vite serves files from `public/` at root):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="6" fill="#011627"/><text x="50%" y="62%" font-family="system-ui,sans-serif" font-size="22" text-anchor="middle" fill="#f7ede2">♠</text></svg>
```

- [ ] **Step 6: Type-check**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds (the `declare const __BUILD_VERSION__` line satisfies the type checker for the Vite-injected global).

- [ ] **Step 7: Commit**

```bash
git add src/ui/components/footer.ts src/ui/templates.ts src/ui/design.css index.html public/
git commit -m "feat: footer with build version + meta + favicon"
```

---

## Task 9: GitHub Actions CI

**Files:**

- Create: `.github/workflows/ci.yml`

The CI runs lint, type-check, unit, component, e2e. The E2E job needs rust-spades running; we'll pull a pre-built binary from a sibling repo action artifact or build from a pinned sha. Simplest reliable path: cargo build it in CI.

- [ ] **Step 1: Create `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  static:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'pnpm'
      - run: pnpm install --frozen-lockfile
      - run: pnpm openapi:check
      - run: pnpm tsc --noEmit -p tsconfig.json
      - run: pnpm lint
      - run: pnpm format:check

  unit-component:
    runs-on: ubuntu-latest
    needs: static
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'pnpm'
      - run: pnpm install --frozen-lockfile
      - run: pnpm test:unit
      - run: pnpm test:component

  e2e:
    runs-on: ubuntu-latest
    needs: static
    env:
      RUST_SPADES_REPO: wlim/rust-spades
      RUST_SPADES_REF: main
      VITE_API_URL: http://localhost:3000
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'pnpm'
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/cache@v4
        with:
          path: ~/.cargo/registry
          key: cargo-${{ runner.os }}
      - run: pnpm install --frozen-lockfile

      - name: Check out rust-spades
        uses: actions/checkout@v4
        with:
          repository: ${{ env.RUST_SPADES_REPO }}
          ref: ${{ env.RUST_SPADES_REF }}
          path: rust-spades

      - name: Build rust-spades
        working-directory: rust-spades
        run: cargo build --release -p spades-server

      - name: Start rust-spades
        working-directory: rust-spades
        run: |
          ./target/release/spades-server --port 3000 --insecure-cookies --cors-allow-origin http://localhost:5173 &
          for i in $(seq 1 30); do
            if curl -fsS http://localhost:3000/games >/dev/null 2>&1; then
              echo "spades-server up"; exit 0
            fi
            sleep 1
          done
          echo "spades-server did not come up"
          exit 1

      - name: Install Playwright browsers
        run: pnpm exec playwright install --with-deps chromium

      - run: pnpm test:e2e

      - name: Upload Playwright report
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-report
          path: playwright-report/
          retention-days: 14
```

- [ ] **Step 2: Pin rust-spades ref**

Replace `RUST_SPADES_REF: main` with a specific commit sha once the user provides one. Until then `main` is acceptable.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: lint / type-check / unit / component / e2e workflow"
```

---

## Task 10: Production deploy script

**Files:**

- Create: `scripts/deploy.sh`
- Modify: `README.md`

The script tars `dist/` and ships it. The shape depends on whether the user picks option (a) — rust-spades `ServeDir` — or (b) reverse proxy. The script is option-(a)-shaped (writes to a `./public/` next to the rust-spades binary on the server), but takes a target via env vars so option (b) is supported by changing the destination path.

- [ ] **Step 1: Create `scripts/deploy.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Env vars expected:
#   DEPLOY_HOST   — ssh destination (e.g. wlim@spades.wlim.dev)
#   DEPLOY_PATH   — absolute path on the host where dist/ lands (e.g. /srv/spades-ts/public)
#
# Examples:
#   DEPLOY_HOST=wlim@spades.wlim.dev DEPLOY_PATH=/srv/spades/public ./scripts/deploy.sh
#
# Assumes the host runs rust-spades with --static-dir $DEPLOY_PATH (or a reverse
# proxy that serves $DEPLOY_PATH as static).

if [[ -z "${DEPLOY_HOST:-}" ]] || [[ -z "${DEPLOY_PATH:-}" ]]; then
  echo "DEPLOY_HOST and DEPLOY_PATH must be set" >&2
  exit 1
fi

echo "Building production bundle…"
pnpm install --frozen-lockfile
pnpm build

echo "Shipping to $DEPLOY_HOST:$DEPLOY_PATH"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp -R dist/* "$tmp"/

# Stage to a temp dir on the host, then atomically swap.
ssh "$DEPLOY_HOST" "mkdir -p $DEPLOY_PATH.staging"
rsync -az --delete "$tmp"/ "$DEPLOY_HOST:$DEPLOY_PATH.staging/"
ssh "$DEPLOY_HOST" "rm -rf $DEPLOY_PATH.previous && \
  ( [ -d $DEPLOY_PATH ] && mv $DEPLOY_PATH $DEPLOY_PATH.previous || true ) && \
  mv $DEPLOY_PATH.staging $DEPLOY_PATH"

echo "Deployed."
```

Make it executable: `chmod +x scripts/deploy.sh`.

- [ ] **Step 2: Document in `README.md`**

Append a "Deploy" section:

````markdown
## Deploy

Production bundle is plain static files; serve from same origin as `rust-spades` to avoid CORS and cookie domain issues.

Two ways:

1. **rust-spades serves static** (recommended): run rust-spades with `--static-dir /srv/spades/public`. The server falls back to `index.html` for unknown paths that aren't API routes.
2. **Reverse proxy in front** (Caddy / nginx): serve `/srv/spades/public` for `/`, proxy `/games`, `/auth`, `/users`, `/matchmaking`, `/challenges`, `/player`, `/openapi.json` to rust-spades.

Either way, deploy with:

```sh
DEPLOY_HOST=wlim@spades.wlim.dev DEPLOY_PATH=/srv/spades/public ./scripts/deploy.sh
```
````

The script builds locally, ships via rsync, and swaps atomically.

````

- [ ] **Step 3: Build verification**

Run: `pnpm build`
Expected: `dist/index.html` + hashed `dist/assets/*` produced; type-check is clean.

- [ ] **Step 4: Commit**

```bash
git add scripts/deploy.sh README.md
git commit -m "build: production deploy script + docs"
````

---

## Task 11 (optional): Revisit deferred E2E — profile + history

**Files:**

- Create: `tests/e2e/profile.spec.ts` (if not already)

Only if rust-spades exposes a test-seed endpoint or a deterministic short-game mode (`--max-points 5` style). If neither, leave Plan 3 Task 13's deferral note in place and skip.

- [ ] **Step 1: Check the server**

`curl -s http://localhost:3000/openapi.json | jq '.paths | keys[]' | grep -i seed`

If empty, **skip the rest of this task.**

- [ ] **Step 2: Write the test from Plan 3 Task 13 Step 1**

(Copy verbatim from `2026-05-11-plan-3-account-aware-ux.md` § Task 13 Step 1.)

- [ ] **Step 3: Run + commit**

```bash
pnpm test:e2e -- profile
git add tests/e2e/profile.spec.ts
git commit -m "test: e2e profile + history (using server seed)"
```

---

## Task 12: Configure server-side static serving (user task — out of spades-ts repo)

This task lives in `rust-spades`, not `spades-ts`. Documented here so the rollout is reproducible.

- [ ] **Step 1: Patch rust-spades**

Add a `--static-dir <path>` flag. In the axum router, after API routes, install:

```rust
.fallback_service(
    tower_http::services::ServeDir::new(static_dir)
        .fallback(tower_http::services::ServeFile::new(static_dir.join("index.html")))
)
```

The `ServeFile::new` fallback handles SPA deep-links by serving `index.html` for anything that didn't match a static asset.

- [ ] **Step 2: Restart rust-spades on the host with `--static-dir /srv/spades/public`.**

- [ ] **Step 3: Smoke**

```sh
curl -fsS https://spades.wlim.dev/ | grep '<title>Spades</title>'
curl -fsS https://spades.wlim.dev/some/unknown/spa/path | grep '<title>Spades</title>'
curl -fsS https://spades.wlim.dev/games   # should still hit the API and return 200/JSON
```

(No commit in spades-ts; record any rust-spades changes there.)

---

## Task 13: Redirect old personal-site `/spades/*`

**Files:**

- Modify: `/Users/wlim/Projects/personal-site/_redirects`

- [ ] **Step 1: Replace the contents of `personal-site/_redirects`**

Before:

```
/spades/* /spades/index.html 200
```

After:

```
/spades/* https://spades.wlim.dev/:splat 301
```

This preserves `:splat` so `/spades/abc123` → `https://spades.wlim.dev/abc123`. Plan 2's URL scheme is `/play/:shortId` though, so you'll want:

```
/spades/      https://spades.wlim.dev/                 301
/spades/*     https://spades.wlim.dev/play/:splat      301
```

The first line catches `/spades` and `/spades/` cleanly; the second routes deep links to `/play/:shortId`.

- [ ] **Step 2: Commit in personal-site**

```bash
cd /Users/wlim/Projects/personal-site
git add _redirects
git commit -m "redirect: /spades/* to spades.wlim.dev"
```

- [ ] **Step 3: Smoke after deploy**

```sh
curl -sI https://wlim.dev/spades/abc | head -n 5
# Expect: HTTP/2 301 ... location: https://spades.wlim.dev/play/abc
```

---

## Task 14: Retire old personal-site `/spades/` content

**Files:**

- Delete: `/Users/wlim/Projects/personal-site/spades/`

Only after Task 13 is live and you've smoke-tested deep-link redirects in production for a few days.

- [ ] **Step 1: Confirm grace period elapsed**

You've left the redirects in place for at least 7 days. No support reports about broken links.

- [ ] **Step 2: Delete in personal-site**

```bash
cd /Users/wlim/Projects/personal-site
git rm -rf spades/
git commit -m "remove: retired spades/ now served from spades-ts"
```

The `_redirects` rule from Task 13 continues to forward any straggler links.

---

## Self-review

**Spec coverage (Phase 4 of the design doc):**

- Empty states → Task 4 ✓
- Loading skeletons → Task 3 ✓
- Mobile pass → Task 6 ✓
- E2E green in CI → Task 9 ✓
- Deploy to spades.wlim.dev → Tasks 10, 12 ✓
- Redirect from `wlim.dev/spades/*` → Task 13 ✓
- Delete `personal-site/spades/*` after grace → Task 14 ✓

**Cross-plan deferrals revisited:**

- Plan 3 OAuth-pending banner caveat → Task 5 ✓
- Plan 3 Task 13 profile E2E → Task 11 (conditional on server seed) ✓
- Plan 2 Task 17 hand-in-WS follow-up → out of scope here; if the server adds it, the WS callback in `routes/play.ts` shortens to a one-line change at that time.

**Placeholder scan:**

- Task 11 explicitly skip-able with a defined precondition (server seed). Acceptable.
- Task 12 is procedural (server-side patch outside this repo) — full instructions provided.
- No "TBD"/"fill in"/"similar to" anywhere else.

**Type consistency:**

- `__BUILD_VERSION__` declared in `footer.ts` and injected in `vite.config.ts` (Plan 1 Task 1 Step 9). Same string at both ends.
- `toast.toasts: Signal<Toast[]>` consumed by `toast.ts`; auto-dismiss timer set in `state/toast.ts` only.
- Static asset paths under `public/` are served at root by Vite; the `favicon.svg` reference in `index.html` matches.

**Open caveats for the reviewer:**

- The OAuth-pending banner (Task 5) uses sessionStorage as the lingering signal. If the user clears storage between OAuth callback and arriving on `/`, the banner won't show — but their pending cookie also expires in 15 minutes server-side (per `handlers_auth.rs` line 411), and they can always start the flow again. The banner is a convenience, not a correctness fix.
- The deploy script assumes you have rsync, ssh, and shell access to `spades.wlim.dev`. If you're deploying behind something like Fly.io or Render, replace `scripts/deploy.sh` with the platform's deploy step — the build output is unchanged.
- CI builds rust-spades from `main` by default. **Pin to a specific commit before merging this plan** (Task 9 Step 2). Floating `main` in CI is a flake source.
- I did not add a Lighthouse / web-vitals step to CI. The bundle should be small (vanilla TS + lit-html + signals + navaid ≈ 30-50KB gzipped), but a `pnpm exec bundlesize` or similar gate is worth adding once you have a baseline.
- I did not write a component test for the footer or the responsive layout — both are static; visual verification + the existing E2E smoke is enough.
