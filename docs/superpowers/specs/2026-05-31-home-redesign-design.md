# Frontend Redesign — Phase 2: Home & Menus (Design Spec)

**Date:** 2026-05-31
**Status:** Approved (visual direction validated via `web/home-preview.html`, light + dark — **minus the landing hero**, which the owner cut)
**Phase:** 2 of the redesign. Builds on Phase 0 (Foundations) and Phase 1 (Table), both merged to master. Remaining after this: Auth/account.

---

## 1. Context & goals

The home route (`web/src/routes/home.ts`, lit-html) renders **hand-rolled inline SVGs** (`TIER_ICONS`, `ROW_ICONS`) instead of the Phase-0 vendored Remix icon pipeline, its quick-match tiles and menu rows predate the design-system polish, and the matchmaking "waiting" state is a bare line of text + Cancel.

This phase brings the menu-first home onto the shared design system and the icon pipeline, and refreshes the matchmaking state — **without a landing hero** (the home stays menu-first) and **without changing matchmaking logic**.

### Goals
- **Quick-match tiles** (Blitz/Rapid/Classic) restyled on Phase-0 tokens, with the inline tier SVGs replaced by the vendored **Remix `icon()`** helper; live queue counts preserved (accent when >0).
- **"Other ways to play" rows** (friends / computers) restyled, inline SVGs → Remix icons, with a chevron affordance.
- A calmer **matchmaking "searching" state**: animated pulsing dots + "Finding players…" + "N of 4 seated · `<tier>`" + Cancel.
- Light/dark + responsive (tiles stack on mobile); reduced-motion respected.

### Non-goals (out of scope)
- **No landing hero** — no wordmark/tagline/eyebrow or decorative card fan. The home opens directly on the menu (as today), just polished.
- **`create` / `lobby` / share-link flows** — deferred to the Auth/account phase (form-heavy).
- Game-view/table (Phase 1, done), auth, profile, settings.
- Matchmaking/queue/SSE **logic** — `onSeek`, `openSse`, `queueSizes`/`queueCountFor` polling, the `quickplay` signal, and the oauth banner behavior are unchanged; only their presentation changes.

---

## 2. Constraints & guardrails
- Stay on lit-html + `@preact/signals-core` + Vite, CSS-only, **no new runtime deps**. Icons are **already vendored** (Phase 0) — no new assets expected (see §4.2).
- Preserve a11y: keep the existing `data-testid`s (`home-menu`, `play-friends`, `play-computers`) for tests; quick-match/row controls remain real `<button>`s with accessible names; `:focus-visible` honored; the searching-dots animation gated by `prefers-reduced-motion`.
- Follow existing `home.ts` structure (it drives a `render(template(), root)` effect on `queueSizes`).

---

## 3. Locked decisions (validated in the preview, hero removed)

| Area | Decision |
| --- | --- |
| Hero | **None** — menu-first home, no wordmark/tagline/card-fan. |
| Icons | Replace `TIER_ICONS`/`ROW_ICONS` inline SVGs with the vendored Remix `icon()` helper: `flashlight-fill` (Blitz), `timer-flash-fill` (Rapid), `hourglass-fill` (Classic), `group-fill` (friends), `robot-2-fill` (computers), `arrow-right-s-line` (row chevron). |
| Tiles | Restyled quick-match tiles: tier-accent top border, big Fraunces time, mono tier label, queue count (accent when >0). |
| Rows | Restyled menu rows: tinted icon chip + title + meta + chevron; left accent border. |
| Searching | Pulsing-dots state, "Finding players…", "N of 4 seated · `<tier>`", Cancel. |
| create/lobby | **Deferred** to the Auth/account phase. |

---

## 4. Architecture

`home.ts` stays a single lit-html route module. Changes are template + CSS, plus an icon-helper swap. The overall layout is unchanged (optional `oauth` banner → "Quick play" tiles → "Other ways to play" rows); no hero is inserted.

### 4.1 Tiles + rows (icon swap + restyle)
- Replace the `TIER_ICONS`/`ROW_ICONS` `Record<string, TemplateResult>` inline-SVG maps and their usages with calls to `icon(name)` from `web/src/ui/icon.ts` (Phase 0): tile icons by tier key; row icons for friends/computers; a chevron `icon('arrow-right-s-line')` on each row.
- Keep the `QUICKPLAY_TIMERS` data, `queueCountFor`, the `onSeek`/`onFriends`/`onComputers` handlers, and the section labels. Keep `data-testid="play-friends"`/`"play-computers"` on the rows and `data-testid="home-menu"` on the container.
- Markup may keep the current `.menu`/`.menu__quickplay`/`.menu__row`/`.quickplay-*` class names and restyle them, or move to refreshed names — whichever is cleaner — but preserve the `data-testid`s, and **remove any `.menu__*`/`.quickplay-*` rules left unused** after the refresh (no dead CSS).

### 4.2 Icons — confirm already vendored
All six names were vendored in Phase 0 (Task 4): `flashlight-fill`, `timer-flash-fill`, `hourglass-fill`, `group-fill`, `robot-2-fill`, `arrow-right-s-line` are all in `web/src/ui/icons/`. The implementation **verifies** they exist (`ls web/src/ui/icons/`); if any is missing, vendor it via the same Apache-2.0 path documented in the Phase-0 plan. No new icons are anticipated.

### 4.3 Searching state
- Replace the `quickplay.value` branch (`.quickplay-wait` with `Finding players… (n/4)` + Cancel `button()`) with a `.home-searching` block: animated dots, "Finding players…", a sub-line "`<waiting>` of 4 seated · `<tier>`", and the Cancel button (still calls `q.cancel`). The `quickplay` signal carries `waiting`; thread the chosen tier label through `QuickplayState` (add a `tier` field set in `onSeek`) so the sub-line can name the time control — a presentational addition, not a logic change. The dots animation is wrapped in `@media (prefers-reduced-motion: no-preference)`.

### 4.4 CSS
- Add `home`-scoped rules to `design.css`: refreshed tile + row rules and `.home-searching` (+ its dots `@keyframes`, reduced-motion-gated). Reuse Phase-0 tokens throughout (no hardcoded colors). Responsive: tiles → single column under the existing mobile breakpoint. Delete `.menu__*`/`.quickplay-*` rules that the refreshed markup no longer uses.

---

## 5. Data flow
- `startQueuePoll`/`stopQueuePoll` + `queueSizes` signal drive per-tile counts via `queueCountFor` (unchanged).
- `onSeek(timer)` opens the matchmaking SSE and sets `quickplay.value = { waiting, cancel, tier }` (add `tier`); `game_start` → `saveSession` + `navigateTo` (unchanged).
- The render effect re-runs on `queueSizes.value` (unchanged).

## 6. Testing & verification
- **Component (`home.spec.ts`)**: update for the refreshed structure but keep asserting the preserved `data-testid`s (`home-menu`, `play-friends`, `play-computers`) render when not searching, and the Cancel/searching path when `quickplay` is set. Add an assertion that a tile renders the `icon()` output (`.icon svg`) rather than the old inline `<svg>`.
- **Gate:** `pnpm -C web build && pnpm -C web test && pnpm -C web lint && pnpm -C web format:check` green.
- **e2e:** `smoke.spec.ts` uses `getByText('Finding players')` — keep that exact phrase in the searching state. Check `web/tests/e2e` for any home selectors the restructure would break and keep them on the preserved `data-testid`s.
- **Visual:** `web/home-preview.html` is the reference for the menu/tiles/rows/searching look (ignore its hero, which is cut); verify light + dark and mobile width.

## 7. Risks & open questions
- **Icon-name availability:** all six are expected in the vendored set — verified at implementation; trivial to vendor one more if not.
- **`data-testid` / e2e selectors:** the restyle must preserve the test-ids and the "Finding players" phrase, or update tests in lockstep.
- **Dead CSS:** the old `.menu__*` / `.quickplay-*` rules must be removed if the markup stops using them (no orphans).

## 8. Deliverables
Tiles/rows swapped to the Remix `icon()` helper + restyled; refreshed searching state (with `tier` threaded through `quickplay`); `design.css` home rules (+ removal of now-dead menu/quickplay rules); updated `home.spec.ts`; gate green; light/dark + responsive verified. No landing hero.
