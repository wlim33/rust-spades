# spades-ts — design

**Status:** Draft for review
**Date:** 2026-05-11
**Replaces:** `personal-site/spades/` (vanilla HTML + Alpine.js client)
**Server:** `rust-spades` (axum) — out of scope for this spec; the user will patch oasgen route coverage separately.

---

## 1. Goals & non-goals

### Goals

1. Replace `personal-site/spades` with a standalone TypeScript SPA at `/Users/wlim/Projects/spades-ts`, served same-origin as the rust-spades API.
2. Preserve gameplay flows that work today: Quick Play, Play with Computers, Play with Friends (challenge + lobby), betting, trick play, URL-based reconnect, drag-to-play, trick animations.
3. Add account-aware UX: email+password auth, Google + GitHub OAuth, own-settings page, public profile pages, player game history.
4. Anonymous play remains first-class — no auth gate to start a game.
5. Clean small design system (CSS variables, minimal CSS). Mobile-friendly.
6. Testing baseline: unit (Vitest), component (testing-library/dom + happy-dom), E2E (Playwright).

### Non-goals

- Email verification flow and password reset flow — server supports them; deliberately deferred.
- Re-implementing game logic on the client — server is authoritative.
- Server changes — out of scope; the user is patching rust-spades separately to expand oasgen coverage of `/openapi.json`.
- Replays, spectator mode, chat — not requested.

### Decisions log

| Topic | Decision |
|---|---|
| Stack | Vite + Vanilla TypeScript |
| Reactivity | `@preact/signals-core` |
| Templating | `lit-html` (template package only) |
| Routing | `navaid` (small router lib) |
| Styling | Clean design tokens via CSS variables |
| API types | Generated from `/openapi.json` via `openapi-typescript`; typed client via `openapi-fetch` |
| API config | `VITE_API_URL` env var; dev = `http://localhost:3000`, prod = `https://spades.wlim.dev` |
| Repo | `/Users/wlim/Projects/spades-ts` (standalone) |
| Deploy | Same origin as API (`spades.wlim.dev/`) |
| Card layer | Port proven design from `card-manager.js`, rewrite in TS as separate modules |
| Anon play | First-class; accounts are optional |
| Testing | Unit + component + full E2E |

---

## 2. Architecture

Single-page app, all client-side, served as static files from same-origin as the API.

### Source tree

```
spades-ts/
├── index.html                       Vite entry; mounts <main id="root">
├── vite.config.ts
├── package.json, tsconfig.json, .eslintrc, .prettierrc
├── .env.development                 VITE_API_URL=http://localhost:3000
├── .env.production                  VITE_API_URL=https://spades.wlim.dev
├── openapi/
│   ├── openapi.json                 Committed snapshot used by CI for drift check
│   ├── fetch.sh                     curl $VITE_API_URL/openapi.json → openapi.json
│   └── generate.sh                  openapi-typescript → src/api/schema.d.ts
├── src/
│   ├── main.ts                      Bootstraps router; mounts root template
│   ├── router.ts                    navaid wrapper; route → render(params) → cleanup
│   ├── api/
│   │   ├── schema.d.ts              Generated; committed
│   │   ├── client.ts                openapi-fetch instance; credentials:'include'; ApiError
│   │   ├── sse.ts                   openSse(url, body, { onEvent, signal }) → { close }
│   │   ├── ws.ts                    openGameWs(gameId, playerId, { onEvent }) → { close }
│   │   └── hand-written.ts          Types for routes not yet covered by oasgen (should be empty after the user's patch)
│   ├── state/
│   │   ├── session.ts               currentUser: Signal<User | null>; login/logout/refresh
│   │   ├── menu.ts                  queue sizes, lobby seats
│   │   ├── game.ts                  createGameStore(); applyState/applyWsEvent/applyPresence
│   │   └── helpers.ts               Pure: sortCards, isCardValid, getLeadSuit, seatRel, formatClock, oppCardCount, cardEq
│   ├── ui/
│   │   ├── design.css               CSS variables (color/space/radius/type), reset, base
│   │   ├── components/              Header, Button, Modal, FormField, Toast (lit-html templates)
│   │   └── templates.ts             Small html`` helpers shared across routes
│   ├── cards/
│   │   ├── card-el.ts               createCardFront/Back, setFront, suit symbols/colors
│   │   ├── hand-manager.ts          South/north/east/west hands; mount/replace/unmount
│   │   ├── trick-manager.ts         4 slots, fillNextSlot, slotsRect(), clear()
│   │   ├── drag.ts                  attachDrag(el, { onPlay, threshold }) → cleanup
│   │   ├── animation.ts             animateTo (rAF), easings (linear/quartIn/quartOut)
│   │   └── orchestrator.ts          Glue layer matching the old CardManager public surface
│   ├── routes/
│   │   ├── home.ts                  /                  Menu
│   │   ├── play.ts                  /play/:shortId     Lobby + in-game + game-over
│   │   ├── login.ts                 /login
│   │   ├── signup.ts                /signup
│   │   ├── oauth-complete.ts        /auth/oauth/complete (callback landing)
│   │   ├── settings.ts              /me          (auth gate)
│   │   ├── profile.ts               /u/:username
│   │   └── notfound.ts              *
│   └── lib/
│       ├── storage.ts               localStorage helpers (saveGameSession etc.)
│       └── util.ts                  navigateTo, debounce, etc.
└── tests/
    ├── unit/                        Vitest
    ├── component/                   Vitest + @testing-library/dom + happy-dom
    └── e2e/                         Playwright
```

### Routes

| Path | Module | Auth |
|---|---|---|
| `/` | `routes/home.ts` | no |
| `/play/:shortId` | `routes/play.ts` | no |
| `/login` | `routes/login.ts` | no |
| `/signup` | `routes/signup.ts` | no |
| `/auth/oauth/complete` | `routes/oauth-complete.ts` | no |
| `/me` | `routes/settings.ts` | **yes** (→ `/login?next=/me`) |
| `/u/:username` | `routes/profile.ts` | no |
| `*` | `routes/notfound.ts` | no |

### Data flow

```
server (axum)  ◀──┬── REST (openapi-fetch, credentials:'include')
                  ├── SSE (POST + ReadableStream) — matchmaking & challenges
                  └── WS  /games/:id/ws?player_id=… — game updates
       │
       ▼
  src/api/*    ── typed responses ──►  src/state/* (signals)
                                              │  effect()
                                              ▼
                            src/routes/* and src/cards/* render via lit-html
                                              │
                                              ▼
                                         DOM (#root)
```

---

## 3. Module contracts

Each module is described as: **what it does**, **how you use it**, **what it depends on**.

### `src/api/client.ts`

Configured `openapi-fetch` instance plus a thin wrapper that normalizes errors to `ApiError { status, message }`. Always sends `credentials: 'include'`. Reads `VITE_API_URL`. Consumers call e.g. `api.GET('/games/{game_id}', { params: { path: { game_id } } })`. Depends on: `schema.d.ts`, `hand-written.ts`.

### `src/api/sse.ts`

`openSse(url, body, { onEvent, signal })` returns `{ close() }`. Parses `event:` / `data:` frames as the current Alpine code does, using a streaming `TextDecoder` and split-buffer. Uses `AbortController`; `close()` is idempotent and suppresses the resulting AbortError. Consumed by matchmaking seek, create-challenge, join-challenge.

### `src/api/ws.ts`

`openGameWs(gameId, playerId, { onEvent }) → { close }`. No auto-reconnect (the caller falls back to polling on close). Wraps `onmessage` to JSON-parse and dispatch. Internally maintains a small async queue so handlers are serialized — preserves today's `_wsQueue` ordering invariant (especially for trick-completion + opponent-play animations).

### `src/state/session.ts`

Exposes:
- `currentUser: Signal<User | null>`
- `refresh()` — calls `GET /auth/me`; sets to user on 200, null on 401, surfaces non-401 errors via toast.
- `loginWithPassword(email, password)`
- `signup(email, password, displayName)`
- `logout()`
- `startOauth(provider: 'google' | 'github')` — sets `next` cookie/localStorage, navigates to `/auth/oauth/{provider}/login`.

Drives the header chrome (sign-in button vs. avatar menu) and route guards.

### `src/state/game.ts`

Factory `createGameStore()` per active game. Signals:
- `phase: Signal<'MENU'|'CREATE'|'WAITING'|'LOBBY'|'BETTING'|'PLAYING'|'GAME_OVER'>`
- `gameState`, `playerIds`, `playerNames`, `playerConnected`
- `hand`, `tableCards`, `playerBets`, `playerTricksWon`, `lastTrickWinnerId`
- `teamAScore`, `teamBScore`, `teamABags`, `teamBBags`
- `timerConfig`, `playerClocksMs`, `activePlayerClockMs`
- `currentPlayerId`, `spadesBroken`

Methods:
- `applyState(state, hand)`
- `applyPresence(players)`
- `applyWsEvent(data)` — orchestrates trick-completion detection and triggers opponent-play animations via the orchestrator handle.
- `pollOnce()` — single REST poll-and-apply.
- `dispose()`

No DOM ownership — the store does not touch `document`. An orchestrator handle is passed in at construction so `applyWsEvent` can `await` animation primitives (`collectTrick`, `playOpponentCardToCenter`, `_placeMissingTrickCards`) in the same order today's `_drainWsQueue` does. Cheaper UI updates (re-rendering the hand, toggling clickable state) are driven by a separate effect in `routes/play.ts` that reads the same signals.

### `src/state/helpers.ts`

Pure deterministic functions. **100% unit-test coverage target.**

- `sortCards(cards)`
- `cardEq(a, b)`
- `isCardValid(hand, leadSuit, spadesBroken, card, isMyTurn, phase) → boolean`
- `getLeadSuit(tableCards, myIdx) → Suit | null`
- `oppCardCount(phase, gameState, tableCards, seatIdx) → number`
- `seatRel(absIdx, myIdx) → 'south' | 'east' | 'north' | 'west'`
- `formatClock(ms) → string`

### `src/cards/orchestrator.ts`

Successor to today's `CardManager` god-class. Public surface preserved so `routes/play.ts` is a thin glue layer.

- `init(containers)` — registers DOM nodes for hand / north / west / east / trick.
- `setupImmediate(playerHand, oppCounts, tableCards, myIdx, northIdx, westIdx, eastIdx, currentPlayerSeatIdx)` — initial placement on game enter / reconnect.
- `updatePlayerHand(cards)`
- `updateOpponentHand(seat, count)`
- `playCardToCenter(card)` — fly-to-center for local player.
- `playOpponentCardToCenter(card, seat)`
- `collectTrick(winnerSeat)` — phased animation: pause → stack → slide toward winner + fade.
- `clearAll()`, `clearTrick()`, `placeCardInTrick(card, seat)`
- `enableInteraction(validCards, onPlay)`, `disableInteraction()`
- `destroy()`

Internally composes `HandManager`, `TrickManager`, `DragController`, `animateTo`.

### `src/cards/hand-manager.ts`

Owns south/north/east/west hand DOM subtrees. API: `mount(seat, cards|count)`, `replace(seat, cards)`, `unmount(seat)`, `el(seat, card)`. Pure layout (flex children).

### `src/cards/trick-manager.ts`

Owns the 4-slot layout. API: `init()`, `fillNextSlot(card, seat)`, `slots()`, `clear()`, `count()`. Neither manager knows about animations beyond setting `transform` / `visibility`.

### `src/cards/drag.ts`

`attachDrag(el, { onPlay, threshold }) → cleanup`. Same `pointer*` event handlers as today. Emits `onPlay(srcRect)` so the orchestrator knows the visual start position for the fly animation.

### `src/cards/animation.ts`

`animateTo(el, opts) → Promise<void>` — rAF-based tween. Easings: `linear`, `quartIn`, `quartOut`. No DOM ownership; pure animator.

### `src/router.ts`

`navaid` instance. Each route module exports `render(params, ctx) → cleanup`. The router calls the previous route's `cleanup` before mounting the next. `navigate(path)` plus a delegated `<a data-link>` handler that pushes state and re-renders.

### `src/routes/play.ts`

The biggest route. Reconnect/boot priority chain (replicates today's `doInit`):

1. localStorage session for this short id → `GET /games/{gid}` + hand + presence → resume.
2. `GET /games/by-player-url/:id` → resume.
3. `GET /challenges/by-short-id/:id` → render lobby state (or surface "started elsewhere" / "no longer available").
4. Fallback: navigate to `/`, show toast.

Creates a single `gameStore` and a single `orchestrator`, injecting the orchestrator into the store so `applyWsEvent` can sequence animations with state updates. Mounts the in-game template. Subscribes signals to lit-html re-renders via one top-level effect. A second, cheaper effect watches `phase`, `hand`, `tableCards`, `currentPlayerId` and calls `orchestrator.renderCards()` / `updateInteraction()` after the next microtask. Opens WS. On WS close, if phase ∉ {GAME_OVER}, starts polling fallback at 2s. Returns a `cleanup()` that closes WS/SSE, calls `orchestrator.destroy()`, and disposes signal effects.

Lobby UI is a sub-render inside `play.ts` (not a separate route) for cohesion with reconnect logic.

### `src/routes/{home,login,signup,settings,profile,oauth-complete,notfound}.ts`

Each exports `render(params) → cleanup`. Use small `FormField`/`Button` lit-html components. Profile fetches `/users/:username` and `/users/:username/games`. `oauth-complete` finalizes session and navigates to the saved `next`.

---

## 4. Reactivity, error handling, lifecycle

### Signals → DOM

Each route's `render(params)` returns a `cleanup` closure:

```ts
const dispose = effect(() => litRender(template(state), root))
return () => {
  dispose()
  orchestrator?.destroy()
  ws?.close()
  sse?.close()
}
```

One top-level effect per route. Inside the effect, `signal.value` reads are auto-tracked.

The card layer is **not** reactive — it's a side-effect target: a separate effect watches `phase`, `hand`, `tableCards`, `currentPlayerId` and calls `orchestrator.renderCards()` / `orchestrator.updateInteraction()` after the next microtask (replicating today's `$watch + $nextTick` pattern).

### Route guards

`/me` requires auth. On mount, if `currentUser.value === null` and `GET /auth/me` returns 401, navigate to `/login?next=/me`. The login route reads `next` and navigates there after success.

### SSE / WS lifecycle

- SSE handlers (matchmaking, challenges) are owned by the route that started them. Route cleanup aborts the `AbortController` and sets a `closedIntentionally` flag so the read loop suppresses an error toast.
- WS is owned by `routes/play.ts`. On `onclose`, if phase ≠ GAME_OVER, switch to polling (`setInterval(pollGame, 2000)`).
- Polling and WS share the same `applyState`/`applyWsEvent` paths — only the source differs.
- Drop-guard pattern from current code is preserved: leaving lobby/seek closes SSE so the server frees the seat / removes the seeker.

### Error handling — three tiers

1. **Network / 4xx-5xx** — `ApiError { status, message }` thrown by `client.ts`. Routes catch and set a `toast` signal; auth flows surface inline form errors.
2. **Server contract violations** (unexpected shape) — log to console, set a generic toast "Something went wrong", do not crash the app.
3. **Programmer errors** — let them throw; Vite overlay surfaces them in dev. Prod has `window.onerror` → toast + hidden details.

A small `Toast` component lives in the persistent app shell, reads a single `toast: Signal<{kind, msg} | null>`, auto-dismisses after 4s.

### Animation correctness invariants (carried from current code)

- Card-in-flight: only one fly-to-center may run at a time; a `playingCard` flag gates input.
- Trick collect is mutually exclusive with new opponent plays — guard via the trick-cards-length ≥ 4 check before queueing new plays.
- AI fast-play skip case: detect "old trick had 4, new table has different cards" and force-place the missed trick before clearing. (The `_placeMissingTrickCards` + `_detectTrickCompletion` pair.)

### Local persistence

- `spades_game_<shortId>` → `{ gid, pid }` for reconnect (unchanged).
- `spades_last_name` — optional, prefills name inputs.
- `spades_oauth_next` — short-lived; cleared after OAuth callback finalizes.
- No localStorage of auth — session lives in the HttpOnly cookie set by the server.

---

## 5. Testing strategy

### Unit (Vitest, `tests/unit/`)

Pure modules only. Target ~100% coverage on these:

- `state/helpers.ts`: `sortCards`, `isCardValid` (all branches: lead suit / spades broken / hand-only-spades / leading off-suit), `getLeadSuit`, `oppCardCount`, `seatRel`, `formatClock`.
- `state/game.ts`: `applyState`, `applyWsEvent` against recorded fixtures (real server payloads: `Betting`, mid-trick, trick complete, AI fast-play skip, `Completed`). Pure transitions, no DOM.
- `cards/animation.ts`: easings; the rAF loop is mocked with fake timers.
- `api/sse.ts`: parser against a hand-crafted chunked stream (split frames, multi-frame chunks, empty lines).

### Component (Vitest + `@testing-library/dom` + happy-dom, `tests/component/`)

DOM-affecting templates, no real network:

- `ui/components/{FormField,Button,Modal,Toast}`.
- `routes/login`, `routes/signup`: mounted with a mocked `api`; asserts validation, error display, redirect-on-success.
- `routes/settings`: rename flow against mocked `PATCH /users/me`.
- `routes/profile`: renders shape from a fixture; loading and not-found states.

Card animations are **not** component-tested (they need real layout / rAF). Pure helpers (`cardEq`, etc.) are unit-tested. Animation correctness is validated in E2E and manually.

### E2E (Playwright, `tests/e2e/`)

Each test starts `rust-spades` against an ephemeral SQLite DB on a random port; the frontend dev server runs with `VITE_API_URL` pointing at it. Helper opens 4 browser contexts when the test needs 4 players. Five tests:

1. **Anonymous AI happy path** — `/` → "Play with Computers" → bet → play a few tricks → assert score updates. Then reload mid-game and assert hand still rendered (reconnect smoke).
2. **Quickplay 4-player match** — 4 contexts, all seek 500-pt; assert all reach BETTING.
3. **Friends challenge** — context 1 creates challenge, copies share link; 2/3/4 navigate and join; assert all reach BETTING.
4. **Auth happy path** — signup → logged in (`/me` shows email) → logout → login → `/me` again. Email+password only; OAuth tested manually.
5. **Profile + history** — completed game set up via API helper, visit `/u/:username`, assert it lists the game.

### Deliberately out

- Visual-regression snapshots (too brittle while the design system is in flux).
- OAuth E2E (provider flakiness in CI).
- Email-verification and password-reset tests (features deferred).

### Tooling

- `tsc --noEmit` in CI.
- ESLint + Prettier.
- `pnpm run gen:api` runs `openapi-typescript` against the configured `VITE_API_URL`. Run manually after a server schema bump. Generated file is committed.

---

## 6. Build & deploy

### Vite

- `base: '/'`, `build.outDir: 'dist'`, `build.sourcemap: true`.
- Dev server on `:5173`. `define`'d env: `VITE_API_URL`, `VITE_BUILD_VERSION` (from `git rev-parse --short HEAD`).
- `index.html` declares `<main id="root">` and `<script type="module" src="/src/main.ts">`.

### Same-origin in prod

Build output (`dist/`) must be served by *something* on `spades.wlim.dev`. Two viable paths (the user is patching rust-spades anyway):

- **(a) default in this spec** — rust-spades serves `dist/` as static via `tower-http`'s `ServeDir`, with SPA fallback to `index.html` for unknown paths that aren't API routes. Single binary, single deploy.
- **(b) alternative** — Caddy/nginx in front; static for `/`, proxy API paths to rust-spades.

The frontend stays the same either way.

### Deploy artifact

- `pnpm run build` → `dist/` (hashed assets + `index.html`).
- `deploy.sh` tars `dist/` and ships it to wherever rust-spades expects (default `./public`).

### CI (single GitHub Actions workflow)

1. `pnpm install --frozen-lockfile`
2. `pnpm run gen:api:check` — regen against `openapi/openapi.json` snapshot; fail if diff (catches forgotten regens after server schema bumps)
3. `pnpm tsc --noEmit`
4. `pnpm lint`
5. `pnpm test:unit`
6. `pnpm test:component`
7. `pnpm test:e2e` — builds rust-spades from a pinned git sha (or pulls a binary artifact), spins it up against tmp sqlite, runs Playwright.

E2E is the slow gate; everything else should be <30s.

### Local dev workflow

```
# Terminal 1
cd ~/Projects/rust-spades
cargo run -p spades-server -- --port 3000 --insecure-cookies \
  --cors-allow-origin http://localhost:5173

# Terminal 2
cd ~/Projects/spades-ts
pnpm dev
# → http://localhost:5173
```

Cross-origin in dev; same-origin in prod. `--insecure-cookies` is dev-only.

---

## 7. Migration plan

### Phase 0 — Prereqs (handled by user, outside this spec)

- Patch rust-spades so `/openapi.json` covers all routes the frontend uses.
- Decide on the same-origin static-serving story — option (a) or (b) above.

### Phase 1 — Scaffold (no behavior change yet)

- Init Vite + TS + ESLint + Prettier + Vitest + Playwright in `/Users/wlim/Projects/spades-ts`.
- Generate `schema.d.ts`. Hand-written stubs for any uncovered routes (~zero after Phase 0).
- Empty shell: design tokens, router, header, home route with menu buttons wired to console.logs.

### Phase 2 — Gameplay parity

- Port `state/helpers.ts` + tests (pure, fastest win).
- Port the card layer (`cards/*`). Verify standalone via a dev-only `/_test/cards` route.
- Port `state/game.ts` with WS/poll integration.
- Build `routes/play.ts` end-to-end: AI game, quickplay, friends challenge, lobby. Anonymous play works.
- **Could ship as a beta replacement at this point.**

### Phase 3 — Account-aware UX

- `state/session.ts` + header chrome (sign in / avatar menu).
- `routes/{login,signup,oauth-complete}.ts`.
- `routes/settings.ts` (rename, sign out).
- `routes/profile.ts` (own + others, with history list).
- When signed in: in-game display name comes from the authenticated user; for anonymous play, the existing `PUT /games/:gid/players/:pid/name` path still works.

### Phase 4 — Polish & cutover

- Empty states, loading skeletons, mobile pass.
- E2E suite green in CI.
- Deploy to `spades.wlim.dev`; redirect `wlim.dev/spades/*` to `spades.wlim.dev/*` via `_redirects` in `personal-site`.
- Keep old static files for a brief grace period, then delete from `personal-site/spades/`.

### Phase 5 — Follow-ups (out of this spec, recorded for later)

- Email verification + password reset flows.
- Backfill oasgen coverage for any remaining routes.
- Replays / chat / spectator mode if desired.

---

## Open questions

None at sign-off. If the rust-spades static-serving story (option a vs b) shifts during Phase 0, only Section 6 needs revisiting — the frontend code is unaffected.
