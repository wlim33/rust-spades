# Landing-page Leaderboard Preview — Design

**Date:** 2026-06-02
**Status:** Approved (pending implementation plan)

## Summary

Add a compact **Top players** leaderboard section to the landing page
(`web/src/routes/home.ts`), placed **below** the existing Quick-play / "other
ways to play" menu. It shows the **top 5** players with **all-time / this-month**
tabs and a link to the full `/leaderboard` page. It reuses the existing
leaderboard data layer and CSS, and is built as a **new, self-contained
component** so the proven `/leaderboard` route is left untouched.

The overriding constraint: the section must **not take away from the gameplay
join buttons**. It is therefore visually and structurally subordinate — a
sibling section *beneath* the menu, never inside it, and it degrades quietly
(muted, non-alarming states) so it can never compete with or disrupt the CTAs.

**No backend changes.** The server already caps every response at
`LEADERBOARD_SIZE = 10` and validates `period`; the widget slices to 5
client-side.

## Locked decisions

| Question | Decision |
| --- | --- |
| Placement | **Below the menu**, stacked (one column on mobile too). Join CTAs stay first and primary. |
| Content | **Top 5** rows, with **all-time / this-month** period tabs + a "View full leaderboard" link. |
| Code structure | **New component** `ui/components/leaderboard-preview.ts` that **reuses the data layer** (types + `request` + CSS). Full `/leaderboard` route untouched. |
| Row count source | Server caps at 10 (no `limit` param); widget slices `entries.slice(0, 5)`. |
| Backend | **No changes.** |

## Background (existing system)

- The frontend is **lit-html + `@preact/signals-core`** with a `navaid`-style
  router (`web/src/main.ts`). `appShell` wraps `header → .page → footer`
  (`ui/templates.ts`). `.page` is `display:flex; flex-direction:column;
  align-items:center; max-width:720px` (`ui/design.css:65`), so a child
  `section` with `max-width:360px` centers directly under the menu.
- **Home** (`routes/home.ts`) renders a `.menu` (`width:100%; max-width:360px`)
  containing Quick-play tiles + the friends/computers rows. It owns
  **module-level signals** (`quickplay`, `oauthBanner`), runs a single
  `effect(() => { void queueSizes.value; render(template(), root); })`, and
  starts/stops queue polling in the route's render/dispose lifecycle. When
  `quickplay` is set, `template()` returns early with a `.home-searching` view
  (no menu). The home component test counts `button`s **inside**
  `[data-testid="home-menu"]` (`tests/component/home.spec.ts:18`).
- **Leaderboard page** (`routes/leaderboard.ts`) fetches
  `request<Leaderboard>('/leaderboard?period=' + p)`, renders
  `rank · username (→ /u/:username) · rating` with all-time/this-month tabs,
  and uses an **epoch guard** (`loadEpoch`) so a slow response can't overwrite
  a newer one. It reads all signals into locals at the top of the effect to
  work around a documented happy-dom/lit-html nested-conditional re-render
  quirk (`routes/leaderboard.ts:43`).
- **Types** (`state/user-types.ts`): `LeaderboardPeriod = 'all-time' |
  'this-month'`; `Leaderboard = { period: string; entries: LeaderboardEntry[] }`;
  `LeaderboardEntry = { rank, username, rating, rd, games_played, score }`.
- **Server** (`crates/spades-server/src/handlers_leaderboard.rs`): `GET
  /leaderboard?period=<all-time|this-month|YYYY-MM>` returns **≤
  `LEADERBOARD_SIZE = 10`** entries (`leaderboard.rs:13`); there is **no
  `limit` query param**. Invalid `period` → 422.
- **Routes** `/u/:username` (profile) and `/leaderboard` are both registered
  (`main.ts:37-38`), so the widget's links are valid.
- **CSS** already defines `.leaderboard__tabs`, `.leaderboard__tab(.is-active)`,
  `.leaderboard__list`, `.leaderboard__row` (grid `2.5rem 1fr auto`),
  `.leaderboard__rank` (tabular-nums), `.leaderboard__rating` (tabular-nums).
  `.panel` gives a bordered, raised card. `.leaderboard` itself is
  `max-width:480px` — the widget will **not** use that class (it needs 360px to
  align with the menu).

## Architecture

A new component owns its own state and renders a lit-html fragment that
`home.ts` embeds. It follows the module-level-signal pattern `home.ts` already
uses for `quickplay`.

### New: `web/src/ui/components/leaderboard-preview.ts`

```ts
// module-level singletons (mirrors home.ts's quickplay pattern)
const period  = signal<LeaderboardPeriod>('all-time');
const board   = signal<Leaderboard | null>(null);
const loading = signal(false);
const error   = signal<string | null>(null);

let loadEpoch = 0; // epoch guard, same as routes/leaderboard.ts

const PREVIEW_SIZE = 5;
```

Exports:

- `leaderboardPreview(): TemplateResult` — pure render of current signal state.
  Reads all four signals into locals **before** building the template (the
  documented quirk-avoidance), then renders the section.
- `startLeaderboardPreview(): void` — kicks off the initial load
  (`void load(period.value)`).
- `stopLeaderboardPreview(): void` — `++loadEpoch` to invalidate any in-flight
  load so a response resolving *after* cleanup cannot write state or render
  into a torn-down root. (Keeps the "cleanup empties the root" home test green.)

Internal:

- `load(p)` — the epoch-guarded fetch copied from `routes/leaderboard.ts`
  (`request<Leaderboard>('/leaderboard?period=' + p)`, ignore stale epochs in
  the `.then`/`.catch`/`.finally`).
- `selectPreviewPeriod(p)` — tab handler: no-op if unchanged, else set `period`
  and `load(p)`.

### Edit: `web/src/routes/home.ts`

- Import `leaderboardPreview`, `startLeaderboardPreview`, `stopLeaderboardPreview`.
- In `render()`, call `startLeaderboardPreview()` next to `startQueuePoll()`.
- In `template()`, embed `${leaderboardPreview()}` **only in the menu branch**,
  as a sibling *after* the `.menu` div — not inside it. (The searching branch
  does not render it.)
- In the dispose closure, call `stopLeaderboardPreview()` next to
  `stopQueuePoll()`.

The existing home `effect` re-renders when the preview signals change, because
`leaderboardPreview()` reads them synchronously during `template()` — the same
mechanism by which the effect already tracks `queueSizes.value`. No second
effect is created.

### Edit: `web/src/ui/design.css`

Add a small `.home-leaderboard*` block (uses existing tokens); reuse the
existing `.leaderboard__*` rules unchanged.

## Data flow

1. `home.render()` → `startLeaderboardPreview()` → `load('all-time')`.
2. `load` sets `loading=true`, fetches, and on the current epoch sets
   `board` (or `error`), then `loading=false`.
3. The home effect (already subscribed via `leaderboardPreview()`'s reads)
   re-renders; the template slices `board.entries.slice(0, PREVIEW_SIZE)`.
4. Clicking a tab → `selectPreviewPeriod(p)` → `load(p)` with the new period;
   epoch guard discards an earlier in-flight response if it lands later.
5. `home`'s dispose → `stopLeaderboardPreview()` invalidates the epoch.

No polling or live updates (unlike the queue counts) — a single fetch per
period selection.

## Markup & CSS

`<section class="home-leaderboard panel" aria-labelledby="home-lb-title">`:

- **Header row**: `<h2 id="home-lb-title" class="home-leaderboard__title">Top
  players</h2>` + an anchor `View full leaderboard <icon arrow-right-s-line>`
  → `/leaderboard` (`data-link`).
- **Tabs**: reuse `.leaderboard__tabs` / `.leaderboard__tab` with
  `role="group"`, `aria-pressed`, `data-testid="home-tab-all-time"` /
  `"home-tab-this-month"`.
- **List**: reuse `.leaderboard__list` / `.leaderboard__row` /
  `.leaderboard__rank` / `.leaderboard__name` (→ `/u/${encodeURIComponent(
  username)}`, `data-link`) / `.leaderboard__rating`.
- Section gets `data-testid="home-leaderboard"`. All test-ids are `home-`
  prefixed to avoid colliding with the full page's.

New CSS (tokens only, no hardcoded colors):

```css
.home-leaderboard { width: 100%; max-width: 360px; margin-top: var(--space-4); }
.home-leaderboard__head {
  display: flex; align-items: baseline; justify-content: space-between;
  gap: var(--space-3); margin-bottom: var(--space-3);
}
.home-leaderboard__title { font-size: var(--text-lg); margin: 0; }
.home-leaderboard__more {
  font-size: var(--text-sm); color: var(--accent); white-space: nowrap;
  display: inline-flex; align-items: center; gap: 2px;
}
.home-leaderboard__status { color: var(--fg-muted); font-size: var(--text-sm); margin: var(--space-2) 0 0; }
```

## States & graceful degradation (the constraint)

The section sits *below* the CTAs, so it can never push them down — and it is
designed to never compete or alarm:

- **Loading:** a muted "Loading…" line, shown **only on first load**
  (`loading && !board`). Period switches and home re-entry keep the cached
  top-5 visible — no flicker.
- **Error:** a muted, **non-red** "Leaderboard unavailable." (deliberately
  downgraded from the full page's red `field-error`) — never a loud block near
  the join buttons. A failed fetch leaves the join menu fully intact.
- **Empty:** "No ranked players yet." (matches the full page's wording).

Render predicates:

```ts
const showLoading = loading && !board;            // first load only
const showError   = !!error;
const showEmpty    = !loading && !error && entries.length === 0;
const showList     = !error && entries.length > 0; // keep visible during refetch
```

## Accessibility

- Tabs: reuse the full page's `role="group"` + `aria-pressed` per tab.
- Section labelled via `aria-labelledby` → the `<h2>`.
- Rank/rating keep `font-variant-numeric: tabular-nums` (inherited from the
  reused classes).
- The "View full leaderboard" affordance is a real `<a data-link>`.

## Testing

New `web/tests/component/leaderboard-preview.spec.ts`, mirroring
`leaderboard.spec.ts` (stub `fetch` → `Response`, drive through
`home.render({}, {path:'/', search})`, flush with `setTimeout(…,0)` twice,
assert DOM, `cleanup()`):

- **Caps at 5**: API returns 10 entries → exactly 5 `.leaderboard__row` render.
- **Tab switch**: clicking `home-tab-this-month` refetches with
  `period=this-month` and marks that tab active.
- **Links**: a row's `.leaderboard__name` href is `/u/<username>`; the header
  link href is `/leaderboard`.
- **Error path**: a rejected/500 fetch shows the muted "unavailable" message
  **and** `[data-testid="home-menu"]` is still present (the core guarantee).
- **Empty**: `entries: []` → "No ranked players yet.".
- **Loading does not flicker** on re-render once a board is cached (optional).

Regression: existing `home.spec.ts` (5 buttons inside `home-menu`; cleanup
empties root) must stay green — guaranteed by keeping the widget a **sibling
section outside `.menu`** and invalidating in-flight loads on stop.

**Gate:** `pnpm -C web build && pnpm -C web test && pnpm -C web lint &&
pnpm -C web format:check` green.

## Out of scope (YAGNI)

- Polling / live updates (the board is fetched once per period selection).
- Avatars or extra columns (rd / games_played / score).
- A shared-data refactor of the full `/leaderboard` route.
- A server-side `limit` query param.
- Showing the section during the matchmaking "searching" state.
