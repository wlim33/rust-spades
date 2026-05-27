# Robust Playwright E2E Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reusable Playwright e2e foundation (fixtures + helpers + page objects) and rewrite the three core flow specs (AI lifecycle, quickplay matchmaking, friends challenge) on top of it, with deterministic waits and parallel-safe isolation.

**Architecture:** API-assisted setup using Playwright's **context-bound** `APIRequestContext` (`page.request` / `context.request`) so auth/anon cookies are automatically shared with the browser context. Auth and AI-game creation go through the backend HTTP API; the SSE-driven matchmaking and challenge flows are driven through the real UI (the app's own `EventSource` owns the long-lived stream). Per-test unique identities give parallel isolation. Page objects bundle each element query with a deterministic wait.

**Tech Stack:** TypeScript, `@playwright/test` ^1.48, Vite dev server (:5173, proxies `/auth` `/games` `/matchmaking` `/challenges` to backend :3000), Rust/axum backend (cookie sessions via `tower_sessions`).

---

## Background the engineer needs

- **The app is same-origin in dev.** `API_URL` (`web/src/lib/util.ts`) is `''`, so the browser calls `/auth/...`, `/games`, etc. on :5173 and Vite proxies to :3000. `playwright.config.ts` already auto-starts both servers (`make -C .. backend DB=` for an in-memory backend on :3000, and `pnpm dev` on :5173) and sets `baseURL: http://localhost:5173`.
- **Cookie sharing rule (critical):** only `page.request` and `context.request` share the cookie jar with their browser context. The standalone `request` test fixture does **not**. All API setup in this plan uses `page.request` (single-context tests) or `context.request` (multi-context tests).
- **AI game ownership:** `POST /games {max_points:500, num_humans:1}` creates a 1-human + 3-bot game (auto-started), returns `{game_id, player_ids}` with the human at `player_ids[0]`. The human seat is owned by the **anonymous identity that made the request**; the bot seats auto-play server-side. Moves go through `POST /games/{game_id}/transition`. Because transitions authorize against the requester's identity, the page MUST share the cookie of whoever created the game — hence `page.request`.
- **Direct-URL boot needs localStorage.** `bootFromUrl` (`web/src/routes/play.ts`) resolves a player by reading `localStorage['spades_game_{shortId}']` = `{"gid":gameId,"pid":playerId}` first. For an API-created game there is no such entry, and the game's short-id won't resolve via the `by-player-url` fallback. So `createAiGame` must seed that key via `page.addInitScript` before `goto`.
- **In-game DOM anchors** (verified in `web/src/routes/play.ts`, `web/src/ui/components/game-table.ts`, `web/src/ui/components/scores.ts`):
  - Betting: `.spades-bets` contains 14 buttons labelled `0`–`13`. It renders **only when it is the local player's turn to bet**.
  - Center text: `.spades-center-text` (non-empty when waiting on others / game over).
  - Hand: `.hand-container .card` (13 at deal, decrements as the local player plays).
  - Playable cards: `.cm-clickable` (added only to legal cards on the local player's turn; a plain click plays one).
  - Scores: `.spades-scores` / `.spades-score-team` ("Score: N | Bags: M").
  - Game over: a `Play Again` button renders; center shows "Team A wins!" / "Team B wins!" / "It's a tie!".
- **Home menu anchors** (`web/src/routes/home.ts`): `[data-testid="home-menu"]`; quickplay buttons labelled `5+3` / `10+5` / `15+10`; `[data-testid="play-friends"]` → `/create`; `[data-testid="play-computers"]` → `/play/new-ai`.
- **Lobby/challenge anchors** (`web/src/routes/play.ts`): open seats are `button.seat-open`; the join modal is `.join-modal` with an `input`; submit is a `Join` button. A challenge needs **all 4 seats filled by humans** before the server emits `game_start`.
- **Create form** (`web/src/routes/create.ts`): clicking `Create` (exact) with no seat selected opens a challenge with 4 empty seats and redirects to the lobby at `/play/{shortId}`.
- **Auth contract** (`web/src/state/session.ts`, `crates/spades-server/src/handlers_auth.rs`): `POST /auth/register {username,email,password}` → 201 + sets the session cookie. Validation: **username** 2–20 chars of `[A-Za-z0-9_-]`; **password** 8–256 chars and not in a weak-password list; **email** must look like an email.

## Known constraints

- **`register` is IP-rate-limited** (`crates/spades-server/src/auth/rate_limit.rs`): `Quota::per_minute(3).allow_burst(20)` keyed on the client IP. The entire suite runs from `127.0.0.1`, so all registrations share one bucket. This build performs ~10 registrations per clean run (the AI flow registers **zero** — anonymous identities work), comfortably under the burst of 20. Mitigations baked into this plan: the AI flow uses anonymous identities; `registerUser` raises a clear, identifiable error on HTTP 429 so a rate-limit trip is never mistaken for a product bug. If the suite later grows past ~20 registrations/minute, the escape hatch is to switch the multiplayer flows to anonymous contexts (drop the `registerUser` call in `newPlayerContext`) — they work anonymously today.

## File structure

```
web/tests/e2e/
  fixtures.ts            # CREATE — extends Playwright `test` (apiUp auto-fixture + authedPage);
                         #          exports newPlayerContext(); re-exports expect. Supersedes setup.ts.
  helpers/
    identity.ts          # CREATE — uniqueUser(): unique, validation-safe TestUser
    auth.ts              # CREATE — registerUser(request, user?): POST /auth/register
    games.ts             # CREATE — createAiGame(request); seedAiSession(page, game)
    routing.ts           # CREATE — GAME_URL_RE + waitForGameUrl(page)
  pages/
    home-page.ts         # CREATE — HomePage
    game-page.ts         # CREATE — GamePage (the workhorse)
    create-page.ts       # CREATE — CreatePage
    lobby-page.ts        # CREATE — LobbyPage
  flows/
    ai-game.spec.ts      # CREATE — moved+expanded from tests/e2e/ai-game.spec.ts
    matchmaking.spec.ts  # CREATE — replaces tests/e2e/quickplay.spec.ts
    challenge.spec.ts    # CREATE — replaces tests/e2e/friends.spec.ts
  setup.ts               # DELETE — replaced by fixtures.ts
  ai-game.spec.ts        # DELETE — moved to flows/
  quickplay.spec.ts      # DELETE — replaced by flows/matchmaking.spec.ts
  friends.spec.ts        # DELETE — replaced by flows/challenge.spec.ts
  auth.spec.ts           # MODIFY — update import './setup' -> './fixtures'
  smoke.spec.ts          # UNCHANGED — imports @playwright/test directly (UI-only, no backend needed)
```

## How to run a single spec

The Playwright config auto-starts both servers (`reuseExistingServer` locally, so a running `make dev` is reused). First run may compile the Rust server (up to the 120s `webServer` timeout). Commands below are run from the repo root.

- One file: `pnpm -C web exec playwright test tests/e2e/flows/ai-game.spec.ts --project=chromium`
- Whole suite: `make e2e` (equivalently `pnpm -C web test:e2e`)

---

### Task 1: Identity + auth helpers + fixtures (the authed foundation)

**Files:**
- Create: `web/tests/e2e/helpers/identity.ts`
- Create: `web/tests/e2e/helpers/auth.ts`
- Create: `web/tests/e2e/fixtures.ts`
- Modify: `web/tests/e2e/auth.spec.ts` (import path)
- Delete: `web/tests/e2e/setup.ts`
- Test (temporary): `web/tests/e2e/flows/_foundation.spec.ts`

- [ ] **Step 1: Write the failing test** — a temporary foundation spec that proves `authedPage` starts logged in.

Create `web/tests/e2e/flows/_foundation.spec.ts`:

```ts
import { test, expect } from '../fixtures';

test('authedPage is recognized by GET /auth/me', async ({ authedPage }) => {
  const res = await authedPage.request.get('/auth/me');
  expect(res.ok()).toBe(true);
  const me = (await res.json()) as { username: string };
  expect(me.username).toMatch(/^e2e_/);
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C web exec playwright test tests/e2e/flows/_foundation.spec.ts --project=chromium`
Expected: FAIL — cannot resolve `../fixtures`.

- [ ] **Step 3: Create `web/tests/e2e/helpers/identity.ts`**

```ts
import { randomUUID } from 'node:crypto';

export type TestUser = { username: string; email: string; password: string };

/** Unique, validation-safe credentials. username: "e2e_" + 10 hex = 14 chars (<=20, [A-Za-z0-9_]). */
export function uniqueUser(): TestUser {
  const id = randomUUID().replace(/-/g, '').slice(0, 10);
  const username = `e2e_${id}`;
  return {
    username,
    email: `${username}@example.test`,
    password: 'e2e-strong-passphrase-9',
  };
}
```

- [ ] **Step 4: Create `web/tests/e2e/helpers/auth.ts`**

```ts
import type { APIRequestContext } from '@playwright/test';
import { uniqueUser, type TestUser } from './identity';

/**
 * Registers a user via POST /auth/register. Pass a context-bound request
 * (page.request / context.request) so the session cookie lands in the
 * browser context that will navigate the app.
 */
export async function registerUser(
  request: APIRequestContext,
  user: TestUser = uniqueUser(),
): Promise<TestUser> {
  const res = await request.post('/auth/register', {
    data: { username: user.username, email: user.email, password: user.password },
  });
  if (res.status() === 429) {
    throw new Error(
      'register rate-limited (HTTP 429). See plan "Known constraints": too many ' +
        'registrations from 127.0.0.1 (burst 20 / 3-per-min).',
    );
  }
  if (!res.ok()) {
    throw new Error(`register failed: ${res.status()} ${await res.text()}`);
  }
  return user;
}
```

- [ ] **Step 5: Create `web/tests/e2e/fixtures.ts`**

```ts
import {
  test as base,
  expect,
  type Page,
  type Browser,
  type BrowserContext,
} from '@playwright/test';
import { registerUser } from './helpers/auth';

const BACKEND_URL = process.env.VITE_API_URL ?? 'http://localhost:3000';
// Mirrors `use.baseURL` in playwright.config.ts. browser.newContext() does not
// inherit config `use` options, so multi-context helpers set it explicitly.
const APP_URL = 'http://localhost:5173';

type Fixtures = {
  apiUp: void;
  authedPage: Page;
};

export const test = base.extend<Fixtures>({
  apiUp: [
    // eslint-disable-next-line no-empty-pattern
    async ({}, use) => {
      const res = await fetch(`${BACKEND_URL}/health`).catch(() => null);
      if (!res || !res.ok) {
        throw new Error(`rust-spades not reachable at ${BACKEND_URL}/health`);
      }
      await use();
    },
    { auto: true },
  ],

  // A Page whose context is already authenticated. registerUser uses
  // page.request, which shares the cookie jar with this page's context.
  authedPage: async ({ page }, use) => {
    await registerUser(page.request);
    await use(page);
  },
});

export { expect };

/**
 * Creates an independent authenticated browser context + page, for multi-player
 * flows that need several simultaneous clients. Caller must close the context.
 */
export async function newPlayerContext(
  browser: Browser,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ baseURL: APP_URL });
  await registerUser(context.request);
  const page = await context.newPage();
  return { context, page };
}
```

- [ ] **Step 6: Update `web/tests/e2e/auth.spec.ts` import and delete `setup.ts`**

In `web/tests/e2e/auth.spec.ts`, change the import source from `'./setup'` to `'./fixtures'` (leave everything else). Then delete the old setup file:

```bash
rm web/tests/e2e/setup.ts
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `pnpm -C web exec playwright test tests/e2e/flows/_foundation.spec.ts tests/e2e/auth.spec.ts --project=chromium`
Expected: PASS (both files).

- [ ] **Step 8: Commit**

```bash
git add web/tests/e2e/fixtures.ts web/tests/e2e/helpers/identity.ts web/tests/e2e/helpers/auth.ts web/tests/e2e/flows/_foundation.spec.ts web/tests/e2e/auth.spec.ts
git rm web/tests/e2e/setup.ts
git commit -m "test(e2e): fixtures + identity/auth helpers (authedPage foundation)"
```

---

### Task 2: Routing helper + HomePage; migrate smoke coverage

**Files:**
- Create: `web/tests/e2e/helpers/routing.ts`
- Create: `web/tests/e2e/pages/home-page.ts`
- Test (temporary, extends `_foundation.spec.ts`): add a HomePage case

- [ ] **Step 1: Write the failing test** — append to `web/tests/e2e/flows/_foundation.spec.ts`:

```ts
import { HomePage } from '../pages/home-page';

test('HomePage renders the five-button menu', async ({ page }) => {
  const home = new HomePage(page);
  await home.goto();
  await expect(home.menu()).toBeVisible();
  await expect(home.menu().locator('button')).toHaveCount(5);
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C web exec playwright test tests/e2e/flows/_foundation.spec.ts --project=chromium`
Expected: FAIL — cannot resolve `../pages/home-page`.

- [ ] **Step 3: Create `web/tests/e2e/helpers/routing.ts`**

```ts
import type { Page } from '@playwright/test';

/** Matches an in-game URL like /play/abc123 but not the /play/new-ai bootstrap. */
export const GAME_URL_RE = /\/play\/(?!new-ai)[^/]+$/;

/** Waits for SPA pushState navigation into a real game URL. */
export async function waitForGameUrl(page: Page, timeout = 15_000): Promise<void> {
  await page.waitForFunction(() => /\/play\/(?!new-ai)[^/]+$/.test(location.pathname), {
    timeout,
  });
}
```

- [ ] **Step 4: Create `web/tests/e2e/pages/home-page.ts`**

```ts
import type { Page, Locator } from '@playwright/test';

export class HomePage {
  constructor(private readonly page: Page) {}

  async goto(): Promise<void> {
    await this.page.goto('/');
  }

  menu(): Locator {
    return this.page.locator('[data-testid="home-menu"]');
  }

  async quickplay(label: '5+3' | '10+5' | '15+10'): Promise<void> {
    await this.page.getByRole('button', { name: label, exact: true }).click();
  }

  async playWithComputers(): Promise<void> {
    await this.page.getByTestId('play-computers').click();
  }

  async playWithFriends(): Promise<void> {
    await this.page.getByTestId('play-friends').click();
  }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `pnpm -C web exec playwright test tests/e2e/flows/_foundation.spec.ts --project=chromium`
Expected: PASS (all cases in the file).

- [ ] **Step 6: Commit**

```bash
git add web/tests/e2e/helpers/routing.ts web/tests/e2e/pages/home-page.ts web/tests/e2e/flows/_foundation.spec.ts
git commit -m "test(e2e): routing helper + HomePage page object"
```

---

### Task 3: GamePage + AI-game helpers; entry-point + reload spec

**Files:**
- Create: `web/tests/e2e/helpers/games.ts`
- Create: `web/tests/e2e/pages/game-page.ts`
- Create: `web/tests/e2e/flows/ai-game.spec.ts`
- Delete: `web/tests/e2e/ai-game.spec.ts`

- [ ] **Step 1: Write the failing test** — create `web/tests/e2e/flows/ai-game.spec.ts` with the entry-point + reload case (mirrors the old test, via page objects):

```ts
import { test, expect } from '../fixtures';
import { GamePage } from '../pages/game-page';
import { HomePage } from '../pages/home-page';
import { waitForGameUrl } from '../helpers/routing';

test('Play with computers: bet, then reload preserves the 13-card hand', async ({ page }) => {
  await new HomePage(page).goto();
  await new HomePage(page).playWithComputers();
  await waitForGameUrl(page);

  const game = new GamePage(page);
  await game.bet(3);
  await game.waitForPlayable();

  await page.reload();
  await expect(game.hand()).toHaveCount(13, { timeout: 10_000 });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C web exec playwright test tests/e2e/flows/ai-game.spec.ts --project=chromium`
Expected: FAIL — cannot resolve `../pages/game-page`.

- [ ] **Step 3: Create `web/tests/e2e/pages/game-page.ts`**

```ts
import { expect, type Page, type Locator } from '@playwright/test';

export class GamePage {
  constructor(private readonly page: Page) {}

  bets(): Locator {
    return this.page.locator('.spades-bets');
  }
  hand(): Locator {
    return this.page.locator('.hand-container .card');
  }
  clickableCards(): Locator {
    return this.page.locator('.cm-clickable');
  }
  centerText(): Locator {
    return this.page.locator('.spades-center-text');
  }

  /** Resolves once the game is in BETTING: either our bet buttons or non-empty center text. */
  async waitForBetting(): Promise<void> {
    await this.page.waitForFunction(
      () =>
        document.querySelector('.spades-bets') !== null ||
        (document.querySelector('.spades-center-text')?.textContent?.trim() ?? '') !== '',
      { timeout: 15_000 },
    );
  }

  /** Waits for our bet turn (bet buttons render only on our turn) and bets `n`. */
  async bet(n: number): Promise<void> {
    await expect(this.bets()).toBeVisible({ timeout: 15_000 });
    await this.bets().getByRole('button', { name: String(n), exact: true }).click();
  }

  /** Resolves when at least one legal card is clickable (i.e., it is our turn to play). */
  async waitForPlayable(): Promise<void> {
    await expect(this.clickableCards().first()).toBeVisible({ timeout: 15_000 });
  }

  async playFirstLegalCard(): Promise<void> {
    await this.waitForPlayable();
    await this.clickableCards().first().click();
  }

  /**
   * Plays the local player's card in all 13 tricks of the current hand. Uses the
   * hand-count decrement as the per-trick synchronization signal: bots resolve the
   * rest of each trick server-side, so our hand drops by exactly one per play.
   */
  async playOutHand(): Promise<void> {
    for (let remaining = 13; remaining > 0; remaining--) {
      await expect(this.hand()).toHaveCount(remaining, { timeout: 20_000 });
      await this.playFirstLegalCard();
      await expect(this.hand()).toHaveCount(remaining - 1, { timeout: 20_000 });
    }
  }
}
```

- [ ] **Step 4: Create `web/tests/e2e/helpers/games.ts`**

```ts
import type { APIRequestContext, Page } from '@playwright/test';

export type AiGame = { gameId: string; playerId: string; shortId: string };

/**
 * Creates a 1-human + 3-bot game (auto-started) via the API. Pass a context-bound
 * request (page.request / context.request): the human seat is owned by the
 * requester's anonymous identity, and the page must share that cookie to make moves.
 */
export async function createAiGame(request: APIRequestContext): Promise<AiGame> {
  const created = await request.post('/games', { data: { max_points: 500, num_humans: 1 } });
  if (!created.ok()) {
    throw new Error(`create AI game failed: ${created.status()} ${await created.text()}`);
  }
  const { game_id, player_ids } = (await created.json()) as {
    game_id: string;
    player_ids: string[];
  };
  const stateRes = await request.get(`/games/${game_id}`);
  const state = (await stateRes.json()) as { short_id?: string | null };
  const shortId = state.short_id ?? game_id;
  return { gameId: game_id, playerId: player_ids[0]!, shortId };
}

/**
 * Seeds the localStorage session the SPA reads on boot, so navigating directly to
 * /play/{shortId} resolves the player. Must run before page.goto.
 */
export async function seedAiSession(page: Page, game: AiGame): Promise<void> {
  await page.addInitScript(
    ([shortId, gid, pid]) => {
      localStorage.setItem(`spades_game_${shortId}`, JSON.stringify({ gid, pid }));
    },
    [game.shortId, game.gameId, game.playerId] as const,
  );
}
```

- [ ] **Step 5: Delete the old AI spec**

```bash
git rm web/tests/e2e/ai-game.spec.ts
```

- [ ] **Step 6: Run test to verify it passes**

Run: `pnpm -C web exec playwright test tests/e2e/flows/ai-game.spec.ts --project=chromium`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/tests/e2e/pages/game-page.ts web/tests/e2e/helpers/games.ts web/tests/e2e/flows/ai-game.spec.ts
git rm web/tests/e2e/ai-game.spec.ts
git commit -m "test(e2e): GamePage + AI-game helpers; entry-point + reload spec"
```

---

### Task 4: AI full-hand lifecycle spec

**Files:**
- Modify: `web/tests/e2e/flows/ai-game.spec.ts` (add the lifecycle test)

- [ ] **Step 1: Write the failing test** — append to `web/tests/e2e/flows/ai-game.spec.ts`:

```ts
import { createAiGame, seedAiSession } from '../helpers/games';

test('AI lifecycle: bet, play all 13 tricks, advance to the next hand', async ({ page }) => {
  // API-assisted setup: create the game with the page's own anon identity so
  // the page is authorized to make the human's moves, then seed the boot session.
  const game = await createAiGame(page.request);
  await seedAiSession(page, game);
  await page.goto(`/play/${game.shortId}`);

  const g = new GamePage(page);
  await g.waitForBetting();
  await g.bet(3);
  await g.playOutHand(); // hand drains 13 -> 0

  // One hand never reaches 500, so a fresh hand should be dealt (13 cards again);
  // accept GAME_OVER as a valid alternative for robustness.
  await expect(async () => {
    const nextHandDealt = (await g.hand().count()) === 13;
    const gameOver = await page
      .getByRole('button', { name: 'Play Again' })
      .isVisible()
      .catch(() => false);
    expect(nextHandDealt || gameOver).toBe(true);
  }).toPass({ timeout: 20_000 });
});
```

- [ ] **Step 2: Run it to verify it fails (then passes)**

This test depends only on code that already exists after Task 3, so it should pass immediately. Run it and confirm green:

Run: `pnpm -C web exec playwright test tests/e2e/flows/ai-game.spec.ts --project=chromium -g "AI lifecycle"`
Expected: PASS. (If it fails, debug `playOutHand` against the live UI — confirm `.cm-clickable` appears on the local player's turn and `.hand-container .card` decrements per play.)

- [ ] **Step 3: Verify the test is meaningful (transient sabotage)**

Temporarily change `await g.bet(3);` to `await g.bet(13);` (an over-bet the bots can't satisfy is irrelevant — instead change the loop bound). Simpler: in `playOutHand`, temporarily change `remaining > 0` to `remaining > 6` and re-run; expect FAIL at the next-hand assertion (hand never reaches 0, no redeal). Revert after confirming.

Run: `pnpm -C web exec playwright test tests/e2e/flows/ai-game.spec.ts --project=chromium -g "AI lifecycle"`
Expected: FAIL while sabotaged; PASS after revert.

- [ ] **Step 4: Commit**

```bash
git add web/tests/e2e/flows/ai-game.spec.ts
git commit -m "test(e2e): AI full-hand lifecycle (bet, 13 tricks, next-hand)"
```

---

### Task 5: Quickplay matchmaking flow (4 contexts)

**Files:**
- Create: `web/tests/e2e/flows/matchmaking.spec.ts`
- Delete: `web/tests/e2e/quickplay.spec.ts`

- [ ] **Step 1: Write the failing test** — create `web/tests/e2e/flows/matchmaking.spec.ts`:

```ts
import { test, newPlayerContext } from '../fixtures';
import { HomePage } from '../pages/home-page';
import { GamePage } from '../pages/game-page';
import { waitForGameUrl } from '../helpers/routing';

test('four players matched via quickplay reach the betting phase', async ({ browser }) => {
  const players = await Promise.all([0, 1, 2, 3].map(() => newPlayerContext(browser)));
  try {
    await Promise.all(players.map((p) => new HomePage(p.page).goto()));
    await Promise.all(players.map((p) => new HomePage(p.page).quickplay('5+3')));

    // All four land in a real game and reach BETTING, regardless of arrival order.
    await Promise.all(players.map((p) => waitForGameUrl(p.page)));
    await Promise.all(players.map((p) => new GamePage(p.page).waitForBetting()));
  } finally {
    await Promise.all(players.map((p) => p.context.close()));
  }
});
```

- [ ] **Step 2: Run it to verify it passes**

All dependencies exist after Tasks 1–3. Run:

Run: `pnpm -C web exec playwright test tests/e2e/flows/matchmaking.spec.ts --project=chromium`
Expected: PASS.

- [ ] **Step 3: Delete the superseded spec**

```bash
git rm web/tests/e2e/quickplay.spec.ts
```

- [ ] **Step 4: Commit**

```bash
git add web/tests/e2e/flows/matchmaking.spec.ts
git rm web/tests/e2e/quickplay.spec.ts
git commit -m "test(e2e): quickplay matchmaking flow via page objects (4 contexts)"
```

---

### Task 6: Friends challenge flow (CreatePage + LobbyPage, 4 contexts)

**Files:**
- Create: `web/tests/e2e/pages/create-page.ts`
- Create: `web/tests/e2e/pages/lobby-page.ts`
- Create: `web/tests/e2e/flows/challenge.spec.ts`
- Delete: `web/tests/e2e/friends.spec.ts`

- [ ] **Step 1: Write the failing test** — create `web/tests/e2e/flows/challenge.spec.ts`:

```ts
import { test, newPlayerContext } from '../fixtures';
import { HomePage } from '../pages/home-page';
import { CreatePage } from '../pages/create-page';
import { LobbyPage } from '../pages/lobby-page';
import { GamePage } from '../pages/game-page';
import { waitForGameUrl } from '../helpers/routing';

test('friends challenge: create, four players join, reach betting', async ({ browser }) => {
  const players = await Promise.all([0, 1, 2, 3].map(() => newPlayerContext(browser)));
  try {
    // Creator opens a challenge with four empty seats and lands in the lobby.
    const creator = players[0]!.page;
    await new HomePage(creator).goto();
    await new HomePage(creator).playWithFriends();
    await creator.waitForURL(/\/create$/);
    await new CreatePage(creator).create();
    await creator.waitForFunction(() => /\/play\/[^/]+$/.test(location.pathname), {
      timeout: 15_000,
    });
    const shareUrl = creator.url();

    // Each player claims the first open seat. Sequential so two players never
    // grab the same seat at the same instant.
    for (let i = 0; i < players.length; i++) {
      const p = players[i]!.page;
      if (i > 0) await p.goto(shareUrl);
      await new LobbyPage(p).joinFirstOpenSeat(`Player${i + 1}`);
    }

    // When the fourth seat fills, everyone navigates into the game and bets.
    await Promise.all(players.map((p) => waitForGameUrl(p.page)));
    await Promise.all(players.map((p) => new GamePage(p.page).waitForBetting()));
  } finally {
    await Promise.all(players.map((p) => p.context.close()));
  }
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C web exec playwright test tests/e2e/flows/challenge.spec.ts --project=chromium`
Expected: FAIL — cannot resolve `../pages/create-page`.

- [ ] **Step 3: Create `web/tests/e2e/pages/create-page.ts`**

```ts
import type { Page } from '@playwright/test';

export class CreatePage {
  constructor(private readonly page: Page) {}

  /** Submits the challenge form. With no seat picked, all four seats stay open. */
  async create(): Promise<void> {
    await this.page.getByRole('button', { name: 'Create', exact: true }).click();
  }
}
```

- [ ] **Step 4: Create `web/tests/e2e/pages/lobby-page.ts`**

```ts
import type { Page } from '@playwright/test';

export class LobbyPage {
  constructor(private readonly page: Page) {}

  /** Claims the first open seat, names the player, joins, and waits for the modal to close. */
  async joinFirstOpenSeat(name: string): Promise<void> {
    await this.page.locator('button.seat-open').first().click({ timeout: 10_000 });
    await this.page.locator('.join-modal input').fill(name);
    await this.page.getByRole('button', { name: 'Join', exact: true }).click();
    await this.page.waitForFunction(() => document.querySelector('.join-modal') === null, {
      timeout: 10_000,
    });
  }
}
```

- [ ] **Step 5: Delete the superseded spec**

```bash
git rm web/tests/e2e/friends.spec.ts
```

- [ ] **Step 6: Run test to verify it passes**

Run: `pnpm -C web exec playwright test tests/e2e/flows/challenge.spec.ts --project=chromium`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/tests/e2e/pages/create-page.ts web/tests/e2e/pages/lobby-page.ts web/tests/e2e/flows/challenge.spec.ts
git rm web/tests/e2e/friends.spec.ts
git commit -m "test(e2e): friends challenge flow (CreatePage + LobbyPage, 4 contexts)"
```

---

### Task 7: Remove scaffolding, lint, full-suite green

**Files:**
- Delete: `web/tests/e2e/flows/_foundation.spec.ts`
- Verify: all of `web/tests/e2e/`

- [ ] **Step 1: Fold the foundation checks into a permanent home, then delete the scaffold**

The two cases in `_foundation.spec.ts` (authedPage → /auth/me, HomePage menu) are valuable. Move the HomePage menu assertion into `smoke.spec.ts` only if not already covered (smoke already asserts the 5-button menu — so drop the duplicate), and move the `authedPage` → `/auth/me` check into `auth.spec.ts` as a new `test`. Then delete the scaffold:

```bash
git rm web/tests/e2e/flows/_foundation.spec.ts
```

Add to `web/tests/e2e/auth.spec.ts` (using its existing `./fixtures` import):

```ts
test('authedPage fixture is recognized by GET /auth/me', async ({ authedPage }) => {
  const res = await authedPage.request.get('/auth/me');
  expect(res.ok()).toBe(true);
  const me = (await res.json()) as { username: string };
  expect(me.username).toMatch(/^e2e_/);
});
```

(Ensure `expect` is imported in `auth.spec.ts` from `./fixtures`.)

- [ ] **Step 2: Lint the new code**

Run: `pnpm -C web lint`
Expected: PASS, 0 warnings. Fix any issues (common: unused imports, `no-empty-pattern` on the `apiUp` fixture — keep the existing eslint-disable comment).

- [ ] **Step 3: Run the full e2e suite**

Run: `make e2e`
Expected: PASS — `smoke.spec.ts`, `auth.spec.ts`, `flows/ai-game.spec.ts`, `flows/matchmaking.spec.ts`, `flows/challenge.spec.ts` all green.

- [ ] **Step 4: Confirm no stale references and the tree is clean**

Run: `grep -rn "./setup" web/tests/e2e || echo "no stale setup imports"`
Expected: `no stale setup imports`.

Run: `ls web/tests/e2e/*.spec.ts`
Expected: only `auth.spec.ts` and `smoke.spec.ts` remain at the top level (flows moved into `flows/`).

- [ ] **Step 5: Commit**

```bash
git add web/tests/e2e/auth.spec.ts
git rm web/tests/e2e/flows/_foundation.spec.ts
git commit -m "test(e2e): retire scaffold; fold auth/me check into auth.spec; full suite green"
```

---

## Self-review notes

**Spec coverage:**
- Foundation (fixtures + helpers + page objects) → Tasks 1–3, 6.
- API-assisted auth (register + cookie) → Task 1 (`authedPage`, `registerUser`).
- API-assisted AI-game setup → Task 3 (`createAiGame`, `seedAiSession`).
- AI lifecycle (full hand + next-hand) → Task 4. Entry-point + reload → Task 3.
- Quickplay (4 contexts) → Task 5. Friends challenge (4 contexts) → Task 6.
- Reliability: unique identities (`uniqueUser`), deterministic waits (page-object methods, `waitForGameUrl`, `toPass`/`toHaveCount` — no `waitForTimeout`), context teardown in `finally` → Tasks 1, 5, 6.
- Smoke nav/404 kept unchanged → noted in file structure.

**Deviations from the spec (deliberate, justified):**
- **AI flow uses an anonymous identity, not a registered user.** `POST /games` works anonymously and registering would only add rate-limit pressure with no coverage gain. The `authedPage` registration path is still built and exercised (Tasks 1, 7).
- **No explicit `DELETE /challenges/{id}` teardown.** The spec proposed it; it's unnecessary because short-ids are unique per run and the e2e backend is in-memory (`DB=`), so a lingering challenge can't perturb another test. Context teardown (always closing contexts) is kept. If a future test asserts on the open-challenges list, add a best-effort cancel then.
- **`helpers/api.ts` from the spec sketch is not created.** Its job (backend base URL for the health probe) collapsed into `fixtures.ts` once setup standardized on context-bound `page.request`/`context.request`; a separate :3000 request context would not share cookies and is unused.

**Placeholder scan:** none — every step has concrete code/commands.

**Type consistency:** `TestUser` (identity.ts) consumed by `registerUser`. `AiGame` (games.ts) produced by `createAiGame`, consumed by `seedAiSession`. `GamePage`/`HomePage`/`CreatePage`/`LobbyPage` constructors all take `Page`. `newPlayerContext` returns `{context, page}` consumed consistently in Tasks 5–6. `waitForGameUrl(page)` signature stable across Tasks 2/5/6.
