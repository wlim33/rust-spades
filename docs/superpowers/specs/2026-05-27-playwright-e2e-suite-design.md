# Robust Playwright E2E Suite — Design

- **Date:** 2026-05-27
- **Status:** Design approved; implementation pending
- **Scope:** `web/tests/e2e/` — a reusable foundation (fixtures + helpers + page objects) plus three hardened flow specs (AI lifecycle, quickplay matchmaking, friends challenge). The existing smoke/nav tests are kept.
- **Approach chosen:** *Reusable foundation*, with **API-assisted setup** (auth + AI-game creation via the backend HTTP API). Multiplayer SSE flows (matchmaking, challenge) are **driven through the real UI** in browser contexts — the app's own `EventSource` owns the long-lived SSE lifecycle; no custom SSE client is built.

## Context

The current e2e suite (`web/tests/e2e/`) has solid *infrastructure* but thin, duplicative tests and no shared abstractions:

- **What's good and stays:** `playwright.config.ts` auto-starts both servers via `webServer` — `make -C .. backend DB=` (in-memory backend, gated on `/health`) and `pnpm dev` (Vite on :5173). `setup.ts` adds an auto-fixture `apiUp` that fails fast with a clear message if the backend is unreachable. `trace: on-first-retry`, `retries: 2` on CI, single Chromium project.
- **What's weak:** Each spec re-derives raw selectors (`.spades-bets button`, `.cm-clickable`, `.hand-container .card`, `.join-modal input`, `button.seat-open`) and re-implements the same pushState-routing wait (`waitForFunction(() => /\/play\/(?!new-ai)[^/]+$/.test(location.pathname))`). `quickplay.spec.ts` and `friends.spec.ts` both hand-roll 4-context orchestration. There are no auth/game-setup helpers, so every flow that needs state must click through the UI to build it. Coverage stops at "reached BETTING phase" — no test plays a hand to completion.

### Verified API + UI facts the design relies on

Routes (`crates/spades-server/src/bin/server/main.rs`) and frontend calls (`web/src/routes/*`, `web/src/state/session.ts`):

- **Auth is cookie-based** (`credentials: 'include'`): `POST /auth/register {email,password,username}` creates a user and sets a session cookie; `POST /auth/login {login,password}`, `POST /auth/logout`, `GET /auth/me`.
- **AI game is request/response:** `POST /games {max_points:500, num_humans:1}` creates a 1-human + 3-bot game, **auto-started server-side**, returning `{game_id, player_ids}` (human = `player_ids[0]`). This is exactly what `/play/new-ai` calls (`startAIGame`, `web/src/routes/play.ts`). Works under an **anonymous identity** — no login required.
- **Moves go through one endpoint:** `POST /games/{game_id}/transition`. **Bots auto-play server-side**, so a test only ever drives the human's moves.
- **Legality is reflected in the DOM:** playable cards carry the `.cm-clickable` class, so a test picks a legal play by clicking the first `.cm-clickable` — no game logic in the test.
- **Matchmaking + challenges are long-lived SSE streams** that must stay open for state to live: `POST /matchmaking/seek` (events `queue_status`, `game_start`), `POST /challenges` (event `challenge_created` → `{challenge_id, short_id, creator_player_id}`), `POST /challenges/{challenge_id}/join/{seat}` (events `joined`, `seat_update`, `game_start`), `DELETE /challenges/{challenge_id}`.
- **Challenges require all 4 seats filled by humans** (no bot fill). Flow: creator submits the `/create` form → lands in the lobby at `/play/{shortId}`; each of 4 players picks an open seat (`button.seat-open`), optionally names themselves in `.join-modal input`, clicks **Join**, and when all 4 seats fill the server emits `game_start` and everyone navigates into the game.
- **Home menu anchors** (`web/src/routes/home.ts`): container `[data-testid="home-menu"]` with exactly 5 buttons — quickplay `5+3` / `10+5` / `15+10`, `[data-testid="play-friends"]` ("Play with friends" → `/create`), `[data-testid="play-computers"]` ("Play with computers" → `/play/new-ai`). Quickplay waiting view shows `Finding players… (n/4)` + Cancel.

## Goal

Make the e2e suite **robust** in the sense the user prioritized: a reusable foundation that is both *stable* (deterministic waits, parallel-safe isolation) and *cheap to extend* (named page-object methods and fixtures instead of duplicated raw selectors), with the three highest-value user flows covered end-to-end on top of it.

## Non-goals (YAGNI)

- No custom Node SSE client — the app's browser `EventSource` drives matchmaking/challenge SSE.
- No CI workflow in this build (user flagged CI as not the priority). The suite stays runnable via `make e2e` / `pnpm test:e2e`; wiring GitHub Actions is a clean follow-up. (Note: a separate `2026-05-26-ci-suites-enforced-design.md` exists for that track.)
- Not covered as dedicated specs: OAuth, email verification, password reset, chat, replay, ratings/profile pages. The existing `auth.spec.ts` stays as-is; auth is otherwise exercised implicitly via the `authedPage` fixture.
- No second browser engine (Chromium only, as today).
- No exhaustive "play to 500 points" by default — the lifecycle test plays **one full hand** and asserts the transition into the next (see AI flow). A play-to-completion variant is optional and tagged `@slow`.

## Design

### Directory layout

```
web/tests/e2e/
  fixtures.ts            # extends Playwright `test`; supersedes setup.ts
  helpers/
    api.ts               # backend base URL + APIRequestContext wrapper
    auth.ts              # registerUser(api) -> { user, storageState }
    games.ts             # createAiGame(api) -> { gameId, playerId, shortId }
    identity.ts          # uniqueUser() -> { username, email, password }
    routing.ts           # waitForGameUrl(page), GAME_URL_RE
  pages/
    home-page.ts         # HomePage: quickplay(label), playWithComputers(), playWithFriends()
    create-page.ts       # CreatePage: create({ seat?, points?, timer? })
    lobby-page.ts        # LobbyPage: joinFirstOpenSeat(name?), waitForGameStart()
    game-page.ts         # GamePage: waitForBetting(), bet(n), playFirstLegalCard(),
                         #           handCount(), waitForPhasePlaying(), playOutHand()
  flows/
    ai-game.spec.ts
    matchmaking.spec.ts
    challenge.spec.ts
  smoke.spec.ts          # kept: home renders menu, quickplay->waiting->cancel, 404
```

`flows/` groups the new behavioral specs; `smoke.spec.ts` stays at the top level. `playwright.config.ts` `testDir: 'tests/e2e'` already globs both.

### Foundation — fixtures (`fixtures.ts`)

Replaces `setup.ts` (same `apiUp` auto-fixture, plus new fixtures). Exports a `test` extended with:

- **`apiUp`** (auto, unchanged): health-gates the backend before any test.
- **`api`**: a Playwright `APIRequestContext` bound to the backend base URL (`process.env.VITE_API_URL ?? 'http://localhost:3000'`). Used by helpers for register/create-game. Disposed automatically.
- **`authedPage`**: a `Page` whose context already carries a registered user's session cookie — so a test starts logged in with **no signup UI**. Built by `registerUser(api)` → capture the `set-cookie` into a `storageState` → `browser.newContext({ storageState })`. Each invocation uses a **unique identity** (`uniqueUser()` = `e2e_${shortUuid}` username/email), making tests parallel-safe under `fullyParallel`. The single-page fixture covers single-actor specs; **multi-context specs (matchmaking, challenge) reuse the same `registerUser` + `storageState` recipe directly**, building N independent contexts in a loop rather than via the fixture.

Existing specs that import `./setup` are migrated to import `./fixtures` (re-export `expect`). `setup.ts` is deleted.

### Foundation — helpers

- **`identity.uniqueUser()`** — returns `{ username, email, password }` with a uuid suffix. Single source of isolation; no two parallel tests collide.
- **`auth.registerUser(api)`** — `POST /auth/register` with a unique identity; returns `{ user, storageState }` (cookies extracted from the response). The cookie-injection path is what makes "logged-in" cheap.
- **`games.createAiGame(api)`** — `POST /games {max_points:500, num_humans:1}` then `GET /games/{game_id}` for `short_id`; returns `{ gameId, playerId, shortId }`. A test then `page.goto('/play/'+shortId)` and the existing localStorage-less boot path loads it. (For the anonymous case the same anon cookie must be on both the `api` context and the page context; the helper returns the cookie so the caller can seed the page, or the test simply clicks "Play with computers" for the entry-point variant.)
- **`routing.waitForGameUrl(page)`** — the one canonical `waitForFunction` for pushState navigation into `/play/{shortId}` (excluding `new-ai`), replacing the copy-pasted regex in every spec.

### Foundation — page objects

Each method bundles the element query **and** a deterministic wait, so specs read as intent and there are no bare timeouts.

- **`HomePage`**: `goto()`, `quickplay('5+3')`, `playWithComputers()`, `playWithFriends()`, `waitingPlayerCount()`, `cancelQuickplay()`.
- **`CreatePage`**: `create({ seat?, points?, timer? })` — fills the `/create` form and submits; resolves once redirected to the lobby; exposes `shortId()` from the URL.
- **`LobbyPage`**: `joinFirstOpenSeat(name?)` — clicks `button.seat-open`, fills `.join-modal input` if `name` given, clicks **Join**, waits for the modal to close; `waitForGameStart()` — waits for navigation into the game.
- **`GamePage`** (the workhorse): `waitForBetting()` (bet buttons present **or** non-empty `.spades-center-text`, matching today's robust check), `bet(n)`, `waitForPhasePlaying()` (`.cm-clickable` visible), `playFirstLegalCard()`, `handCount()` (`.hand-container .card` count), `readScores()`, and `playOutHand()` = loop {`playFirstLegalCard()` until 13 tricks resolved}. Exact score/next-hand selectors are confirmed against the running UI during implementation (TDD); the page-object interface above is the contract specs depend on.

### Flow specs

**`flows/ai-game.spec.ts`** (single context; the deep one)
1. *Entry-point fidelity (hardened existing test):* `HomePage.playWithComputers()` → `waitForGameUrl` → `GamePage.bet(3)` → `waitForPhasePlaying()` → reload → assert `handCount() === 13`. (Reconnect/resume.)
2. *Full lifecycle (new):* `createAiGame(api)` → seed page cookie → `goto('/play/'+shortId)` → `GamePage.bet(n)` → `playOutHand()` (13 legal plays, bots resolve each trick) → assert the hand is scored (`readScores()` changed) **and** the next hand's betting UI appears (`waitForBetting()` again). Proves the full game loop, bounded to one hand.
3. *(optional, `@slow`)* play hands until a side reaches 500 and a game-over view renders.

**`flows/matchmaking.spec.ts`** (4 contexts; hardens `quickplay.spec.ts`)
- Register 4 users via API; open 4 `authedPage`-style contexts. Each `HomePage.goto()` + `quickplay('5+3')` in `Promise.all`. Then `Promise.all` each `waitForGameUrl` and `GamePage.waitForBetting()`. Assertions are arrival-order independent. Teardown closes all contexts.

**`flows/challenge.spec.ts`** (4 contexts; hardens `friends.spec.ts`)
- Register 4 users via API. Creator: `HomePage.playWithFriends()` → `CreatePage.create({})` (no pre-picked seat → 4 open seats) → capture `shortId`. All 4 players sequentially: navigate to `/play/{shortId}` → `LobbyPage.joinFirstOpenSeat(name)` (sequential so two players never grab the same seat) → then `Promise.all` `LobbyPage.waitForGameStart()` + `GamePage.waitForBetting()`. `afterEach`: `DELETE /challenges/{challengeId}` so a lingering open challenge can't perturb later tests; close all contexts.

### Reliability practices (the "robust" part)

- **Isolation:** every test mints unique identities; the e2e backend is in-memory (`DB=`), so no persisted cross-run state.
- **Deterministic waits only:** web-first assertions (`expect(...).toBeVisible()/toHaveCount()` with scoped timeouts) and `waitForFunction` for pushState; **no `waitForTimeout`/sleep**. All waits live inside page-object methods.
- **Multi-context discipline:** N contexts driven via `Promise.all`; seat selection in the challenge flow is sequential to avoid seat races; all assertions order-independent.
- **Teardown:** contexts always closed in `finally`/`afterEach`; challenges explicitly `DELETE`d. Keep `trace: on-first-retry` and CI `retries: 2`.

## Testing this work

The suite *is* the test. Verification = `make e2e` (which auto-starts the in-memory backend + Vite) runs green locally, and each new spec fails meaningfully when its target behavior is broken (verified by transient sabotage during TDD — e.g. a wrong bet value should fail `playOutHand`). Foundation helpers/page objects get exercised by the flow specs rather than having standalone unit tests.

## Open questions / confirm-at-implementation

- Exact selectors for **scoreboard** and **game-over/next-hand** views (for `GamePage.readScores()` and lifecycle assertion 2) — read off the running UI during TDD.
- Whether `POST /auth/register` returns the session cookie directly or requires a follow-up `login` to obtain it — confirm the `set-cookie` on register; fall back to register-then-login in `registerUser` if needed.
- Anonymous-identity cookie sharing for `createAiGame` seeding (api context cookie ↔ page context) vs. simply driving the "Play with computers" button for setup — pick whichever proves stable; both are acceptable.
