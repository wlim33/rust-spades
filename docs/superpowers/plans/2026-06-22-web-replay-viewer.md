# Web Replay Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a lichess-style step-through replay viewer at `/replay/:id` that consumes `GET /games/{id}/replay.json`, shows all four (revealed) hands oriented around the viewer, and steps/animates through bids and tricks with a running score.

**Architecture:** A pure `ReplayController` (no DOM) holds the fetched model + a move cursor and derives a `ViewState` per step (including the trick winner, the one rule the client computes). A `ReplayBoard` renders that `ViewState` by reusing the existing card-animation *primitives* (`animation.ts`, `card-el.ts`) and the `gameTable` scaffold — NOT the live `CardOrchestrator` (which can't show four face-up hands and is welded to live-play/WS semantics). A lazy `/replay/:id` route wires controller↔board with playback controls; profile rows and the end-of-game screen link into it.

**Tech Stack:** TypeScript SPA — lit-html, @preact/signals-core, navaid router, openapi-fetch (typed client). Vitest (unit/component) + Playwright (e2e). Card primitives in `web/src/cards/`.

## Global Constraints

- This implements Sections 2 & 3 of `docs/superpowers/specs/2026-06-22-replay-viewer-design.md`. The server endpoint (Section 1) is already done and committed.
- Reuse the animation **primitives** (`animation.ts` `animateTo`/`EASE`, `card-el.ts` `createFront`/`setPos`/`CardEl`) and the `gameTable` scaffold; do NOT instantiate `CardOrchestrator` and do NOT modify it.
- Styling uses ONLY tokens from `web/src/ui/tokens.css` (no raw hex/px); accent color reserved for interactive elements (CLAUDE.md web invariant).
- Respect `prefers-reduced-motion`: animated transitions fall back to instant placement (mirror `orchestrator.ts`'s `skipAnims()` gate).
- The replay model's cards are `trick_notation` shape: `{ suit: "S"|"H"|"D"|"C", rank: "2".."9"|"T"|"J"|"Q"|"K"|"A" }` (single-char syms). The app's `Card` is `{ suit: "Spade"|"Heart"|"Diamond"|"Club", rank: <app Rank> }`. A `tnCardToApp` mapper bridges them. Spades replays contain only suited cards (no specials).
- `seatRel(absIdx, myIdx)` (`web/src/state/helpers.ts`) maps an absolute seat index (0..3, order N,E,S,W) to a relative `Seat` ('south'|'west'|'north'|'east') for the viewer; use it with `viewer_seat` (or 0 when null) to orient the board.
- Run web scripts via `pnpm -C web <script>`: `test`, `test:e2e`, `lint`, `build`. Lint is ESLint flat config.
- Commit with pathspec; the repo carries unrelated `web/` WIP (`web/src/cards/animation.ts`, `web/src/cards/orchestrator.ts`, `web/src/ui/design.css`, `web/src/ui/tokens.css`, `web/tests/unit/animation.spec.ts`) — never stage those. Working on `master` (no branch) per the session's standing choice.
- Do NOT regenerate OpenAPI artifacts (no server changes here).

---

## File Structure

- `web/src/replay/types.ts` — local view types (`ViewState`, `Move`) + the `tnCardToApp` mapper + replay-model type aliases off the generated schema.
- `web/src/replay/controller.ts` — `ReplayController` (pure logic).
- `web/src/replay/board.ts` — `ReplayBoard` (DOM rendering, reuses primitives + gameTable scaffold).
- `web/src/routes/replay.ts` — the `/replay/:id` route module (fetch, page shell, controls, controller↔board wiring, error states).
- `web/src/router.ts` — register the lazy route (modify).
- `web/src/routes/profile.ts` — link game-history rows to `/replay/:id` (modify).
- `web/src/routes/game-view.ts` — "Review game" button on terminal state (modify).
- `web/tests/unit/replay-controller.spec.ts` — controller unit tests.
- `web/tests/unit/replay-board.spec.ts` — board component test.
- `web/tests/e2e/replay.spec.ts` — end-to-end (or extend an existing e2e file).
- `web/src/ui/design.css` — replay-specific layout/controls styles. NOTE: this file is in the WIP set; see Task 5 for how to handle.

---

### Task 1: Replay types + `tnCardToApp` mapper + API fetch

**Files:**
- Create: `web/src/replay/types.ts`
- Create or modify: `web/src/api/hand-written.ts` (add `fetchReplay`) — follow its existing pattern
- Test: `web/tests/unit/replay-controller.spec.ts` (start the file with mapper tests)

**Interfaces:**
- Produces:
  - `type ReplayResponse = components['schemas']['GameReplayResponse']`
  - `type TnCard = { suit: string; rank: string }` (the replay card shape)
  - `type TnEvent` — the model event union (derive from `ReplayResponse['model']['events'][number]`)
  - `function tnCardToApp(c: TnCard): Card` — maps single-char sym → app `Card`.
  - `async function fetchReplay(id: string): Promise<ReplayResponse>` — GET `/games/{game_id}/replay.json`.

- [ ] **Step 1: Write the failing mapper test**

Create `web/tests/unit/replay-controller.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { tnCardToApp } from '../../src/replay/types';

describe('tnCardToApp', () => {
  it('maps single-char syms to app card', () => {
    expect(tnCardToApp({ suit: 'S', rank: 'A' })).toEqual({ suit: 'Spade', rank: 'Ace' });
    expect(tnCardToApp({ suit: 'C', rank: 'T' })).toEqual({ suit: 'Club', rank: 'Ten' });
    expect(tnCardToApp({ suit: 'H', rank: '2' })).toEqual({ suit: 'Heart', rank: 'Two' });
    expect(tnCardToApp({ suit: 'D', rank: 'K' })).toEqual({ suit: 'Diamond', rank: 'King' });
  });
});
```

> Confirm the app `Rank` spelling first: read `web/src/state/helpers.ts` lines 1–20 for the exact `Rank` union values (e.g. `'Two'..'Ace'`). Use those exact strings in the mapper and test. If the app uses different rank tokens, adjust the expected values to match.

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test -- replay-controller`
Expected: FAIL — cannot import `tnCardToApp`.

- [ ] **Step 3: Implement types + mapper + fetch**

Create `web/src/replay/types.ts`:

```ts
import type { components } from '../api/schema';
import type { Card, Suit, Rank } from '../state/helpers';

export type ReplayResponse = components['schemas']['GameReplayResponse'];
export type ReplayModel = ReplayResponse['model'];
export type TnEvent = ReplayModel['events'][number];
export type TnCard = { suit: string; rank: string };

const SUIT_MAP: Record<string, Suit> = { S: 'Spade', H: 'Heart', D: 'Diamond', C: 'Club' };
const RANK_MAP: Record<string, Rank> = {
  '2': 'Two', '3': 'Three', '4': 'Four', '5': 'Five', '6': 'Six', '7': 'Seven',
  '8': 'Eight', '9': 'Nine', T: 'Ten', J: 'Jack', Q: 'Queen', K: 'King', A: 'Ace',
};

/** Map a trick-notation card (single-char syms) to the app's Card type. */
export function tnCardToApp(c: TnCard): Card {
  const suit = SUIT_MAP[c.suit];
  const rank = RANK_MAP[c.rank];
  if (!suit || !rank) throw new Error(`unmappable card: ${c.rank}${c.suit}`);
  return { suit, rank };
}
```

Add to `web/src/api/hand-written.ts` (mirror its existing helpers; use the typed `api` client from `client.ts`):

```ts
import { api } from './client';
import type { ReplayResponse } from '../replay/types';

/** Fetch a finished game's replay model. Throws on 403 (in-progress) / 404. */
export async function fetchReplay(id: string): Promise<ReplayResponse> {
  const { data, error } = await api.GET('/games/{game_id}/replay.json', {
    params: { path: { game_id: id } },
  });
  if (error || !data) throw new Error(typeof error === 'string' ? error : 'replay fetch failed');
  return data as ReplayResponse;
}
```

> Match the actual `api.GET` call signature used elsewhere in `hand-written.ts`/`client.ts` (openapi-fetch returns `{ data, error, response }`). If existing helpers surface the HTTP status for 403-vs-404 distinction, follow that pattern so the route (Task 4) can show the right message — e.g. return/throw something carrying `response.status`.

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm -C web test -- replay-controller`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/replay/types.ts web/src/api/hand-written.ts web/tests/unit/replay-controller.spec.ts
git commit -m "feat(web): replay types, tnCardToApp mapper, fetchReplay"
```

---

### Task 2: `ReplayController` (pure logic)

**Files:**
- Create: `web/src/replay/controller.ts`
- Test: `web/tests/unit/replay-controller.spec.ts` (extend)

**Interfaces:**
- Consumes: `ReplayResponse`, `TnEvent`, `tnCardToApp`, app `Card`, `getTrickWinner`-style rule.
- Produces:
  - `type Move = { kind: 'bid'; round: number } | { kind: 'card'; round: number }` (one step = one bid or one card). (Internal granularity; the controller exposes navigation, not Move directly, but define it for tests.)
  - `type ViewState = { round: number; totalRounds: number; toAct: Seat | null; hands: Record<Seat, Card[]>; trick: { seat: Seat; card: Card }[]; trickWinner: Seat | null; bids: Record<Seat, number | null>; tricksWon: Record<Seat, number>; score: [number, number]; phase: 'bid' | 'play' | 'done'; aborted: boolean }`
  - `class ReplayController` with: `constructor(res: ReplayResponse)`; `viewerSeatIdx: number`; `next(): void`; `prev(): void`; `seekStart(): void`; `seekEnd(): void`; `jumpRound(r: number): void`; `state(): ViewState`; `atStart(): boolean`; `atEnd(): boolean`; `stepIndex(): number`; `totalSteps(): number`.
- Seat type imported from `web/src/cards/hand-manager.ts` (`'south'|'west'|'north'|'east'`).

> **Trick-winner rule (the one derived thing):** highest spade wins; else highest card of the led suit. Implement locally over app `Card` (don't import engine code). Rank order from `web/src/state/helpers.ts` (`RANK_ORDER`).

- [ ] **Step 1: Write failing controller tests**

Add to `web/tests/unit/replay-controller.spec.ts` (build a small two-round fixture `ReplayResponse` inline — one `deal` event per round with 4 `DealtHand`s, a `call` event, and `play` events; cards as `{suit,rank}` single-char):

```ts
import { ReplayController } from '../../src/replay/controller';
import type { ReplayResponse } from '../../src/replay/types';

function fixture(): ReplayResponse {
  // Minimal 1-round model: deal (4 hands of 1 card each for brevity is invalid for
  // real spades, but the controller is rule-agnostic about hand size), a call, one trick.
  return {
    model: {
      meta: { game_hint: 'spades', seats: ['N', 'E', 'S', 'W'], dealer: 'N',
              players: ['Ann', 'Bo', 'Cy', 'Di'], partnerships: [['N','S'],['E','W']], caps: [], version: 1, extra: [] },
      deck: { suits: ['S','H','D','C'], ranks: ['2','3','4','5','6','7','8','9','T','J','Q','K','A'] },
      events: [
        { type: 'deal', hands: [
          { target: 'N', cards: [{ suit: 'C', rank: 'K' }] },
          { target: 'E', cards: [{ suit: 'C', rank: '5' }] },
          { target: 'S', cards: [{ suit: 'C', rank: '2' }] },
          { target: 'W', cards: [{ suit: 'C', rank: 'T' }] },
        ] },
        { type: 'call', start: 'E', values: ['3', '4', 'nil', '4'] },
        { type: 'play', leader: 'E', cards: [
          { suit: 'C', rank: '5' }, { suit: 'C', rank: '2' },
          { suit: 'C', rank: 'T' }, { suit: 'C', rank: 'K' },
        ] },
      ],
    },
    cumulative_by_round: [[84, 56]],
    viewer_seat: 2, // S
  } as unknown as ReplayResponse;
}

describe('ReplayController', () => {
  it('starts before any move and reaches the deal/bids on step', () => {
    const c = new ReplayController(fixture());
    expect(c.atStart()).toBe(true);
    expect(c.viewerSeatIdx).toBe(2);
    const s0 = c.state();
    expect(s0.totalRounds).toBe(1);
    // viewer seat 2 (S) → south at bottom
    expect(s0.hands.south.length).toBe(1);
  });

  it('reveals bids then plays card-by-card with correct trick winner', () => {
    const c = new ReplayController(fixture());
    c.seekEnd();
    const s = c.state();
    // KC is the only/highest club → leader-relative seat that played KC wins.
    // N played KC; with viewer seat S, N is relative 'north'.
    expect(s.trickWinner).toBe('north');
    expect(s.score).toEqual([84, 56]);
    expect(c.atEnd()).toBe(true);
  });

  it('prev() undoes a step', () => {
    const c = new ReplayController(fixture());
    c.seekEnd();
    c.prev();
    expect(c.atEnd()).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test -- replay-controller`
Expected: FAIL — `ReplayController` not found.

- [ ] **Step 3: Implement the controller**

Create `web/src/replay/controller.ts`. Implement:
- Parse `model.events` into rounds: each `deal` starts a round; collect that round's `call` values and `play` tricks.
- Map every card via `tnCardToApp` once at construction (store app cards).
- Orient seats: `seatAbs` 0..3 = N,E,S,W; `rel(abs) = seatRel(abs, viewerSeatIdx)`. `viewerSeatIdx = res.viewer_seat ?? 0`.
- A linear step list: for each round, 4 bid-steps (reveal each seat's bid) then, per trick, 4 card-steps. Cursor = index into steps; `state()` derives `ViewState` for the cursor by replaying steps up to it.
- `trickWinner`: when a trick's 4 cards are all shown, compute winner via the local rule, return the *relative* seat.
- `score`: `cumulative_by_round[round-1]` for completed rounds, `[0,0]` before round 1 completes (or carry previous). `bids`: from the round's `call.values` in seat order (N,E,S,W), mapped to relative seats; `nil` → 0. `tricksWon`: count tricks won so far in the current round (derive via the rule).
- `phase`: 'bid' while revealing bids, 'play' during tricks, 'done' at end; `aborted` from `model`'s termination (read `meta.extra` for `Termination`, or accept the model has fewer plays — if the last round is partial, mark aborted only if extra says so).

Write complete, focused code (~150–200 lines). Keep it pure — no DOM, no imports from `board.ts`/`orchestrator.ts`. Import `seatRel` from `../state/helpers`, `Seat` from `../cards/hand-manager`, `RANK_ORDER` from `../state/helpers`.

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm -C web test -- replay-controller`
Expected: PASS (all controller tests).

- [ ] **Step 5: Add edge-case tests + commit**

Add tests for: `jumpRound` bounds; a nil bid renders as `0`; multi-round score progression (extend the fixture to 2 rounds). Make them pass (fix the controller if needed).

```bash
git add web/src/replay/controller.ts web/tests/unit/replay-controller.spec.ts
git commit -m "feat(web): ReplayController step-through logic + trick-winner"
```

---

### Task 3: `ReplayBoard` (DOM rendering)

**Files:**
- Create: `web/src/replay/board.ts`
- Test: `web/tests/unit/replay-board.spec.ts`

**Interfaces:**
- Consumes: `ViewState`, `Seat`, `Containers` (`web/src/cards/hand-manager.ts`), `createFront`/`setPos`/`CardEl` (`card-el.ts`), `animateTo`/`EASE` (`animation.ts`).
- Produces:
  - `class ReplayBoard { constructor(containers: Containers); render(prev: ViewState | null, next: ViewState, opts?: { animate?: boolean }): Promise<void>; clear(): void }`
  - `render` shows all four hands face-up in their seat containers, the current trick in the trick container, and animates the *delta* from `prev`→`next` when `opts.animate` and the delta is a single card play; otherwise snaps.

> **Reuse, don't reinvent:** the existing `gameTable()` (`web/src/ui/components/game-table.ts`) builds the table DOM with four seat containers + a trick container and returns refs. The route (Task 4) builds that scaffold and passes its resolved `Containers` to `ReplayBoard`. For laying out a face-up hand in a seat container, mirror the fan layout in `web/src/cards/hand-layout.ts` (read it); for the trick area, mirror how `trick-manager.ts` positions the four slots. `ReplayBoard` does NOT use `HandManager`/`TrickManager` directly (they assume one face-up hand + opponent backs); it positions `createFront(card)` elements itself so all four seats show faces.

- [ ] **Step 1: Write the failing component test**

Create `web/tests/unit/replay-board.spec.ts` (vitest + jsdom — match how other component tests set up DOM; check an existing `*.spec.ts` under `web/tests/unit/` for the jsdom container pattern):

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { ReplayBoard } from '../../src/replay/board';
import type { Containers } from '../../src/cards/hand-manager';

function makeContainers(): Containers {
  const mk = () => document.createElement('div');
  return { south: mk(), west: mk(), north: mk(), east: mk(), trick: mk() };
}

describe('ReplayBoard', () => {
  let containers: Containers;
  beforeEach(() => { containers = makeContainers(); });

  it('renders all four hands face-up', async () => {
    const board = new ReplayBoard(containers);
    const vs = /* build a ViewState with 1 card in each seat hand, empty trick */;
    await board.render(null, vs, { animate: false });
    // each seat container has its face-up cards
    expect(containers.south.querySelectorAll('.card-front').length).toBeGreaterThan(0);
    expect(containers.north.querySelectorAll('.card-front').length).toBeGreaterThan(0);
  });

  it('places trick cards in the trick container', async () => {
    const board = new ReplayBoard(containers);
    const vs = /* ViewState with 2 cards on the table */;
    await board.render(null, vs, { animate: false });
    expect(containers.trick.querySelectorAll('.card-front').length).toBe(2);
  });
});
```

Build the `ViewState` literals inline (import the type from `controller.ts`). Fill the `/* ... */` with concrete objects.

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test -- replay-board`
Expected: FAIL — `ReplayBoard` not found.

- [ ] **Step 3: Implement the board**

Create `web/src/replay/board.ts`. Implement:
- `render(prev, next, opts)`: 
  - Determine the delta. If `opts.animate !== false`, reduced-motion is off (`matchMedia('(prefers-reduced-motion: reduce)').matches === false`), and `next` differs from `prev` by exactly one newly-played card (a single seat gained one trick card and lost one hand card), animate that card flying from its hand position to the trick slot via `animateTo` (ease `'backOut'`), then reconcile the rest. Otherwise **snap**: clear and re-render all four hands + the trick from `next` with `setPos` (no animation).
  - Render each seat's hand face-up: for `seat of ['south','west','north','east']`, clear the container, create a `createFront(card)` per `next.hands[seat]` card, position them fanned (mirror `hand-layout.ts`'s spacing; opponents can use a simpler tighter fan).
  - Render the trick: for each `{seat, card}` in `next.trick`, place a `createFront(card)` in `containers.trick` offset toward that seat (mirror `trick-manager.ts` slot positions).
  - Highlight `next.trickWinner` if set (e.g. a CSS class on the winner's trick card — use a token-based style).
- `clear()`: empty all five containers.
- Honor reduced motion exactly like `orchestrator.ts:skipAnims()`.

Write complete code (~150–250 lines). Use only token-based styling (add classes; put CSS in Task 4's stylesheet step).

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm -C web test -- replay-board`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/replay/board.ts web/tests/unit/replay-board.spec.ts
git commit -m "feat(web): ReplayBoard face-up four-hand renderer"
```

---

### Task 4: `/replay/:id` route — fetch, shell, controls, wiring, error states

**Files:**
- Create: `web/src/routes/replay.ts`
- Modify: `web/src/router.ts` (register lazy route)
- Modify: `web/src/ui/design.css` (replay layout + controls styles) — see WIP note below

**Interfaces:**
- Consumes: `fetchReplay`, `ReplayController`, `ReplayBoard`, `gameTable`/`makeRefs` (`game-table.ts`), `appShell`/templates, `seatRel`.
- Produces: a `RouteModule` exported as `replay` with `render: (params, ctx) => cleanup`.

- [ ] **Step 1: Register the lazy route**

In `web/src/router.ts`, add a code-split entry for `/replay/:id` mirroring how `/play/:shortId` is registered (a `RouteLoader` that dynamically imports the module). Read the existing route registrations and copy the lazy pattern exactly.

- [ ] **Step 2: Implement the route module**

Create `web/src/routes/replay.ts` (mirror `leaderboard.ts`'s structure — signals, `appShell`, cleanup). It must:
- Read `params.id`, call `fetchReplay(id)`.
- **Error states:** on 403 → render "This game is still in progress" with a link to the live game; on 404 → the not-found view; other errors → a generic error with retry. (Use the status info `fetchReplay` surfaces.)
- On success: build the table scaffold via `gameTable({ refs: makeRefs(), ... })`, render it into the page, resolve the `Containers` from the refs (`.value`), construct `new ReplayController(res)` and `new ReplayBoard(containers)`.
- Render an initial snapshot: `board.render(null, controller.state(), { animate: false })`.
- **Controls** (token-styled, accent only on the buttons): `|<` seekStart, `<` prev, `▶/⏸` autoplay toggle, `>` next, `>|` seekEnd, and a round jump (e.g. `‹ Round n/N ›`). Each calls the controller then `board.render(prevState, controller.state(), { animate: <single-step?> })`. Animate only on single `next()`/autoplay steps; snap on prev/jump/seek.
- **Side panel:** bids per seat, tricks-won, running score (`state().score`), round n/N, and an "Aborted" marker when `state().aborted`. Nil bids show "nil".
- **Autoplay:** a `setInterval` that calls `next()` until `atEnd()`, cleared on pause/teardown.
- **Cleanup:** the returned function clears the autoplay interval and calls `board.clear()`.

Write complete code (~200 lines). Keep DOM/control rendering in lit-html; keep all game logic in the controller.

- [ ] **Step 3: Styles**

Add replay layout + control styles. **WIP note:** `web/src/ui/design.css` is in the uncommitted WIP set. To avoid entangling with that WIP, create a NEW file `web/src/replay/replay.css` and import it from `replay.ts` (Vite supports CSS imports), using only `tokens.css` variables. This keeps the replay styles in a committable file separate from the WIP'd `design.css`.

- [ ] **Step 4: Verify the route renders**

Run: `pnpm -C web test` (unit/component suite still green).
Run: `pnpm -C web build` (type-check + bundle succeeds; confirms the lazy route + imports resolve).
Manual smoke (optional): `make dev`, open `/replay/<a finished game id>`.

- [ ] **Step 5: Commit**

```bash
git add web/src/routes/replay.ts web/src/router.ts web/src/replay/replay.css
git commit -m "feat(web): /replay/:id route with playback controls + error states"
```

---

### Task 5: Entry points — profile rows + end-of-game review button

**Files:**
- Modify: `web/src/routes/profile.ts` (link game-history rows)
- Modify: `web/src/routes/game-view.ts` (Review button on terminal state)

**Interfaces:**
- Consumes: the `/replay/:id` route (navigation via the app's link/anchor convention).

- [ ] **Step 1: Link profile game rows**

In `web/src/routes/profile.ts`, the recent-games list currently renders `<code>${entry.game_id.slice(0,8)}</code>` + seat (around lines 56–64). Wrap each row in a link to `/replay/${entry.game_id}` using the app's navigation convention (an `<a href>` that navaid intercepts — check how other in-app links are written, e.g. profile/leaderboard links). Keep the existing content; just make the row navigable.

- [ ] **Step 2: Add the end-of-game Review button**

In `web/src/routes/game-view.ts`, when the game has reached a terminal state (Completed/Aborted — find where the end-of-game UI is rendered), add a "Review game" button/link to `/replay/${gameId}`. Use the game's id available in that route's scope. Token-styled, accent (it's interactive).

- [ ] **Step 3: Verify**

Run: `pnpm -C web test` (green).
Run: `pnpm -C web build` (succeeds).

- [ ] **Step 4: Commit**

```bash
git add web/src/routes/profile.ts web/src/routes/game-view.ts
git commit -m "feat(web): link replays from profile rows + end-of-game review"
```

---

### Task 6: e2e + full gate

**Files:**
- Create: `web/tests/e2e/replay.spec.ts` (or extend an existing e2e spec)

- [ ] **Step 1: Write the e2e**

Create `web/tests/e2e/replay.spec.ts` mirroring the existing Playwright specs' setup (they auto-start the backend via the Playwright config). The flow:
- Drive a vs-AI game to completion (reuse whatever helper/flow existing e2e tests use to play a game; if none plays to completion, navigate directly to `/replay/<id>` for a known-finished game created via the API in the test setup).
- Assert the replay page renders four hands, step forward with the `>` control, and the score/round indicator updates.
- Assert the in-progress case: hitting `/replay/<an in-progress game id>` shows the "still in progress" message.

Keep it focused; match the existing e2e conventions for selectors and backend setup.

- [ ] **Step 2: Run e2e**

Run: `pnpm -C web test:e2e -- replay`
Expected: PASS. (One-time: `pnpm -C web exec playwright install chromium` if not installed.)

- [ ] **Step 3: Full gate**

Run: `pnpm -C web lint` → clean.
Run: `pnpm -C web test` → all unit/component pass.
Run: `make check` → green (note: this also runs the Rust suite; nothing Rust changed, so it should pass as before). If `make check` web tests fail due to the unrelated WIP, report it (do not fix WIP).

- [ ] **Step 4: Commit**

```bash
git add web/tests/e2e/replay.spec.ts
git commit -m "test(web): e2e for replay viewer"
```

---

## Self-Review notes (for the implementer)

- **Spec coverage:** Section 2 (`ReplayController` Task 2, `ReplayBoard` Task 3 — reuses primitives, four face-up hands, viewer-seat orientation via `seatRel`, animate-delta-or-snap, reduced-motion) and Section 3 (route Task 4, entry points Task 5, edge cases 403/404/aborted/nil in Tasks 2+4, tests across 2/3/6).
- **Deferred follow-up from the server plan:** make `get_replay_json` tolerate a present-but-invalid Bearer token (anon fallback → `viewer_seat: null`) for full public-share parity. The viewer uses cookie/no-cred auth so it's not blocking; if you have spare cycles, fix it in `crates/spades-server/.../handlers/games.rs` and add a `Vec<Vec<i32>>`-vs-`[i32;2]` note — otherwise leave it for a server follow-up.
- **Schema looseness to watch:** the generated `Card` schema is `{suit, rank}` (no `kind`/special) and `cumulative_by_round` is `Vec<Vec<i32>>`. Spades replays are all suited cards, and inner score arrays are always length 2 — the controller/board can rely on that. `Meta.extra` is absent from the generated types (oasgen-skipped); read `Termination` from it via a loose cast if needed for the aborted marker, or infer aborted from a short final round.
- **Card-shape gotcha:** replay cards are single-char syms — ALWAYS run them through `tnCardToApp` before `createFront`. Never pass a raw replay card to a primitive.
- **WIP isolation:** replay styles go in a NEW `web/src/replay/replay.css`, NOT the WIP'd `design.css`. Never stage the five WIP files.
- **Type consistency:** `ViewState`, `ReplayController` methods (`next/prev/seekStart/seekEnd/jumpRound/state/atStart/atEnd`), `ReplayBoard.render(prev, next, opts)`, `tnCardToApp`, `fetchReplay` are used consistently across Tasks 1–6.
```
