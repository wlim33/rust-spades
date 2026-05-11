# spades-ts — Plan 2: Gameplay Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reach functional parity with the current `personal-site/spades` Alpine client — Quick Play, Play with Computers, Play with Friends (challenge create + join + lobby), full in-game flow (betting / trick play / collect / game-over), URL-based reconnect, and drag-to-play with animations. **Anonymous play only.** Account-aware UX is Plan 3.

**Architecture:** Two source-of-truth layers: a `gameStore` of signals (state) and an `orchestrator` (DOM/animation) — separate concerns, glued by `routes/play.ts`. REST via `openapi-fetch`, SSE/WS via small typed helpers. Pure state helpers fully unit-tested; card layer covered by component tests for what's testable in happy-dom and by Playwright for animation correctness.

**Tech Stack:** Built on Plan 1 scaffold. New deps: `openapi-fetch`, `openapi-typescript` (dev).

**Reference spec:** `/Users/wlim/Projects/spades-ts/docs/superpowers/specs/2026-05-11-spades-ts-design.md`
**Reference port source:** `/Users/wlim/Projects/personal-site/spades/index.html` (Alpine component), `/Users/wlim/Projects/personal-site/spades/js/card-manager.js`

**Prereq (handled by user, outside this plan):** rust-spades is patched so `/openapi.json` covers all routes the frontend uses (auth endpoints excluded — those belong to Plan 3).

---

## Files this plan creates or modifies

| Path                                    | Action   | Responsibility                                              |
| --------------------------------------- | -------- | ----------------------------------------------------------- |
| `openapi/openapi.json`                  | create   | Committed snapshot                                          |
| `openapi/fetch.sh`                      | create   | Refresh snapshot from a running server                      |
| `openapi/generate.sh`                   | create   | Run openapi-typescript → `src/api/schema.d.ts`              |
| `src/api/schema.d.ts`                   | generate | Generated; committed                                        |
| `src/api/client.ts`                     | create   | openapi-fetch instance + `ApiError`                         |
| `src/api/sse.ts`                        | create   | SSE helper                                                  |
| `src/api/ws.ts`                         | create   | WS helper                                                   |
| `src/api/hand-written.ts`               | create   | Empty after Phase 0; reserved for future use                |
| `src/state/helpers.ts`                  | create   | Pure card/game helpers                                      |
| `src/state/game.ts`                     | create   | `createGameStore()`                                         |
| `src/state/menu.ts`                     | create   | Queue sizes signal + poller                                 |
| `src/cards/card-el.ts`                  | create   | DOM factories                                               |
| `src/cards/animation.ts`                | create   | `animateTo`, easings                                        |
| `src/cards/hand-manager.ts`             | create   | Hand DOM subtrees                                           |
| `src/cards/trick-manager.ts`            | create   | Trick slots                                                 |
| `src/cards/drag.ts`                     | create   | Pointer-based drag                                          |
| `src/cards/orchestrator.ts`             | create   | Public surface, composes the above                          |
| `src/lib/storage.ts`                    | create   | Game-session localStorage                                   |
| `src/routes/play.ts`                    | create   | Lobby + in-game + game-over                                 |
| `src/routes/home.ts`                    | modify   | Replace `console.log` with real wiring                      |
| `src/main.ts`                           | modify   | Register `/play/:shortId` route                             |
| `src/ui/design.css`                     | modify   | Game-table layout + card classes                            |
| `package.json`                          | modify   | Add `openapi-fetch`, `openapi-typescript`, generate scripts |
| `tests/unit/helpers.spec.ts`            | create   | All pure helpers                                            |
| `tests/unit/sse.spec.ts`                | create   | Parser                                                      |
| `tests/unit/game-store.spec.ts`         | create   | applyState + applyWsEvent against fixtures                  |
| `tests/unit/api-client.spec.ts`         | create   | ApiError wrapping                                           |
| `tests/component/card-el.spec.ts`       | create   | DOM factories                                               |
| `tests/component/hand-manager.spec.ts`  | create   | Mount/replace/unmount                                       |
| `tests/component/trick-manager.spec.ts` | create   | Slot fill                                                   |
| `tests/component/drag.spec.ts`          | create   | Pointer events                                              |
| `tests/e2e/ai-game.spec.ts`             | create   | Anonymous AI happy path + reload reconnect                  |
| `tests/e2e/quickplay.spec.ts`           | create   | 4-context quickplay match                                   |
| `tests/e2e/friends.spec.ts`             | create   | Create challenge + 3 joins                                  |
| `tests/fixtures/ws-events/*.json`       | create   | Recorded WS payloads for game-store tests                   |

---

## Task 1: Add openapi tooling and generate schema

**Files:**

- Modify: `package.json`
- Create: `openapi/fetch.sh`, `openapi/generate.sh`, `openapi/openapi.json` (committed snapshot), `src/api/schema.d.ts` (generated, committed), `src/api/hand-written.ts`

- [ ] **Step 1: Install openapi tooling**

Run: `pnpm add openapi-fetch && pnpm add -D openapi-typescript`
Expected: both added; lockfile updated.

- [ ] **Step 2: Add scripts to `package.json`**

Replace the `"scripts"` block with:

```json
{
  "dev": "vite",
  "build": "tsc -p tsconfig.json --noEmit && vite build",
  "preview": "vite preview",
  "lint": "eslint . --max-warnings=0",
  "format": "prettier --write .",
  "format:check": "prettier --check .",
  "test:unit": "vitest run --project=unit",
  "test:component": "vitest run --project=component",
  "test:watch": "vitest",
  "test:e2e": "playwright test",
  "test": "pnpm test:unit && pnpm test:component",
  "openapi:fetch": "bash openapi/fetch.sh",
  "openapi:generate": "bash openapi/generate.sh",
  "openapi:check": "bash openapi/generate.sh && git diff --exit-code src/api/schema.d.ts"
}
```

- [ ] **Step 3: Create `openapi/fetch.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
URL="${VITE_API_URL:-http://localhost:3000}/openapi.json"
echo "Fetching $URL"
curl -fsSL "$URL" | python3 -m json.tool > openapi/openapi.json
echo "Wrote openapi/openapi.json"
```

Make executable: `chmod +x openapi/fetch.sh`.

- [ ] **Step 4: Create `openapi/generate.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
pnpm exec openapi-typescript openapi/openapi.json -o src/api/schema.d.ts
```

Make executable: `chmod +x openapi/generate.sh`.

- [ ] **Step 5: Fetch the schema from a running server**

The user must have rust-spades running on `http://localhost:3000` with the Phase 0 oasgen patch applied.

Run: `pnpm openapi:fetch`
Expected: `openapi/openapi.json` written.

If rust-spades isn't running, abort here and resolve before continuing. Do NOT hand-craft `openapi.json`.

- [ ] **Step 6: Generate `src/api/schema.d.ts`**

Run: `pnpm openapi:generate`
Expected: `src/api/schema.d.ts` written; should compile cleanly.

- [ ] **Step 7: Create `src/api/hand-written.ts`**

```ts
// Reserved for endpoint types that aren't covered by /openapi.json yet.
// Should be empty after the Phase 0 server-side oasgen patch is in.
export {};
```

- [ ] **Step 8: Commit**

```bash
git add openapi/ src/api/ package.json pnpm-lock.yaml
git commit -m "build: add openapi tooling and generated schema"
```

---

## Task 2: API client (openapi-fetch wrapper + ApiError)

**Files:**

- Create: `src/api/client.ts`, `tests/unit/api-client.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/unit/api-client.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ApiError, request } from '../../src/api/client';

describe('api client', () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it('throws ApiError on 4xx with parsed JSON message', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: 'bad name' }), {
            status: 400,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    await expect(request('/games/foo', { method: 'GET' })).rejects.toMatchObject({
      status: 400,
      message: 'bad name',
    });
  });

  it('throws ApiError on 5xx with statusText fallback', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response('boom', {
            status: 503,
            statusText: 'Service Unavailable',
          }),
      ),
    );
    await expect(request('/games/foo', { method: 'GET' })).rejects.toMatchObject({
      status: 503,
      message: 'Service Unavailable',
    });
  });

  it('returns parsed JSON on 2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ ok: true }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    const data = await request<{ ok: boolean }>('/games/foo', { method: 'GET' });
    expect(data).toEqual({ ok: true });
  });

  it('sends credentials: include', async () => {
    const spy = vi.fn(
      async () =>
        new Response('null', { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', spy);
    await request('/foo', { method: 'GET' });
    const init = spy.mock.calls[0]![1] as RequestInit;
    expect(init.credentials).toBe('include');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL — `src/api/client.ts` not found.

- [ ] **Step 3: Implement `src/api/client.ts`**

```ts
import createClient from 'openapi-fetch';
import type { paths } from './schema';
import { API_URL } from '../lib/util';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export const api = createClient<paths>({
  baseUrl: API_URL,
  credentials: 'include',
});

/**
 * Low-level request used by SSE/WS helpers and for endpoints not yet typed.
 * Routes that have generated types should use `api.GET/POST/...` directly.
 */
export async function request<T>(path: string, init: RequestInit): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(init.headers ?? {}),
    },
  });
  if (!res.ok) {
    let message = res.statusText;
    try {
      const body = (await res.clone().json()) as { error?: string; message?: string };
      message = body.error ?? body.message ?? message;
    } catch {
      // body wasn't JSON; keep statusText
    }
    throw new ApiError(res.status, message);
  }
  const contentType = res.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return (await res.json()) as T;
  }
  return undefined as T;
}
```

Note: import name `schema` matches `src/api/schema.d.ts`. The generated file exports `paths`.

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:unit`
Expected: 4 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/api/client.ts tests/unit/api-client.spec.ts
git commit -m "feat: api client with ApiError + credentials:include"
```

---

## Task 3: SSE helper

**Files:**

- Create: `src/api/sse.ts`, `tests/unit/sse.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/unit/sse.spec.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { openSse } from '../../src/api/sse';

function makeStreamingResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    async start(controller) {
      for (const c of chunks) {
        controller.enqueue(encoder.encode(c));
        await new Promise((r) => setTimeout(r, 0));
      }
      controller.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

describe('openSse', () => {
  beforeEach(() => vi.unstubAllGlobals());

  it('parses event + data pairs across chunk boundaries', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        makeStreamingResponse([
          'event: queue_status\ndata: {"waiti',
          'ng":2}\n\nevent: game_start\ndata: {"game_id":"abc"}\n\n',
        ]),
      ),
    );

    const events: Array<{ type: string; data: string }> = [];
    await new Promise<void>((resolve) => {
      const sse = openSse('/matchmaking/seek', undefined, {
        onEvent: (type, data) => {
          events.push({ type, data });
          if (events.length === 2) resolve();
        },
      });
      // eventually
      setTimeout(() => sse.close(), 1000);
    });

    expect(events).toEqual([
      { type: 'queue_status', data: '{"waiting":2}' },
      { type: 'game_start', data: '{"game_id":"abc"}' },
    ]);
  });

  it('close() is idempotent and suppresses AbortError', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url, init) => {
        const signal = (init as RequestInit).signal as AbortSignal;
        const stream = new ReadableStream<Uint8Array>({
          start(controller) {
            signal.addEventListener('abort', () =>
              controller.error(new DOMException('aborted', 'AbortError')),
            );
          },
        });
        return new Response(stream, {
          status: 200,
          headers: { 'content-type': 'text/event-stream' },
        });
      }),
    );

    const errors: unknown[] = [];
    const sse = openSse('/x', undefined, {
      onEvent: () => {},
      onError: (e) => errors.push(e),
    });
    sse.close();
    sse.close();
    await new Promise((r) => setTimeout(r, 10));
    expect(errors.length).toBe(0);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL — `src/api/sse.ts` not found.

- [ ] **Step 3: Implement `src/api/sse.ts`**

```ts
import { API_URL } from '../lib/util';

export type SseHandle = { close(): void };

export type SseOptions = {
  method?: 'GET' | 'POST';
  onEvent: (type: string, data: string) => void;
  onError?: (err: unknown) => void;
};

export function openSse<Body>(path: string, body: Body | undefined, opts: SseOptions): SseHandle {
  const controller = new AbortController();
  let closed = false;

  const close = (): void => {
    if (closed) return;
    closed = true;
    controller.abort();
  };

  void (async () => {
    try {
      const res = await fetch(`${API_URL}${path}`, {
        method: opts.method ?? 'POST',
        signal: controller.signal,
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (!res.ok || !res.body) {
        throw new Error(`SSE ${path}: ${res.status} ${res.statusText}`);
      }
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let eventType: string | null = null;
      // eslint-disable-next-line no-constant-condition
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          if (line.startsWith('event:')) {
            eventType = line.slice(6).trim();
          } else if (line.startsWith('data:') && eventType) {
            opts.onEvent(eventType, line.slice(5).trim());
            eventType = null;
          } else if (line === '') {
            eventType = null;
          }
        }
      }
    } catch (e) {
      if (closed) return;
      if (e instanceof DOMException && e.name === 'AbortError') return;
      opts.onError?.(e);
    }
  })();

  return { close };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:unit`
Expected: both cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/api/sse.ts tests/unit/sse.spec.ts
git commit -m "feat: typed SSE helper"
```

---

## Task 4: WS helper

**Files:**

- Create: `src/api/ws.ts`

No unit test — WebSocket isn't ergonomic to test in happy-dom. Behavior is covered by E2E tests in later tasks. The wrapper is small enough that the type contract carries most of the weight.

- [ ] **Step 1: Implement `src/api/ws.ts`**

```ts
import { API_URL } from '../lib/util';

export type WsHandle = { close(): void };

export type WsOptions = {
  onEvent: (data: unknown) => void;
  onOpen?: () => void;
  onClose?: () => void;
  onError?: (e: unknown) => void;
};

/**
 * Connects to /games/:gameId/ws?player_id=:playerId. Caller is responsible
 * for fallback (e.g. polling) on `onClose` — this helper does not auto-reconnect.
 *
 * Maintains an internal async queue so consumers can `await` per-event work
 * without dropping subsequent messages.
 */
export function openGameWs(gameId: string, playerId: string | null, opts: WsOptions): WsHandle {
  const wsUrl = `${API_URL.replace(/^https/, 'wss').replace(/^http/, 'ws')}/games/${encodeURIComponent(
    gameId,
  )}/ws${playerId ? `?player_id=${encodeURIComponent(playerId)}` : ''}`;

  const ws = new WebSocket(wsUrl);
  const queue: unknown[] = [];
  let draining = false;
  let closed = false;

  const drain = async (): Promise<void> => {
    if (draining) return;
    draining = true;
    try {
      while (queue.length > 0) {
        const data = queue.shift();
        try {
          await opts.onEvent(data);
        } catch (e) {
          opts.onError?.(e);
        }
      }
    } finally {
      draining = false;
    }
  };

  ws.onmessage = (evt) => {
    try {
      const data = JSON.parse(evt.data as string);
      queue.push(data);
      void drain();
    } catch (e) {
      opts.onError?.(e);
    }
  };
  ws.onopen = () => opts.onOpen?.();
  ws.onclose = () => {
    if (closed) return;
    opts.onClose?.();
  };
  ws.onerror = (e) => opts.onError?.(e);

  return {
    close: () => {
      if (closed) return;
      closed = true;
      try {
        ws.close();
      } catch {
        // already closed
      }
    },
  };
}
```

- [ ] **Step 2: Type-check passes**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/api/ws.ts
git commit -m "feat: typed WS helper with serialized event queue"
```

---

## Task 5: Pure state helpers

**Files:**

- Create: `src/state/helpers.ts`, `tests/unit/helpers.spec.ts`

These match `personal-site/spades/index.html` lines 308-326 (`cardLabel`, `formatClock`, `sortCards`) and 451-705 (`getLeadSuit`, `isCardValid`, `seatName`, `oppCardCount`, etc.).

- [ ] **Step 1: Write failing tests**

`tests/unit/helpers.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import {
  type Card,
  type Suit,
  cardEq,
  sortCards,
  seatRel,
  formatClock,
  getLeadSuit,
  isCardValid,
  oppCardCount,
} from '../../src/state/helpers';

const c = (suit: Suit, rank: Card['rank']): Card => ({ suit, rank });

describe('cardEq', () => {
  it('matches identical cards', () => {
    expect(cardEq(c('Spade', 'Ace'), c('Spade', 'Ace'))).toBe(true);
  });
  it('rejects differing rank', () => {
    expect(cardEq(c('Spade', 'Ace'), c('Spade', 'King'))).toBe(false);
  });
  it('handles null inputs', () => {
    expect(cardEq(null, c('Spade', 'Ace'))).toBe(false);
    expect(cardEq(null, null)).toBe(false);
  });
});

describe('sortCards', () => {
  it('groups by suit (Spade, Heart, Diamond, Club) and high rank first', () => {
    const hand: Card[] = [
      c('Club', 'Two'),
      c('Spade', 'Three'),
      c('Heart', 'King'),
      c('Spade', 'Ace'),
    ];
    expect(sortCards(hand)).toEqual([
      c('Spade', 'Ace'),
      c('Spade', 'Three'),
      c('Heart', 'King'),
      c('Club', 'Two'),
    ]);
  });
  it('does not mutate input', () => {
    const hand: Card[] = [c('Club', 'Two'), c('Spade', 'Ace')];
    const copy = [...hand];
    sortCards(hand);
    expect(hand).toEqual(copy);
  });
});

describe('seatRel', () => {
  it('south for self', () => expect(seatRel(2, 2)).toBe('south'));
  it('east for +1', () => expect(seatRel(3, 2)).toBe('east'));
  it('north for +2', () => expect(seatRel(0, 2)).toBe('north'));
  it('west for +3', () => expect(seatRel(1, 2)).toBe('west'));
});

describe('formatClock', () => {
  it('null is --:--', () => expect(formatClock(null)).toBe('--:--'));
  it('formats m:ss', () => expect(formatClock(65_000)).toBe('1:05'));
  it('rounds up sub-second', () => expect(formatClock(500)).toBe('0:01'));
  it('floors negative to 0:00', () => expect(formatClock(-1000)).toBe('0:00'));
});

describe('getLeadSuit', () => {
  it('returns null when table is empty', () => {
    const tc: (Card | null)[] = [null, null, null, null];
    expect(getLeadSuit(tc, 0)).toBe(null);
  });
  it('returns the leader suit', () => {
    // 2 cards on the table; current player is at seat 0; leader was 2 seats back = seat 2
    const tc: (Card | null)[] = [c('Heart', 'Ace'), null, c('Heart', 'Five'), null];
    expect(getLeadSuit(tc, 0)).toBe('Heart');
  });
});

describe('isCardValid', () => {
  const hand = (cards: Card[]) => cards;
  it('always valid when not your turn', () => {
    expect(
      isCardValid({
        hand: hand([c('Heart', 'Ace')]),
        leadSuit: null,
        spadesBroken: false,
        card: c('Heart', 'Ace'),
        isMyTurn: false,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
  it('always valid in BETTING', () => {
    expect(
      isCardValid({
        hand: [c('Heart', 'Ace')],
        leadSuit: null,
        spadesBroken: false,
        card: c('Heart', 'Ace'),
        isMyTurn: true,
        phase: 'BETTING',
      }),
    ).toBe(true);
  });
  it('must follow lead suit if held', () => {
    const myHand: Card[] = [c('Heart', 'Two'), c('Spade', 'Ace')];
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: 'Heart',
        spadesBroken: true,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(false);
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: 'Heart',
        spadesBroken: true,
        card: c('Heart', 'Two'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
  it('any suit if void in lead suit', () => {
    const myHand: Card[] = [c('Diamond', 'Two')];
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: 'Heart',
        spadesBroken: false,
        card: c('Diamond', 'Two'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
  it('cannot lead spade unless broken or hand is spades-only', () => {
    const myHand: Card[] = [c('Spade', 'Ace'), c('Heart', 'Two')];
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: null,
        spadesBroken: false,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(false);
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: null,
        spadesBroken: true,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
    expect(
      isCardValid({
        hand: [c('Spade', 'Ace')],
        leadSuit: null,
        spadesBroken: false,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
});

describe('oppCardCount', () => {
  it('returns 13 during BETTING', () => {
    expect(oppCardCount('BETTING', null, [null, null, null, null], 1)).toBe(13);
  });
  it('returns 0 outside PLAYING/BETTING', () => {
    expect(oppCardCount('MENU', null, [null, null, null, null], 1)).toBe(0);
  });
  it('decrements for played card at the seat', () => {
    expect(oppCardCount('PLAYING', { Trick: 3 }, [null, c('Spade', 'Two'), null, null], 1)).toBe(
      13 - 3 - 1,
    );
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL — `src/state/helpers.ts` not found.

- [ ] **Step 3: Implement `src/state/helpers.ts`**

```ts
export type Suit = 'Spade' | 'Heart' | 'Diamond' | 'Club';
export type Rank =
  | 'Two'
  | 'Three'
  | 'Four'
  | 'Five'
  | 'Six'
  | 'Seven'
  | 'Eight'
  | 'Nine'
  | 'Ten'
  | 'Jack'
  | 'Queen'
  | 'King'
  | 'Ace';

export type Card = { suit: Suit; rank: Rank };

export type Phase = 'MENU' | 'CREATE' | 'WAITING' | 'LOBBY' | 'BETTING' | 'PLAYING' | 'GAME_OVER';

const SUIT_ORDER: Suit[] = ['Spade', 'Heart', 'Diamond', 'Club'];
const RANK_ORDER: Rank[] = [
  'Two',
  'Three',
  'Four',
  'Five',
  'Six',
  'Seven',
  'Eight',
  'Nine',
  'Ten',
  'Jack',
  'Queen',
  'King',
  'Ace',
];

export function cardEq(a: Card | null | undefined, b: Card | null | undefined): boolean {
  if (!a || !b) return false;
  return a.suit === b.suit && a.rank === b.rank;
}

export function sortCards(cards: readonly Card[]): Card[] {
  return [...cards].sort((a, b) => {
    const si = SUIT_ORDER.indexOf(a.suit) - SUIT_ORDER.indexOf(b.suit);
    if (si !== 0) return si;
    return RANK_ORDER.indexOf(b.rank) - RANK_ORDER.indexOf(a.rank);
  });
}

export type RelativeSeat = 'south' | 'east' | 'north' | 'west';

export function seatRel(absIdx: number, myIdx: number): RelativeSeat {
  const rel = (((absIdx - myIdx) % 4) + 4) % 4;
  return (['south', 'east', 'north', 'west'] as const)[rel]!;
}

export function formatClock(ms: number | null | undefined): string {
  if (ms == null) return '--:--';
  const totalSec = Math.max(0, Math.ceil(ms / 1000));
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s < 10 ? '0' : ''}${s}`;
}

/**
 * Lead suit derived from the table.
 *
 * The current player's seat (`currentPlayerSeatIdx`) is *next to play*; the
 * leader sat `n` seats before, where `n` is the number of cards already on
 * the table.
 */
export function getLeadSuit(
  tableCards: readonly (Card | null)[],
  currentPlayerSeatIdx: number,
): Suit | null {
  let n = 0;
  for (const c of tableCards) {
    if (c && (c as { suit?: string }).suit !== 'Blank') n++;
  }
  if (n === 0) return null;
  const leaderSeat = (((currentPlayerSeatIdx - n) % 4) + 4) % 4;
  const leadCard = tableCards[leaderSeat];
  return leadCard ? leadCard.suit : null;
}

export function isCardValid(args: {
  hand: readonly Card[];
  leadSuit: Suit | null;
  spadesBroken: boolean;
  card: Card;
  isMyTurn: boolean;
  phase: Phase;
}): boolean {
  if (!args.isMyTurn || args.phase !== 'PLAYING') return true;
  if (args.leadSuit) {
    if (args.hand.some((c) => c.suit === args.leadSuit)) {
      return args.card.suit === args.leadSuit;
    }
    return true;
  }
  if (args.spadesBroken) return true;
  if (args.hand.every((c) => c.suit === 'Spade')) return true;
  return args.card.suit !== 'Spade';
}

type GameStateValue = string | { Betting?: number; Trick?: number; Completed?: unknown };

export function oppCardCount(
  phase: Phase,
  gameState: GameStateValue | null,
  tableCards: readonly (Card | null)[],
  seatIdx: number,
): number {
  if (phase === 'BETTING') return 13;
  if (phase !== 'PLAYING') return 0;
  const trickNum =
    typeof gameState === 'object' && gameState !== null && 'Trick' in gameState
      ? (gameState.Trick as number)
      : 0;
  let count = 13 - trickNum;
  const tc = tableCards[seatIdx];
  if (tc && (tc as { suit?: string }).suit !== 'Blank') count--;
  return Math.max(0, count);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:unit`
Expected: all helper cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/state/helpers.ts tests/unit/helpers.spec.ts
git commit -m "feat: pure state helpers (sort, validate, lead suit, etc.)"
```

---

## Task 6: Card DOM factories + animation primitive

**Files:**

- Create: `src/cards/card-el.ts`, `src/cards/animation.ts`, `tests/component/card-el.spec.ts`, `tests/unit/animation.spec.ts`

- [ ] **Step 1: Write failing component test for card-el**

`tests/component/card-el.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { createFront, createBack, setFront } from '../../src/cards/card-el';

describe('card-el', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders a red heart Ace with red color class', () => {
    const el = createFront({ suit: 'Heart', rank: 'Ace' });
    expect(el.className).toContain('card-red');
    expect(el.textContent).toBe('A♥');
  });

  it('renders a black spade 10 with black color class', () => {
    const el = createFront({ suit: 'Spade', rank: 'Ten' });
    expect(el.className).toContain('card-black');
    expect(el.textContent).toBe('10♠');
  });

  it('back card has card-back class and no text', () => {
    const el = createBack();
    expect(el.className).toContain('card-back');
    expect(el.textContent).toBe('');
  });

  it('setFront mutates an existing element', () => {
    const el = createBack();
    setFront(el, { suit: 'Diamond', rank: 'Two' });
    expect(el.className).toContain('card-red');
    expect(el.textContent).toBe('2♦');
  });
});
```

- [ ] **Step 2: Write failing unit test for animation**

`tests/unit/animation.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { EASE } from '../../src/cards/animation';

describe('easings', () => {
  it('linear is identity at endpoints', () => {
    expect(EASE.linear(0)).toBe(0);
    expect(EASE.linear(1)).toBe(1);
  });
  it('quartOut at 0 is 0', () => {
    expect(EASE.quartOut(0)).toBe(0);
  });
  it('quartOut at 1 is 1', () => {
    expect(EASE.quartOut(1)).toBe(1);
  });
  it('quartIn at 0.5 is 0.0625', () => {
    expect(EASE.quartIn(0.5)).toBeCloseTo(0.0625, 5);
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `pnpm test`
Expected: FAIL — modules not found.

- [ ] **Step 4: Implement `src/cards/card-el.ts`**

```ts
import type { Card } from '../state/helpers';

const SUIT_SYMBOL = { Spade: '♠', Heart: '♥', Diamond: '♦', Club: '♣' } as const;
const RANK_DISPLAY = {
  Two: '2',
  Three: '3',
  Four: '4',
  Five: '5',
  Six: '6',
  Seven: '7',
  Eight: '8',
  Nine: '9',
  Ten: '10',
  Jack: 'J',
  Queen: 'Q',
  King: 'K',
  Ace: 'A',
} as const;
const SUIT_COLOR = { Spade: 'black', Heart: 'red', Diamond: 'red', Club: 'black' } as const;

export type CardPos = { x: number; y: number };
export type CardEl = HTMLDivElement & { _cm: CardPos };

export function cardText(card: Card): string {
  return RANK_DISPLAY[card.rank] + SUIT_SYMBOL[card.suit];
}

export function createFront(card: Card): CardEl {
  const el = document.createElement('div') as CardEl;
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
  el._cm = { x: 0, y: 0 };
  return el;
}

export function createBack(): CardEl {
  const el = document.createElement('div') as CardEl;
  el.className = 'card card-back';
  el._cm = { x: 0, y: 0 };
  return el;
}

export function setFront(el: CardEl, card: Card): void {
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
}

export function setPos(el: CardEl, x: number, y: number): void {
  el._cm.x = x;
  el._cm.y = y;
  el.style.transform = `translate(${x}px, ${y}px)`;
}
```

- [ ] **Step 5: Implement `src/cards/animation.ts`**

```ts
import { setPos, type CardEl } from './card-el';

export type EaseFn = (t: number) => number;

export const EASE: Record<'linear' | 'quartIn' | 'quartOut', EaseFn> = {
  linear: (t) => t,
  quartOut: (t) => {
    const u = t - 1;
    return 1 - u * u * u * u;
  },
  quartIn: (t) => t * t * t * t,
};

export type AnimateOpts = {
  x: number;
  y: number;
  duration?: number;
  delay?: number;
  ease?: keyof typeof EASE;
  onStart?: () => void;
  onProgress?: (raw: number, eased: number) => void;
  onComplete?: () => void;
};

export function animateTo(el: CardEl, opts: AnimateOpts): Promise<void> {
  return new Promise((resolve) => {
    const startX = el._cm.x;
    const startY = el._cm.y;
    const easeFn = EASE[opts.ease ?? 'quartOut'];
    const duration = opts.duration ?? 300;
    const run = (): void => {
      const startTime = performance.now();
      opts.onStart?.();
      const tick = (now: number): void => {
        const elapsed = now - startTime;
        const raw = Math.min(elapsed / duration, 1);
        const t = easeFn(raw);
        const cx = startX + (opts.x - startX) * t;
        const cy = startY + (opts.y - startY) * t;
        setPos(el, cx, cy);
        opts.onProgress?.(raw, t);
        if (raw < 1) requestAnimationFrame(tick);
        else {
          opts.onComplete?.();
          resolve();
        }
      };
      requestAnimationFrame(tick);
    };
    if (opts.delay && opts.delay > 0) setTimeout(run, opts.delay);
    else run();
  });
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `pnpm test`
Expected: card-el (4 cases) and animation easings (4 cases) all pass.

- [ ] **Step 7: Commit**

```bash
git add src/cards/card-el.ts src/cards/animation.ts tests/component/card-el.spec.ts tests/unit/animation.spec.ts
git commit -m "feat: card DOM factories + animate-to primitive"
```

---

## Task 7: HandManager

**Files:**

- Create: `src/cards/hand-manager.ts`, `tests/component/hand-manager.spec.ts`

This is the south/north/east/west hand DOM ownership. Owns mount/replace/unmount; does not animate. Source reference: `card-manager.js` lines 193-242 + 572-595 (initial setup).

- [ ] **Step 1: Write failing component test**

`tests/component/hand-manager.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { HandManager } from '../../src/cards/hand-manager';
import type { Card } from '../../src/state/helpers';

const c = (suit: Card['suit'], rank: Card['rank']): Card => ({ suit, rank });

describe('HandManager', () => {
  let south: HTMLDivElement;
  let north: HTMLDivElement;
  let hm: HandManager;

  beforeEach(() => {
    document.body.innerHTML = '<div id="s"></div><div id="n"></div>';
    south = document.getElementById('s') as HTMLDivElement;
    north = document.getElementById('n') as HTMLDivElement;
    hm = new HandManager();
    hm.setContainers({ south, north, west: south, east: south, trick: south });
  });

  it('mounts the player hand sorted by suit/rank', () => {
    hm.setPlayerHand([c('Heart', 'Two'), c('Spade', 'Ace')]);
    expect(south.children.length).toBe(2);
    expect(south.children[0]!.textContent).toBe('A♠');
    expect(south.children[1]!.textContent).toBe('2♥');
  });

  it('replacing the hand reuses kept entries and unmounts removed', () => {
    hm.setPlayerHand([c('Heart', 'Two'), c('Spade', 'Ace')]);
    const aceEl = south.children[0];
    hm.setPlayerHand([c('Spade', 'Ace')]);
    expect(south.children.length).toBe(1);
    expect(south.children[0]).toBe(aceEl); // same DOM node reused
  });

  it('mounts opponent backs by count', () => {
    hm.setOpponentCount('north', 13);
    expect(north.children.length).toBe(13);
    expect(north.children[0]!.className).toContain('card-back');
  });

  it('reducing opponent count unmounts the tail', () => {
    hm.setOpponentCount('north', 3);
    hm.setOpponentCount('north', 1);
    expect(north.children.length).toBe(1);
  });

  it('removeCard removes a specific player card and returns its element', () => {
    hm.setPlayerHand([c('Heart', 'Two'), c('Spade', 'Ace')]);
    const removed = hm.removeCard(c('Spade', 'Ace'));
    expect(removed?.textContent).toBe('A♠');
    expect(south.children.length).toBe(1);
  });

  it('clear empties everything', () => {
    hm.setPlayerHand([c('Spade', 'Ace')]);
    hm.setOpponentCount('north', 3);
    hm.clear();
    expect(south.children.length).toBe(0);
    expect(north.children.length).toBe(0);
  });

  it('cards() returns the live entries for the player', () => {
    hm.setPlayerHand([c('Spade', 'Ace')]);
    const entries = hm.cards('south');
    expect(entries.length).toBe(1);
    expect(entries[0]!.card).toEqual(c('Spade', 'Ace'));
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:component`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/cards/hand-manager.ts`**

```ts
import type { Card } from '../state/helpers';
import { createBack, createFront, type CardEl } from './card-el';
import { sortCards, cardEq } from '../state/helpers';

export type Seat = 'south' | 'north' | 'east' | 'west';
export type Containers = Record<Seat | 'trick', HTMLElement>;

export type HandEntry = { card: Card | null; el: CardEl };

export class HandManager {
  private containers: Containers | null = null;
  private hands: Record<Seat, HandEntry[]> = { south: [], north: [], east: [], west: [] };

  setContainers(containers: Containers): void {
    this.containers = containers;
  }

  setPlayerHand(cards: readonly Card[]): void {
    if (!this.containers) return;
    const sorted = sortCards(cards);
    const existing = this.hands.south;
    const kept: HandEntry[] = [];

    // Unmount any cards no longer in the hand
    for (const entry of existing) {
      if (!sorted.some((c) => cardEq(c, entry.card))) {
        entry.el.remove();
      }
    }

    // Build the new ordered list, reusing nodes when possible
    for (const card of sorted) {
      const found = existing.find((e) => cardEq(e.card, card));
      if (found) kept.push(found);
      else kept.push({ card, el: createFront(card) });
    }

    const container = this.containers.south;
    container.innerHTML = '';
    for (const entry of kept) container.appendChild(entry.el);
    this.hands.south = kept;
  }

  setOpponentCount(seat: Exclude<Seat, 'south'>, count: number): void {
    if (!this.containers) return;
    const container = this.containers[seat];
    const entries = this.hands[seat];
    if (count < entries.length) {
      const removed = entries.splice(count);
      for (const e of removed) e.el.remove();
    } else {
      for (let i = entries.length; i < count; i++) {
        const el = createBack();
        container.appendChild(el);
        entries.push({ card: null, el });
      }
    }
  }

  removeCard(card: Card): CardEl | null {
    const entries = this.hands.south;
    const idx = entries.findIndex((e) => cardEq(e.card, card));
    if (idx === -1) return null;
    const [entry] = entries.splice(idx, 1);
    entry!.el.remove();
    return entry!.el;
  }

  popOpponentBack(seat: Exclude<Seat, 'south'>): CardEl | null {
    const entries = this.hands[seat];
    const entry = entries.pop();
    if (!entry) return null;
    entry.el.remove();
    return entry.el;
  }

  cards(seat: Seat): readonly HandEntry[] {
    return this.hands[seat];
  }

  clear(): void {
    for (const seat of ['south', 'north', 'east', 'west'] as Seat[]) {
      for (const e of this.hands[seat]) e.el.remove();
      this.hands[seat] = [];
    }
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:component`
Expected: all 7 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/cards/hand-manager.ts tests/component/hand-manager.spec.ts
git commit -m "feat: HandManager for player + opponent DOM"
```

---

## Task 8: TrickManager

**Files:**

- Create: `src/cards/trick-manager.ts`, `tests/component/trick-manager.spec.ts`

Owns the 4-slot trick layout. Source reference: `card-manager.js` lines 149-175.

- [ ] **Step 1: Write failing test**

`tests/component/trick-manager.spec.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { TrickManager } from '../../src/cards/trick-manager';
import type { Card } from '../../src/state/helpers';

const c = (suit: Card['suit'], rank: Card['rank']): Card => ({ suit, rank });

describe('TrickManager', () => {
  let container: HTMLDivElement;
  let tm: TrickManager;

  beforeEach(() => {
    document.body.innerHTML = '<div id="trick"></div>';
    container = document.getElementById('trick') as HTMLDivElement;
    tm = new TrickManager();
    tm.init(container);
  });

  it('initializes with 4 placeholder slots', () => {
    expect(container.children.length).toBe(4);
    for (const child of container.children) {
      expect(child.className).toContain('trick-placeholder');
    }
  });

  it('fillNextSlot replaces a placeholder with a faced card', () => {
    tm.fillNextSlot(c('Heart', 'Ace'), 'south');
    expect(container.children[0]!.textContent).toBe('A♥');
    expect(container.children[0]!.className).not.toContain('trick-placeholder');
    expect(tm.count()).toBe(1);
  });

  it('clear() resets back to 4 placeholders', () => {
    tm.fillNextSlot(c('Heart', 'Ace'), 'south');
    tm.clear();
    expect(container.children.length).toBe(4);
    expect(tm.count()).toBe(0);
  });

  it('slots() returns the live list', () => {
    tm.fillNextSlot(c('Spade', 'King'), 'east');
    const slots = tm.slots();
    expect(slots[0]!.card).toEqual(c('Spade', 'King'));
    expect(slots[0]!.seat).toBe('east');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:component`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/cards/trick-manager.ts`**

```ts
import type { Card } from '../state/helpers';
import type { Seat } from './hand-manager';
import { cardText, type CardEl } from './card-el';

export type TrickSlot = { card: Card; seat: Seat; el: CardEl };

const SUIT_COLOR = { Spade: 'black', Heart: 'red', Diamond: 'red', Club: 'black' } as const;

export class TrickManager {
  private container: HTMLElement | null = null;
  private slotEls: CardEl[] = [];
  private filled: TrickSlot[] = [];

  init(container: HTMLElement): void {
    this.container = container;
    this.clear();
  }

  fillNextSlot(card: Card, seat: Seat): TrickSlot | null {
    const slot = this.slotEls.find((el) => el.classList.contains('trick-placeholder'));
    if (!slot) return null;
    const colorClass = SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black';
    slot.className = `card card-front ${colorClass}`;
    slot.textContent = cardText(card);
    const entry: TrickSlot = { card, seat, el: slot };
    this.filled.push(entry);
    return entry;
  }

  slots(): readonly TrickSlot[] {
    return this.filled;
  }

  slotEl(idx: number): CardEl | undefined {
    return this.slotEls[idx];
  }

  count(): number {
    return this.filled.length;
  }

  clear(): void {
    if (!this.container) return;
    this.container.innerHTML = '';
    this.slotEls = [];
    this.filled = [];
    for (let i = 0; i < 4; i++) {
      const el = document.createElement('div') as CardEl;
      el.className = 'card trick-placeholder';
      el._cm = { x: 0, y: 0 };
      this.container.appendChild(el);
      this.slotEls.push(el);
    }
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:component`
Expected: 4 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/cards/trick-manager.ts tests/component/trick-manager.spec.ts
git commit -m "feat: TrickManager owns the 4-slot trick layout"
```

---

## Task 9: Drag controller

**Files:**

- Create: `src/cards/drag.ts`, `tests/component/drag.spec.ts`

Pointer-based drag with a "drag-up threshold or click = play" gesture. Source reference: `card-manager.js` lines 430-543.

- [ ] **Step 1: Write failing test**

`tests/component/drag.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { attachDrag } from '../../src/cards/drag';

function pointer(el: HTMLElement, type: string, opts: PointerEventInit = {}): void {
  el.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 1, ...opts }));
}

describe('attachDrag', () => {
  let parent: HTMLElement;
  let el: HTMLElement;
  let onPlay: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    document.body.innerHTML =
      '<div id="parent"><div id="card" style="width:50px;height:70px"></div></div>';
    parent = document.getElementById('parent')!;
    el = document.getElementById('card')!;
    onPlay = vi.fn();
    // happy-dom doesn't implement setPointerCapture on Elements consistently — stub it.
    (el as HTMLElement & { setPointerCapture?: (n: number) => void }).setPointerCapture = () => {};
  });

  it('plays on simple click (small move)', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointerup', { clientX: 2, clientY: 1 });
    expect(onPlay).toHaveBeenCalledTimes(1);
  });

  it('plays when dragged up past threshold', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointermove', { clientX: 0, clientY: -70 });
    pointer(el, 'pointerup', { clientX: 0, clientY: -70 });
    expect(onPlay).toHaveBeenCalledTimes(1);
  });

  it('does not play when drag is below threshold and not a click', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointermove', { clientX: 30, clientY: 30 });
    pointer(el, 'pointerup', { clientX: 30, clientY: 30 });
    expect(onPlay).not.toHaveBeenCalled();
  });

  it('cleanup detaches listeners', () => {
    const cleanup = attachDrag(el, { threshold: 60, onPlay });
    cleanup();
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointerup', { clientX: 2, clientY: 2 });
    expect(onPlay).not.toHaveBeenCalled();
  });

  it('reports the source rect for fly animations', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointermove', { clientX: 0, clientY: -80 });
    pointer(el, 'pointerup', { clientX: 0, clientY: -80 });
    expect(onPlay).toHaveBeenCalledWith(
      expect.objectContaining({ width: expect.any(Number), height: expect.any(Number) }),
    );
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:component`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/cards/drag.ts`**

```ts
export type DragOpts = {
  threshold: number;
  onPlay: (srcRect: DOMRect) => void;
};

export function attachDrag(el: HTMLElement, opts: DragOpts): () => void {
  let startX = 0;
  let startY = 0;
  let dragging = false;
  let placeholder: HTMLElement | null = null;

  const onDown = (e: PointerEvent): void => {
    e.preventDefault();
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // pointer capture isn't supported (e.g. in test envs); proceed without it
    }
    startX = e.clientX;
    startY = e.clientY;
    dragging = true;

    if (el.parentNode) {
      placeholder = document.createElement('div');
      placeholder.className = 'card-placeholder';
      placeholder.style.width = el.offsetWidth + 'px';
      placeholder.style.height = el.offsetHeight + 'px';
      el.parentNode.insertBefore(placeholder, el);
    }

    const rect = el.getBoundingClientRect();
    el.classList.add('dragging');
    el.style.left = rect.left + 'px';
    el.style.top = rect.top + 'px';
    el.style.width = rect.width + 'px';
    el.style.height = rect.height + 'px';
    el.style.transform = '';
  };

  const onMove = (e: PointerEvent): void => {
    if (!dragging) return;
    e.preventDefault();
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    el.style.transform = `translate(${dx}px, ${dy}px)`;
    if (dy < -opts.threshold) el.classList.add('card-will-play');
    else el.classList.remove('card-will-play');
  };

  const reset = (): void => {
    if (placeholder?.parentNode) placeholder.parentNode.removeChild(placeholder);
    placeholder = null;
    el.classList.remove('dragging', 'card-will-play');
    el.style.left = '';
    el.style.top = '';
    el.style.width = '';
    el.style.height = '';
    el.style.transform = '';
  };

  const onUp = (e: PointerEvent): void => {
    if (!dragging) return;
    dragging = false;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    const isClick = Math.abs(dx) < 10 && Math.abs(dy) < 10;
    const isPlay = dy < -opts.threshold;
    const srcRect = el.getBoundingClientRect();
    reset();
    if (isPlay || isClick) opts.onPlay(srcRect);
  };

  const onCancel = (): void => {
    if (!dragging) return;
    dragging = false;
    reset();
  };

  el.addEventListener('pointerdown', onDown);
  el.addEventListener('pointermove', onMove);
  el.addEventListener('pointerup', onUp);
  el.addEventListener('pointercancel', onCancel);

  return () => {
    el.removeEventListener('pointerdown', onDown);
    el.removeEventListener('pointermove', onMove);
    el.removeEventListener('pointerup', onUp);
    el.removeEventListener('pointercancel', onCancel);
  };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:component`
Expected: 5 cases pass.

- [ ] **Step 5: Commit**

```bash
git add src/cards/drag.ts tests/component/drag.spec.ts
git commit -m "feat: pointer-based drag controller"
```

---

## Task 10: Card orchestrator (glue layer)

**Files:**

- Create: `src/cards/orchestrator.ts`

The orchestrator's public surface matches the existing `CardManager` so `routes/play.ts` reads like the current Alpine component's calls. Composition: `HandManager` + `TrickManager` + `attachDrag` + `animateTo`.

This is the largest single module in the plan. The reference is `personal-site/spades/js/card-manager.js`. Behavior is pinned by the tests in Task 11.

- [ ] **Step 1: Implement `src/cards/orchestrator.ts`**

```ts
import type { Card } from '../state/helpers';
import { cardEq, sortCards } from '../state/helpers';
import { createBack, createFront, setPos, type CardEl } from './card-el';
import { animateTo } from './animation';
import { HandManager, type Seat, type Containers } from './hand-manager';
import { TrickManager } from './trick-manager';
import { attachDrag } from './drag';

type WinnerOffset = { x: number; y: number };
const TRICK_OFFSETS: Record<Seat, WinnerOffset> = {
  south: { x: 0, y: 60 },
  north: { x: 0, y: -60 },
  west: { x: -80, y: 0 },
  east: { x: 80, y: 0 },
};

export type OrchestratorOpts = {
  containers: Containers;
};

export class CardOrchestrator {
  private hand = new HandManager();
  private trick = new TrickManager();
  private containers: Containers;
  private dragCleanups: Array<() => void> = [];
  private collectingTrick = false;
  private lastPlayRect: DOMRect | null = null;
  private initialized = false;

  constructor(opts: OrchestratorOpts) {
    this.containers = opts.containers;
    this.hand.setContainers(this.containers);
    this.trick.init(this.containers.trick);
  }

  /** First-time setup or reconnect: place everything immediately, no deal animation. */
  setupImmediate(args: {
    playerHand: readonly Card[];
    oppCounts: { north: number; west: number; east: number };
    tableCards: readonly (Card | null)[];
    myIdx: number;
    northIdx: number;
    westIdx: number;
    eastIdx: number;
    currentPlayerSeatIdx: number;
  }): void {
    this.clearAll();
    this.hand.setPlayerHand(args.playerHand);
    this.hand.setOpponentCount('north', args.oppCounts.north);
    this.hand.setOpponentCount('west', args.oppCounts.west);
    this.hand.setOpponentCount('east', args.oppCounts.east);

    const seatMap: Record<number, Seat> = {
      [args.myIdx]: 'south',
      [args.northIdx]: 'north',
      [args.westIdx]: 'west',
      [args.eastIdx]: 'east',
    };
    this.trick.clear();
    const n = args.tableCards.filter(
      (tc) => tc && (tc as { suit?: string }).suit !== 'Blank',
    ).length;
    const leaderSeat = (((args.currentPlayerSeatIdx - n) % 4) + 4) % 4;
    for (let i = 0; i < 4; i++) {
      const absIdx = (leaderSeat + i) % 4;
      const tc = args.tableCards[absIdx];
      const seat = seatMap[absIdx];
      if (tc && (tc as { suit?: string }).suit !== 'Blank' && seat) {
        this.trick.fillNextSlot(tc, seat);
      }
    }
    this.initialized = true;
  }

  isInitialized(): boolean {
    return this.initialized;
  }

  updatePlayerHand(cards: readonly Card[]): void {
    this.hand.setPlayerHand(cards);
  }

  updateOpponentCount(seat: Exclude<Seat, 'south'>, count: number): void {
    this.hand.setOpponentCount(seat, count);
  }

  /** Fly the south player's card from hand → next trick slot. */
  async playCardToCenter(card: Card): Promise<void> {
    const removed = this.hand.removeCard(card);
    const slot = this.trick.fillNextSlot(card, 'south');
    if (!removed || !slot) return;

    const srcRect = this.lastPlayRect ?? removed.getBoundingClientRect();
    this.lastPlayRect = null;

    slot.el.style.visibility = 'hidden';
    document.body.appendChild(removed);
    removed.style.position = 'fixed';
    removed.style.left = srcRect.left + 'px';
    removed.style.top = srcRect.top + 'px';
    removed.style.width = srcRect.width + 'px';
    removed.style.height = srcRect.height + 'px';
    removed.style.zIndex = '1000';
    removed.style.margin = '0';
    removed.style.transform = '';
    removed._cm = { x: 0, y: 0 };

    const targetRect = slot.el.getBoundingClientRect();
    await animateTo(removed, {
      x: targetRect.left - srcRect.left,
      y: targetRect.top - srcRect.top,
      duration: 250,
      ease: 'quartOut',
    });
    removed.remove();
    slot.el.style.visibility = '';
  }

  /** Drop an opponent's card into the next trick slot (no fly animation today). */
  playOpponentCardToCenter(card: Card, seat: Exclude<Seat, 'south'>): void {
    this.hand.popOpponentBack(seat);
    this.trick.fillNextSlot(card, seat);
  }

  /** Force-place a card that should be in the trick (used to backfill on AI fast-play). */
  placeCardInTrick(card: Card, seat: Seat): void {
    this.trick.fillNextSlot(card, seat);
  }

  /** Pause → stack → slide toward winner → fade. */
  async collectTrick(winnerSeat: Seat): Promise<void> {
    if (this.trick.count() === 0) return;
    if (this.collectingTrick) return;
    this.collectingTrick = true;
    try {
      const container = this.containers.trick;
      const containerRect = container.getBoundingClientRect();
      const filled = [...this.trick.slots()];
      const positions = filled.map((entry) => {
        const r = entry.el.getBoundingClientRect();
        return {
          left: r.left - containerRect.left,
          top: r.top - containerRect.top,
          width: r.width,
          height: r.height,
        };
      });

      // Absolutely position all slots so layout doesn't shift during fade
      const allSlots = [0, 1, 2, 3]
        .map((i) => this.trick.slotEl(i))
        .filter((x): x is CardEl => !!x);
      const allPositions = allSlots.map((el) => {
        const r = el.getBoundingClientRect();
        return {
          left: r.left - containerRect.left,
          top: r.top - containerRect.top,
          width: r.width,
          height: r.height,
        };
      });
      allSlots.forEach((el, i) => {
        el.style.position = 'absolute';
        el.style.left = allPositions[i]!.left + 'px';
        el.style.top = allPositions[i]!.top + 'px';
        el.style.width = allPositions[i]!.width + 'px';
        el.style.height = allPositions[i]!.height + 'px';
      });
      filled.forEach((entry) => {
        entry.el.style.transform = '';
        entry.el._cm = { x: 0, y: 0 };
      });
      allSlots.forEach((el) => {
        if (el.classList.contains('trick-placeholder')) el.style.visibility = 'hidden';
      });

      await new Promise((r) => setTimeout(r, 400));

      const cw = positions[0]?.width ?? 46;
      const ch = positions[0]?.height ?? 64;
      const centerX = containerRect.width / 2 - cw / 2;
      const centerY = containerRect.height / 2 - ch / 2;

      await Promise.all(
        filled.map((entry, i) => {
          const pos = positions[i]!;
          const targetX = centerX - pos.left + (i - 1.5) * 2;
          const targetY = centerY - pos.top + (i - 1.5) * 1;
          return animateTo(entry.el, { x: targetX, y: targetY, duration: 200, ease: 'quartOut' });
        }),
      );

      const offset = TRICK_OFFSETS[winnerSeat];
      await Promise.all(
        filled.map((entry) =>
          animateTo(entry.el, {
            x: entry.el._cm.x + offset.x,
            y: entry.el._cm.y + offset.y,
            duration: 300,
            ease: 'quartIn',
            onProgress: (t) => {
              entry.el.style.opacity = `${1 - t}`;
            },
            onComplete: () => {
              entry.el.style.opacity = '';
            },
          }),
        ),
      );

      this.trick.clear();
    } finally {
      this.collectingTrick = false;
    }
  }

  clearTrick(): void {
    this.trick.clear();
  }

  enableInteraction(validCards: readonly Card[], onPlay: (card: Card) => void): void {
    this.disableInteraction();
    for (const entry of this.hand.cards('south')) {
      if (!entry.card) continue;
      const isValid = validCards.some((vc) => cardEq(vc, entry.card!));
      if (isValid) {
        entry.el.classList.add('cm-clickable');
        entry.el.classList.remove('cm-invalid');
        entry.el.style.opacity = '';
        const card = entry.card;
        const cleanup = attachDrag(entry.el, {
          threshold: 60,
          onPlay: (rect) => {
            this.lastPlayRect = rect;
            onPlay(card);
          },
        });
        this.dragCleanups.push(cleanup);
      } else {
        entry.el.classList.remove('cm-clickable');
        entry.el.classList.add('cm-invalid');
        entry.el.style.opacity = '0.35';
      }
    }
  }

  disableInteraction(): void {
    for (const fn of this.dragCleanups) fn();
    this.dragCleanups = [];
    for (const entry of this.hand.cards('south')) {
      entry.el.classList.remove('cm-clickable', 'cm-invalid', 'dragging', 'card-will-play');
      entry.el.style.opacity = '';
      entry.el.style.left = '';
      entry.el.style.top = '';
      entry.el.style.width = '';
      entry.el.style.height = '';
      entry.el.style.transform = '';
    }
  }

  trickCount(): number {
    return this.trick.count();
  }

  clearAll(): void {
    this.disableInteraction();
    this.hand.clear();
    this.trick.clear();
    this.initialized = false;
  }

  destroy(): void {
    this.clearAll();
  }
}
```

- [ ] **Step 2: Type-check passes**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/cards/orchestrator.ts
git commit -m "feat: CardOrchestrator glues hand/trick/drag/animation"
```

(Behavior is covered by the E2E tests in Task 14; happy-dom doesn't simulate layout well enough for collect-trick to be component-testable.)

---

## Task 11: localStorage helper for game sessions

**Files:**

- Create: `src/lib/storage.ts`, `tests/unit/storage.spec.ts`

- [ ] **Step 1: Write failing test**

`tests/unit/storage.spec.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { saveSession, loadSession, clearSession } from '../../src/lib/storage';

describe('game session storage', () => {
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

  it('round-trips a session', () => {
    saveSession('abc', 'gid', 'pid');
    expect(loadSession('abc')).toEqual({ gid: 'gid', pid: 'pid' });
  });

  it('returns null for missing', () => {
    expect(loadSession('nope')).toBe(null);
  });

  it('clears a session', () => {
    saveSession('abc', 'gid', 'pid');
    clearSession('abc');
    expect(loadSession('abc')).toBe(null);
  });

  it('returns null on parse error', () => {
    localStorage.setItem('spades_game_bad', 'not json');
    expect(loadSession('bad')).toBe(null);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL.

- [ ] **Step 3: Implement `src/lib/storage.ts`**

```ts
export type SavedSession = { gid: string; pid: string };

const key = (shortId: string): string => `spades_game_${shortId}`;

export function saveSession(shortId: string, gid: string, pid: string): void {
  try {
    localStorage.setItem(key(shortId), JSON.stringify({ gid, pid }));
  } catch {
    // localStorage may be unavailable (e.g. private mode); ignore
  }
}

export function loadSession(shortId: string): SavedSession | null {
  try {
    const raw = localStorage.getItem(key(shortId));
    if (!raw) return null;
    return JSON.parse(raw) as SavedSession;
  } catch {
    return null;
  }
}

export function clearSession(shortId: string): void {
  try {
    localStorage.removeItem(key(shortId));
  } catch {
    // ignore
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test:unit`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/storage.ts tests/unit/storage.spec.ts
git commit -m "feat: localStorage helper for game sessions"
```

---

## Task 12: Game store with fixture-driven tests

**Files:**

- Create: `src/state/game.ts`, `tests/fixtures/ws-events/{betting,trick,trick-complete,completed}.json`, `tests/unit/game-store.spec.ts`

The store is the most consequential pure module. Cover `applyState` for the three live phases (BETTING, PLAYING mid-trick, GAME_OVER) and `applyPresence`. `applyWsEvent` is exercised via the same payloads — it dispatches to either `applyState` (for `StateChanged`) or a presence update.

You'll create the fixtures by running rust-spades against a recorded game and capturing the WS payloads. For this plan, hand-write minimal fixtures that cover the branches (a typical state has ~15 fields, all listed in `personal-site/spades/index.html` lines 815-840).

- [ ] **Step 1: Create fixture: `tests/fixtures/ws-events/betting.json`**

```json
{
  "game_id": "00000000-0000-0000-0000-000000000001",
  "state": { "Betting": 0 },
  "team_a_score": 0,
  "team_b_score": 0,
  "team_a_bags": 0,
  "team_b_bags": 0,
  "current_player_id": "p0",
  "player_names": [
    { "player_id": "p0", "name": "Alice" },
    { "player_id": "p1", "name": null },
    { "player_id": "p2", "name": null },
    { "player_id": "p3", "name": null }
  ],
  "table_cards": [null, null, null, null],
  "player_bets": [null, null, null, null],
  "player_tricks_won": [0, 0, 0, 0],
  "last_trick_winner_id": null,
  "timer_config": null,
  "player_clocks_ms": null,
  "active_player_clock_ms": null
}
```

- [ ] **Step 2: Create fixture: `tests/fixtures/ws-events/trick.json`**

```json
{
  "game_id": "00000000-0000-0000-0000-000000000001",
  "state": { "Trick": 1 },
  "team_a_score": 0,
  "team_b_score": 0,
  "team_a_bags": 0,
  "team_b_bags": 0,
  "current_player_id": "p2",
  "player_names": [
    { "player_id": "p0", "name": "Alice" },
    { "player_id": "p1", "name": "Bob" },
    { "player_id": "p2", "name": "Carol" },
    { "player_id": "p3", "name": "Dan" }
  ],
  "table_cards": [
    { "suit": "Heart", "rank": "Ace" },
    { "suit": "Heart", "rank": "Five" },
    null,
    null
  ],
  "player_bets": [3, 4, 2, 3],
  "player_tricks_won": [1, 0, 0, 0],
  "last_trick_winner_id": "p0",
  "timer_config": null,
  "player_clocks_ms": null,
  "active_player_clock_ms": null
}
```

- [ ] **Step 3: Create fixture: `tests/fixtures/ws-events/completed.json`**

```json
{
  "game_id": "00000000-0000-0000-0000-000000000001",
  "state": "Completed",
  "team_a_score": 540,
  "team_b_score": 420,
  "team_a_bags": 3,
  "team_b_bags": 5,
  "current_player_id": "p0",
  "player_names": [
    { "player_id": "p0", "name": "Alice" },
    { "player_id": "p1", "name": "Bob" },
    { "player_id": "p2", "name": "Carol" },
    { "player_id": "p3", "name": "Dan" }
  ],
  "table_cards": [null, null, null, null],
  "player_bets": [3, 4, 2, 3],
  "player_tricks_won": [4, 3, 3, 3],
  "last_trick_winner_id": "p0",
  "timer_config": null,
  "player_clocks_ms": null,
  "active_player_clock_ms": null
}
```

- [ ] **Step 4: Write failing test**

`tests/unit/game-store.spec.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { createGameStore } from '../../src/state/game';
import betting from '../fixtures/ws-events/betting.json';
import trick from '../fixtures/ws-events/trick.json';
import completed from '../fixtures/ws-events/completed.json';

describe('createGameStore', () => {
  it('applyState BETTING transitions phase to BETTING', () => {
    const s = createGameStore('p0');
    s.applyState(betting as never, { player_id: 'p0', cards: [] });
    expect(s.phase.value).toBe('BETTING');
    expect(s.currentPlayerId.value).toBe('p0');
    expect(s.playerNames.value).toEqual(['Alice', null, null, null]);
  });

  it('applyState mid-trick transitions phase to PLAYING with correct table', () => {
    const s = createGameStore('p0');
    s.applyState(trick as never, { player_id: 'p0', cards: [{ suit: 'Spade', rank: 'Ace' }] });
    expect(s.phase.value).toBe('PLAYING');
    expect(s.tableCards.value[0]).toEqual({ suit: 'Heart', rank: 'Ace' });
    expect(s.playerBets.value).toEqual([3, 4, 2, 3]);
    expect(s.hand.value.length).toBe(1);
  });

  it('applyState Completed → GAME_OVER', () => {
    const s = createGameStore('p0');
    s.applyState(completed as never, { player_id: 'p0', cards: [] });
    expect(s.phase.value).toBe('GAME_OVER');
    expect(s.teamAScore.value).toBe(540);
  });

  it('applyPresence updates the seat-aligned connected flags', () => {
    const s = createGameStore('p0');
    s.applyState(trick as never, { player_id: 'p0', cards: [] });
    s.applyPresence([
      { player_id: 'p0', connected: true },
      { player_id: 'p1', connected: false },
      { player_id: 'p2', connected: true },
      { player_id: 'p3', connected: false },
    ]);
    expect(s.playerConnected.value).toEqual([true, false, true, false]);
  });

  it('updateSpadesBroken resets on BETTING and detects spade off-lead', () => {
    const s = createGameStore('p0');
    s.applyState(betting as never, { player_id: 'p0', cards: [] });
    expect(s.spadesBroken.value).toBe(false);
    s.applyState(trick as never, { player_id: 'p0', cards: [] });
    // table is Heart Ace, Heart Five — no spade played, spades still not broken
    expect(s.spadesBroken.value).toBe(false);
  });
});
```

- [ ] **Step 5: Run test to verify it fails**

Run: `pnpm test:unit`
Expected: FAIL.

- [ ] **Step 6: Implement `src/state/game.ts`**

```ts
import { signal, type Signal } from '@preact/signals-core';
import type { Card, Phase, Suit } from './helpers';
import { getLeadSuit } from './helpers';

export type GameStateValue = string | { Betting?: number; Trick?: number; Completed?: unknown };

export type PlayerNameEntry = { player_id: string; name: string | null };
export type TimerConfig = { initial_time_secs: number; increment_secs: number } | null;
export type PresencePlayer = { player_id: string; connected: boolean };

export type GameStateResponse = {
  game_id: string;
  state: GameStateValue;
  team_a_score: number;
  team_b_score: number;
  team_a_bags: number;
  team_b_bags: number;
  current_player_id: string | null;
  player_names: PlayerNameEntry[];
  table_cards?: (Card | null)[];
  player_bets?: (number | null)[];
  player_tricks_won?: number[];
  last_trick_winner_id?: string | null;
  timer_config?: TimerConfig;
  player_clocks_ms?: (number | null)[] | null;
  active_player_clock_ms?: number | null;
  last_completed_trick?: (Card | null)[] | null;
  short_id?: string | null;
};

export type HandResponse = {
  player_id: string;
  cards: Card[];
};

export type GameStore = {
  // Identity
  playerId: Signal<string>;
  // Phase
  phase: Signal<Phase>;
  gameState: Signal<GameStateValue | null>;
  // Players & table
  playerIds: Signal<string[]>;
  playerNames: Signal<(string | null)[]>;
  playerConnected: Signal<boolean[]>;
  currentPlayerId: Signal<string | null>;
  hand: Signal<Card[]>;
  tableCards: Signal<(Card | null)[]>;
  playerBets: Signal<(number | null)[]>;
  playerTricksWon: Signal<number[]>;
  lastTrickWinnerId: Signal<string | null>;
  // Scores
  teamAScore: Signal<number>;
  teamBScore: Signal<number>;
  teamABags: Signal<number>;
  teamBBags: Signal<number>;
  // Clock
  timerConfig: Signal<TimerConfig>;
  playerClocksMs: Signal<(number | null)[] | null>;
  activePlayerClockMs: Signal<number | null>;
  // Derived
  spadesBroken: Signal<boolean>;
  // Mutators
  applyState: (state: GameStateResponse, hand: HandResponse) => void;
  applyPresence: (players: PresencePlayer[]) => void;
};

function phaseFromState(g: GameStateValue): Phase {
  if (typeof g === 'object' && g !== null && 'Betting' in g) return 'BETTING';
  if (typeof g === 'object' && g !== null && 'Trick' in g) return 'PLAYING';
  if (g === 'Completed') return 'GAME_OVER';
  if (typeof g === 'string') {
    if (g.startsWith('Betting')) return 'BETTING';
    if (g.startsWith('Trick')) return 'PLAYING';
  }
  return 'LOBBY';
}

export function createGameStore(playerIdInit: string): GameStore {
  const playerId = signal(playerIdInit);
  const phase = signal<Phase>('LOBBY');
  const gameState = signal<GameStateValue | null>(null);
  const playerIds = signal<string[]>([]);
  const playerNames = signal<(string | null)[]>([null, null, null, null]);
  const playerConnected = signal<boolean[]>([true, true, true, true]);
  const currentPlayerId = signal<string | null>(null);
  const hand = signal<Card[]>([]);
  const tableCards = signal<(Card | null)[]>([null, null, null, null]);
  const playerBets = signal<(number | null)[]>([null, null, null, null]);
  const playerTricksWon = signal<number[]>([0, 0, 0, 0]);
  const lastTrickWinnerId = signal<string | null>(null);
  const teamAScore = signal(0);
  const teamBScore = signal(0);
  const teamABags = signal(0);
  const teamBBags = signal(0);
  const timerConfig = signal<TimerConfig>(null);
  const playerClocksMs = signal<(number | null)[] | null>(null);
  const activePlayerClockMs = signal<number | null>(null);
  const spadesBroken = signal(false);

  const updateSpadesBroken = (): void => {
    if (phase.value === 'BETTING') {
      spadesBroken.value = false;
      return;
    }
    if (spadesBroken.value) return;
    const myIdx = playerIds.value.indexOf(playerId.value);
    const leadSuit: Suit | null = getLeadSuit(tableCards.value, myIdx < 0 ? 0 : myIdx);
    if (leadSuit && leadSuit !== 'Spade') {
      for (const c of tableCards.value) {
        if (c && c.suit === 'Spade') {
          spadesBroken.value = true;
          return;
        }
      }
    }
  };

  const applyState: GameStore['applyState'] = (state, handData) => {
    gameState.value = state.state;
    currentPlayerId.value = state.current_player_id;
    teamAScore.value = state.team_a_score;
    teamBScore.value = state.team_b_score;
    teamABags.value = state.team_a_bags;
    teamBBags.value = state.team_b_bags;
    timerConfig.value = state.timer_config ?? null;
    playerClocksMs.value = state.player_clocks_ms ?? null;
    activePlayerClockMs.value = state.active_player_clock_ms ?? null;
    playerNames.value = (state.player_names ?? []).map((e) => e.name);
    playerIds.value = (state.player_names ?? []).map((e) => e.player_id);
    tableCards.value = state.table_cards ?? [null, null, null, null];
    playerBets.value = state.player_bets ?? [null, null, null, null];
    playerTricksWon.value = state.player_tricks_won ?? [0, 0, 0, 0];
    lastTrickWinnerId.value = state.last_trick_winner_id ?? null;
    hand.value = handData.cards ?? [];
    phase.value = phaseFromState(state.state);
    updateSpadesBroken();
  };

  const applyPresence: GameStore['applyPresence'] = (players) => {
    const next: boolean[] = [true, true, true, true];
    for (const p of players) {
      const idx = playerIds.value.indexOf(p.player_id);
      if (idx !== -1) next[idx] = p.connected;
    }
    playerConnected.value = next;
  };

  return {
    playerId,
    phase,
    gameState,
    playerIds,
    playerNames,
    playerConnected,
    currentPlayerId,
    hand,
    tableCards,
    playerBets,
    playerTricksWon,
    lastTrickWinnerId,
    teamAScore,
    teamBScore,
    teamABags,
    teamBBags,
    timerConfig,
    playerClocksMs,
    activePlayerClockMs,
    spadesBroken,
    applyState,
    applyPresence,
  };
}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `pnpm test:unit`
Expected: 5 cases pass.

- [ ] **Step 8: Commit**

```bash
git add src/state/game.ts tests/unit/game-store.spec.ts tests/fixtures/
git commit -m "feat: signal-based game store + fixture tests"
```

---

## Task 13: Menu queue-sizes signal

**Files:**

- Create: `src/state/menu.ts`

- [ ] **Step 1: Implement `src/state/menu.ts`**

```ts
import { signal } from '@preact/signals-core';
import { request } from '../api/client';

export type QueueSize = {
  max_points: number;
  timer_config: { initial_time_secs: number; increment_secs: number };
  waiting: number;
};

export const queueSizes = signal<QueueSize[]>([]);

let timer: ReturnType<typeof setInterval> | null = null;

export async function refreshQueueSizes(): Promise<void> {
  try {
    queueSizes.value = await request<QueueSize[]>('/matchmaking/queue-sizes', { method: 'GET' });
  } catch {
    // best-effort; ignore
  }
}

export function startQueuePoll(intervalMs = 10_000): void {
  stopQueuePoll();
  void refreshQueueSizes();
  timer = setInterval(() => void refreshQueueSizes(), intervalMs);
}

export function stopQueuePoll(): void {
  if (timer) clearInterval(timer);
  timer = null;
}

export function queueCountFor(timerCfg: QueueSize['timer_config']): number {
  const e = queueSizes.value.find(
    (q) =>
      q.max_points === 500 &&
      q.timer_config.initial_time_secs === timerCfg.initial_time_secs &&
      q.timer_config.increment_secs === timerCfg.increment_secs,
  );
  return e?.waiting ?? 0;
}
```

- [ ] **Step 2: Type-check**

Run: `pnpm tsc --noEmit -p tsconfig.json`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/state/menu.ts
git commit -m "feat: queue-sizes signal + polling"
```

---

## Task 14: Play route — AI / Computers happy path

**Files:**

- Create: `src/routes/play.ts`, `src/ui/components/scores.ts`, `src/ui/components/game-table.ts`
- Modify: `src/main.ts`, `src/routes/home.ts`, `src/ui/design.css`

This task wires Play with Computers end-to-end. Quickplay and Friends come in Tasks 15-16; their UIs live in the same `play.ts` route (lobby + waiting are sub-renders).

The route is large because it owns the entire in-game template plus the boot/reconnect chain. Keep this task disciplined: only Play with Computers in this commit.

- [ ] **Step 1: Add game-table styles to `src/ui/design.css`**

Append:

```css
.card {
  width: 46px;
  height: 64px;
  border-radius: var(--radius-sm);
  background: white;
  border: 1px solid rgba(0, 0, 0, 0.15);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 14px;
  user-select: none;
  position: relative;
}
.card-red {
  color: var(--color-card-red);
}
.card-black {
  color: var(--color-card-black);
}
.card-back {
  background: repeating-linear-gradient(45deg, #2a9d8f, #2a9d8f 4px, #1f7e74 4px, #1f7e74 8px);
}
.trick-placeholder {
  background: transparent;
  border: 1px dashed rgba(0, 0, 0, 0.15);
}
.card.dragging {
  position: fixed;
  z-index: 999;
}
.card-will-play {
  box-shadow: 0 -4px 12px rgba(42, 157, 143, 0.4);
}
.cm-clickable {
  cursor: pointer;
}
.card-placeholder {
  opacity: 0;
}

.spades-table {
  display: grid;
  grid-template-columns: 1fr 2fr 1fr;
  grid-template-rows: auto 1fr auto;
  grid-template-areas: 'north north north' 'west center east' 'south south south';
  gap: var(--space-3);
  width: 100%;
  max-width: 720px;
  min-height: 480px;
}
.seat-north {
  grid-area: north;
  justify-self: center;
}
.seat-south {
  grid-area: south;
  justify-self: center;
}
.seat-west {
  grid-area: west;
  align-self: center;
}
.seat-east {
  grid-area: east;
  align-self: center;
  justify-self: end;
}
.spades-table-center {
  grid-area: center;
  display: flex;
  align-items: center;
  justify-content: center;
}

.card-container {
  display: flex;
  gap: 2px;
  flex-wrap: wrap;
  justify-content: center;
}
.hand-container {
  gap: 4px;
}
.trick-container {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--space-2);
  min-width: 240px;
}

.spades-scores {
  display: flex;
  justify-content: space-between;
  gap: var(--space-4);
  width: 100%;
  max-width: 720px;
  padding: var(--space-3) 0;
  font-size: var(--font-size-sm);
}
.spades-score-team {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
}
.spades-scores-center {
  color: var(--color-muted);
}

.spades-bets {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: var(--space-1);
  margin-top: var(--space-3);
  width: 100%;
  max-width: 480px;
}
.spades-bet {
  padding: var(--space-2);
}
```

- [ ] **Step 2: Create `src/ui/components/scores.ts`**

```ts
import { html, type TemplateResult } from 'lit-html';

export type ScoresProps = {
  teamAScore: number;
  teamBScore: number;
  teamABags: number;
  teamBBags: number;
  myTeam: 'A' | 'B';
  centerText: string;
};

export function scores(p: ScoresProps): TemplateResult {
  return html`<section class="spades-scores">
    <div class="spades-score-team">
      <strong>Team A${p.myTeam === 'A' ? ' (You)' : ''}</strong>
      <span>Score: ${p.teamAScore} | Bags: ${p.teamABags}</span>
    </div>
    <div class="spades-scores-center">${p.centerText}</div>
    <div class="spades-score-team">
      <strong>Team B${p.myTeam === 'B' ? ' (You)' : ''}</strong>
      <span>Score: ${p.teamBScore} | Bags: ${p.teamBBags}</span>
    </div>
  </section>`;
}
```

- [ ] **Step 3: Create `src/ui/components/game-table.ts`**

```ts
import { html, type TemplateResult, type Ref, createRef, ref } from 'lit-html/directives/ref.js';

export type SeatProps = {
  name: string;
  active: boolean;
  connected: boolean;
  betInfo: string;
  clockText: string | null;
};

export type GameTableRefs = {
  hand: Ref<HTMLDivElement>;
  north: Ref<HTMLDivElement>;
  west: Ref<HTMLDivElement>;
  east: Ref<HTMLDivElement>;
  trick: Ref<HTMLDivElement>;
};

export function makeRefs(): GameTableRefs {
  return {
    hand: createRef<HTMLDivElement>(),
    north: createRef<HTMLDivElement>(),
    west: createRef<HTMLDivElement>(),
    east: createRef<HTMLDivElement>(),
    trick: createRef<HTMLDivElement>(),
  };
}

export function gameTable(args: {
  north: SeatProps;
  west: SeatProps;
  east: SeatProps;
  south: SeatProps;
  centerText: string;
  refs: GameTableRefs;
}): TemplateResult {
  const seat = (cls: string, p: SeatProps, refEl: Ref<HTMLDivElement>): TemplateResult =>
    html`<div class=${`spades-seat ${cls}${p.active ? ' active' : ''}`}>
      <span class="spades-seat-label">${p.connected ? '● ' : '○ '}${p.name}</span>
      ${p.clockText ? html`<span class="spades-clock">${p.clockText}</span>` : null}
      <span class="spades-seat-info">${p.betInfo}</span>
      <div class="card-container opp-container" ${ref(refEl)}></div>
    </div>`;

  return html`<div class="spades-table">
    ${seat('seat-north', args.north, args.refs.north)}
    ${seat('seat-west', args.west, args.refs.west)}
    <div class="spades-table-center">
      <div class="spades-trick-area">
        <div class="card-container trick-container" ${ref(args.refs.trick)}></div>
      </div>
      <span class="spades-center-text">${args.centerText}</span>
    </div>
    ${seat('seat-east', args.east, args.refs.east)}
    <div class="spades-seat seat-south${args.south.active ? ' active' : ''}">
      <span class="spades-seat-label">${args.south.connected ? '● ' : '○ '}${args.south.name}</span>
      ${args.south.clockText
        ? html`<span class="spades-clock">${args.south.clockText}</span>`
        : null}
      <span class="spades-seat-info">${args.south.betInfo}</span>
      <div class="card-container hand-container" ${ref(args.refs.hand)}></div>
    </div>
  </div>`;
}
```

- [ ] **Step 4: Create `src/routes/play.ts`**

This is the largest single source file in the plan. It composes everything that's been built.

```ts
import { html, render } from 'lit-html';
import { effect } from '@preact/signals-core';
import { api, ApiError, request } from '../api/client';
import { openGameWs, type WsHandle } from '../api/ws';
import { createGameStore, type GameStore } from '../state/game';
import { sortCards, isCardValid, oppCardCount, seatRel, type Card } from '../state/helpers';
import { saveSession, loadSession, clearSession } from '../lib/storage';
import { navigateTo } from '../lib/util';
import { CardOrchestrator } from '../cards/orchestrator';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { scores } from '../ui/components/scores';
import { gameTable, makeRefs, type GameTableRefs } from '../ui/components/game-table';
import type { RouteModule } from '../router';

const POLL_INTERVAL = 2000;

type Resources = {
  cleanups: Array<() => void>;
  ws: WsHandle | null;
  pollTimer: ReturnType<typeof setInterval> | null;
  orchestrator: CardOrchestrator | null;
};

function disposeResources(r: Resources): void {
  r.ws?.close();
  r.ws = null;
  if (r.pollTimer) clearInterval(r.pollTimer);
  r.pollTimer = null;
  r.orchestrator?.destroy();
  r.orchestrator = null;
  for (const c of r.cleanups) c();
  r.cleanups = [];
}

async function startAIGame(
  shortIdHint: string | null,
): Promise<{ gameId: string; playerId: string; shortId: string }> {
  // POST /games with max_points + num_humans=1
  // openapi-fetch typing for this route is set up by Phase 0; if not present yet, use `request`.
  const created = await request<{ game_id: string; player_ids: string[] }>('/games', {
    method: 'POST',
    body: JSON.stringify({ max_points: 500, num_humans: 1 }),
  });
  const state = await request<{ short_id?: string | null }>(`/games/${created.game_id}`, {
    method: 'GET',
  });
  const shortId = state.short_id ?? shortIdHint ?? created.game_id;
  return { gameId: created.game_id, playerId: created.player_ids[0]!, shortId };
}

async function bootFromUrl(
  shortId: string,
  resources: Resources,
): Promise<{ store: GameStore; gameId: string; playerId: string } | { error: string }> {
  // 1. localStorage
  const saved = loadSession(shortId);
  if (saved) {
    try {
      const state = await request<never>(`/games/${saved.gid}`, { method: 'GET' });
      const hand = await request<never>(`/games/${saved.gid}/players/${saved.pid}/hand`, {
        method: 'GET',
      });
      const store = createGameStore(saved.pid);
      store.applyState(state, hand);
      try {
        const presence = await request<{ players: { player_id: string; connected: boolean }[] }>(
          `/games/${saved.gid}/presence`,
          { method: 'GET' },
        );
        store.applyPresence(presence.players);
      } catch {
        // optional
      }
      return { store, gameId: saved.gid, playerId: saved.pid };
    } catch {
      clearSession(shortId);
    }
  }

  // 2. by-player-url
  try {
    const resp = await request<{
      game_id: string;
      player_short_id?: string;
      player_id: string;
      game: never;
      hand: never;
    }>(`/games/by-player-url/${shortId}`, { method: 'GET' });
    const playerId = resp.player_short_id ?? resp.player_id;
    const store = createGameStore(playerId);
    store.applyState(resp.game, resp.hand);
    saveSession(shortId, resp.game_id, playerId);
    return { store, gameId: resp.game_id, playerId };
  } catch {
    // fall through
  }

  // 3. by-short-id (challenge)
  try {
    const status = await request<{ status: 'open' | 'started' | 'cancelled' | 'expired' }>(
      `/challenges/by-short-id/${shortId}`,
      { method: 'GET' },
    );
    if (status.status === 'open') {
      // Lobby-only state: rendered by a future Quickplay/Friends task.
      return { error: 'Challenge lobby (handled in Task 16)' };
    }
    if (status.status === 'started') return { error: 'This game has already started.' };
    return { error: 'This challenge is no longer available.' };
  } catch {
    return { error: 'Game or challenge not found.' };
  }
}

function renderInGame(args: {
  root: HTMLElement;
  store: GameStore;
  gameId: string;
  resources: Resources;
  refs: GameTableRefs;
}): () => void {
  const { store, refs, root } = args;

  const myIdx = (): number => store.playerIds.value.indexOf(store.playerId.value);
  const seatName = (idx: number): string => store.playerNames.value[idx] ?? `Seat ${idx + 1}`;

  const template = (): ReturnType<typeof html> => {
    const i = myIdx();
    const north = (i + 2) % 4;
    const west = (i + 3) % 4;
    const east = (i + 1) % 4;
    const teamA = i === 0 || i === 2 ? 'A' : 'B';
    const isMyTurn = store.currentPlayerId.value === store.playerId.value;

    const betButtons = (): ReturnType<typeof html> => {
      if (store.phase.value !== 'BETTING' || !isMyTurn) return html``;
      const onBet = async (amount: number): Promise<void> => {
        try {
          await request(`/games/${args.gameId}/transition`, {
            method: 'POST',
            body: JSON.stringify({ type: 'bet', amount }),
          });
        } catch (e) {
          console.error('bet failed', e);
        }
      };
      return html`<div class="spades-bets">
        ${Array.from({ length: 14 }, (_, n) =>
          button({ label: String(n), onClick: () => void onBet(n), variant: 'primary' }),
        )}
      </div>`;
    };

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

    const playAgain =
      store.phase.value === 'GAME_OVER'
        ? button({
            label: 'Play Again',
            onClick: () => {
              clearSession(store.playerId.value);
              navigateTo('/');
            },
            variant: 'primary',
          })
        : html``;

    return appShell(html`
      ${scores({
        teamAScore: store.teamAScore.value,
        teamBScore: store.teamBScore.value,
        teamABags: store.teamABags.value,
        teamBBags: store.teamBBags.value,
        myTeam: teamA,
        centerText:
          store.phase.value === 'PLAYING'
            ? `Trick ${
                typeof store.gameState.value === 'object' &&
                store.gameState.value &&
                'Trick' in store.gameState.value
                  ? (store.gameState.value as { Trick: number }).Trick
                  : 0
              }/13`
            : '',
      })}
      ${gameTable({
        north: {
          name: seatName(north),
          active: store.playerIds.value[north] === store.currentPlayerId.value,
          connected: store.playerConnected.value[north] ?? true,
          betInfo:
            store.playerBets.value[north] != null
              ? `Bet ${store.playerBets.value[north]} / Won ${store.playerTricksWon.value[north]}`
              : '',
          clockText: null,
        },
        west: {
          name: seatName(west),
          active: store.playerIds.value[west] === store.currentPlayerId.value,
          connected: store.playerConnected.value[west] ?? true,
          betInfo:
            store.playerBets.value[west] != null
              ? `Bet ${store.playerBets.value[west]} / Won ${store.playerTricksWon.value[west]}`
              : '',
          clockText: null,
        },
        east: {
          name: seatName(east),
          active: store.playerIds.value[east] === store.currentPlayerId.value,
          connected: store.playerConnected.value[east] ?? true,
          betInfo:
            store.playerBets.value[east] != null
              ? `Bet ${store.playerBets.value[east]} / Won ${store.playerTricksWon.value[east]}`
              : '',
          clockText: null,
        },
        south: {
          name: seatName(i),
          active: store.playerIds.value[i] === store.currentPlayerId.value,
          connected: store.playerConnected.value[i] ?? true,
          betInfo:
            store.playerBets.value[i] != null
              ? `Bet ${store.playerBets.value[i]} / Won ${store.playerTricksWon.value[i]}`
              : '',
          clockText: null,
        },
        centerText,
        refs,
      })}
      ${betButtons()} ${playAgain}
    `);
  };

  // Top-level effect: render the template
  const disposeRender = effect(() => render(template(), root));

  // After first render, set up the orchestrator with the refs
  const containers = {
    south: refs.hand.value!,
    north: refs.north.value!,
    west: refs.west.value!,
    east: refs.east.value!,
    trick: refs.trick.value!,
  };
  const orchestrator = new CardOrchestrator({ containers });
  args.resources.orchestrator = orchestrator;

  // Side-effect effect: keep orchestrator in sync
  const disposeCards = effect(() => {
    // Re-read everything so we depend on them
    const phase = store.phase.value;
    const hand = store.hand.value;
    const tableCards = store.tableCards.value;
    const currentPlayerId = store.currentPlayerId.value;
    const i = store.playerIds.value.indexOf(store.playerId.value);
    if (i < 0) return;
    if (phase !== 'BETTING' && phase !== 'PLAYING' && phase !== 'GAME_OVER') return;

    if (!orchestrator.isInitialized() && hand.length > 0) {
      orchestrator.setupImmediate({
        playerHand: sortCards(hand),
        oppCounts: {
          north: oppCardCount(phase, store.gameState.value, tableCards, (i + 2) % 4),
          west: oppCardCount(phase, store.gameState.value, tableCards, (i + 3) % 4),
          east: oppCardCount(phase, store.gameState.value, tableCards, (i + 1) % 4),
        },
        tableCards,
        myIdx: i,
        northIdx: (i + 2) % 4,
        westIdx: (i + 3) % 4,
        eastIdx: (i + 1) % 4,
        currentPlayerSeatIdx:
          store.playerIds.value.indexOf(currentPlayerId ?? '') >= 0
            ? store.playerIds.value.indexOf(currentPlayerId ?? '')
            : 0,
      });
    }

    if (orchestrator.isInitialized()) {
      orchestrator.updatePlayerHand(sortCards(hand));
      orchestrator.updateOpponentCount(
        'north',
        oppCardCount(phase, store.gameState.value, tableCards, (i + 2) % 4),
      );
      orchestrator.updateOpponentCount(
        'west',
        oppCardCount(phase, store.gameState.value, tableCards, (i + 3) % 4),
      );
      orchestrator.updateOpponentCount(
        'east',
        oppCardCount(phase, store.gameState.value, tableCards, (i + 1) % 4),
      );
    }

    // Interaction
    const isMyTurn = currentPlayerId === store.playerId.value;
    if (phase === 'PLAYING' && isMyTurn) {
      const leadSuit = (() => {
        // re-derive (helpers.getLeadSuit needs currentPlayerSeatIdx)
        const currentSeat = store.playerIds.value.indexOf(currentPlayerId ?? '');
        let n = 0;
        for (const c of tableCards) if (c && (c as { suit?: string }).suit !== 'Blank') n++;
        if (n === 0) return null;
        const leaderSeat = (((currentSeat - n) % 4) + 4) % 4;
        return tableCards[leaderSeat]?.suit ?? null;
      })();
      const validCards = sortCards(hand).filter((card) =>
        isCardValid({
          hand,
          leadSuit,
          spadesBroken: store.spadesBroken.value,
          card,
          isMyTurn: true,
          phase: 'PLAYING',
        }),
      );
      orchestrator.enableInteraction(validCards, (card) => {
        void (async () => {
          orchestrator.disableInteraction();
          await orchestrator.playCardToCenter(card);
          try {
            await request(`/games/${args.gameId}/transition`, {
              method: 'POST',
              body: JSON.stringify({ type: 'card', card }),
            });
          } catch (e) {
            console.error('play failed', e);
          }
        })();
      });
    } else {
      orchestrator.disableInteraction();
    }
  });

  args.resources.cleanups.push(disposeRender);
  args.resources.cleanups.push(disposeCards);
  return () => {};
}

async function pollOnce(store: GameStore, gameId: string, playerId: string): Promise<void> {
  try {
    const state = await request<never>(`/games/${gameId}`, { method: 'GET' });
    const hand = await request<never>(`/games/${gameId}/players/${playerId}/hand`, {
      method: 'GET',
    });
    store.applyState(state, hand);
    try {
      const presence = await request<{ players: never[] }>(`/games/${gameId}/presence`, {
        method: 'GET',
      });
      store.applyPresence(presence.players);
    } catch {
      // optional
    }
  } catch (e) {
    console.error('poll failed', e);
  }
}

export const play: RouteModule<{ shortId: string }> = {
  render: (params) => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    const resources: Resources = { cleanups: [], ws: null, pollTimer: null, orchestrator: null };

    let store: GameStore | null = null;
    let gameId = '';
    let playerId = '';

    // Pre-render a loading shell
    render(appShell(html`<p>Loading game…</p>`), root);

    void (async () => {
      // Special case: "/play/new-ai" boots an AI game synthetically.
      if (params.shortId === 'new-ai') {
        try {
          const ai = await startAIGame(null);
          gameId = ai.gameId;
          playerId = ai.playerId;
          saveSession(ai.shortId, ai.gameId, ai.playerId);
          navigateTo(`/play/${ai.shortId}`);
          // re-invoke this route with the new path; the router will handle it
          return;
        } catch (e) {
          render(appShell(html`<p>Failed to start AI game.</p>`), root);
          return;
        }
      }

      const result = await bootFromUrl(params.shortId, resources);
      if ('error' in result) {
        render(
          appShell(
            html`<p>${result.error}</p>
              <p><a href="/" data-link>Back home</a></p>`,
          ),
          root,
        );
        return;
      }
      store = result.store;
      gameId = result.gameId;
      playerId = result.playerId;

      const refs = makeRefs();
      renderInGame({ root, store, gameId, resources, refs });

      // Open WS; on close, fall back to polling
      resources.ws = openGameWs(gameId, playerId, {
        onEvent: (data) => {
          // For now, treat the WS payload as a fresh state snapshot.
          // Plan 2 Task 17 ("WS event handler") expands this to handle presence/aborted.
          const wsData = data as never;
          // Hand update is needed too; fetch lazily.
          void (async () => {
            try {
              const hand = await request<never>(`/games/${gameId}/players/${playerId}/hand`, {
                method: 'GET',
              });
              store!.applyState(wsData, hand);
            } catch {
              // ignore
            }
          })();
        },
        onClose: () => {
          if (store!.phase.value !== 'GAME_OVER') {
            resources.pollTimer = setInterval(
              () => void pollOnce(store!, gameId, playerId),
              POLL_INTERVAL,
            );
          }
        },
      });
    })();

    return () => disposeResources(resources);
  },
};
```

- [ ] **Step 5: Wire `play` route in `src/main.ts`**

Replace the routes object:

```ts
import { play } from './routes/play';
// ...
const router = createRouter({
  '/': home,
  '/play/:shortId': play,
  '*': notFound,
});
```

- [ ] **Step 6: Wire "Play with Computers" in `src/routes/home.ts`**

Replace `onComputers`:

```ts
function onComputers(): void {
  navigateTo('/play/new-ai');
}
```

Add `import { navigateTo } from '../lib/util';` at the top.

- [ ] **Step 7: Manual smoke**

Start rust-spades on :3000 with `--insecure-cookies --cors-allow-origin http://localhost:5173`.
Run: `pnpm dev`
Click "Play with Computers"; verify a game starts, you can bet, play a card.

- [ ] **Step 8: Commit**

```bash
git add src/routes/play.ts src/main.ts src/routes/home.ts src/ui/components/scores.ts src/ui/components/game-table.ts src/ui/design.css
git commit -m "feat: play route + Play with Computers end-to-end"
```

---

## Task 15: Quickplay (matchmaking SSE)

**Files:**

- Modify: `src/routes/home.ts`, `src/state/menu.ts` (already covered)
- Create: `src/routes/play.ts` extension for `WAITING` phase (in-line)

- [ ] **Step 1: Wire quickplay buttons in `src/routes/home.ts`**

Replace `onSeek`:

```ts
import { openSse, type SseHandle } from '../api/sse';
import { saveSession } from '../lib/storage';

let activeSeek: SseHandle | null = null;

function onSeek(timer: TimerCfg): void {
  if (activeSeek) return;
  // Show waiting overlay
  const waitEl = document.getElementById('quickplay-status')!;
  waitEl.textContent = 'Finding players…';
  activeSeek = openSse(
    '/matchmaking/seek',
    { max_points: 500, timer_config: timer },
    {
      onEvent: (eventType, data) => {
        try {
          const parsed = JSON.parse(data);
          if (eventType === 'queue_status') {
            waitEl.textContent = `Finding players… (${parsed.waiting}/4)`;
          } else if (eventType === 'game_start') {
            const shortId = parsed.short_id ?? parsed.player_url ?? parsed.game_id;
            saveSession(shortId, parsed.game_id, parsed.player_short_id ?? parsed.player_id);
            activeSeek?.close();
            activeSeek = null;
            navigateTo(`/play/${shortId}`);
          }
        } catch {
          // ignore parse errors
        }
      },
      onError: () => {
        waitEl.textContent = 'Failed to find match.';
        activeSeek?.close();
        activeSeek = null;
      },
    },
  );
}
```

- [ ] **Step 2: Add the status element to the home template**

In `template()`, add below the `.menu`:

```ts
html`<p id="quickplay-status" class="menu__label"></p>`;
```

- [ ] **Step 3: Add a Cancel handler**

After the status element:

```ts
html`<button
  type="button"
  class="btn btn--secondary"
  style=${activeSeek ? '' : 'display: none'}
  @click=${() => {
    activeSeek?.close();
    activeSeek = null;
    document.getElementById('quickplay-status')!.textContent = '';
  }}
>
  Cancel
</button>`;
```

(Since `home.ts` is currently stateless and re-renders per route mount, the simplest fix is to keep `activeSeek` at module scope and re-render when it changes. For brevity, you can also keep the status text as the only signal of "in queue" and skip a dedicated button — the user just navigates away to cancel via SSE drop guard.)

- [ ] **Step 4: Manual smoke**

Open two browser tabs, click the same Quickplay button on each; with two more clients (or by running 2 more contexts via Playwright), confirm `game_start` fires and both navigate to `/play/:shortId`.

- [ ] **Step 5: Commit**

```bash
git add src/routes/home.ts
git commit -m "feat: quickplay via matchmaking SSE"
```

---

## Task 16: Friends — create challenge + lobby + join

**Files:**

- Create: `src/routes/create.ts` (challenge creation form), and `src/routes/play.ts` already handles lobby via the "challenge open" branch — extend it.

This task is substantial; treat it as four sub-commits.

- [ ] **Step 1: Create `src/routes/create.ts` with the create form**

This is a route at `/create`. Form: name, seat picker (A/B/C/D + None), points (200/300/500), timer (None / 5+3 / 10+5 / 15+10). Submit → `POST /challenges` SSE → on `challenge_created` → `navigateTo('/play/' + short_id)`.

```ts
import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { openSse, type SseHandle } from '../api/sse';
import { navigateTo, API_URL as _API_URL } from '../lib/util';
import { saveSession } from '../lib/storage';
import type { RouteModule } from '../router';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;
const TIMER_PRESETS: { label: string; value: TimerCfg }[] = [
  { label: 'None', value: null },
  { label: '5+3', value: { initial_time_secs: 300, increment_secs: 3 } },
  { label: '10+5', value: { initial_time_secs: 600, increment_secs: 5 } },
  { label: '15+10', value: { initial_time_secs: 900, increment_secs: 10 } },
];

export const create: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    let sse: SseHandle | null = null;

    let name = '';
    let seat: 'A' | 'B' | 'C' | 'D' | null = null;
    let points = 500;
    let timerIdx = 0;
    let errorMsg = '';

    const template = (): ReturnType<typeof html> =>
      appShell(html`
        <h2>Create Challenge</h2>
        ${errorMsg ? html`<p style="color: var(--color-danger)">${errorMsg}</p>` : null}
        <label
          >Your name
          <input
            type="text"
            maxlength="20"
            @input=${(e: Event) => {
              name = (e.target as HTMLInputElement).value;
            }}
        /></label>
        <fieldset>
          <legend>Pick seat</legend>
          ${(['A', 'B', 'C', 'D'] as const).map((s) =>
            button({
              label: `Seat ${s}`,
              onClick: () => {
                seat = seat === s ? null : s;
                rerender();
              },
              variant: seat === s ? 'primary' : 'secondary',
            }),
          )}
        </fieldset>
        <fieldset>
          <legend>Points</legend>
          ${[200, 300, 500].map((p) =>
            button({
              label: String(p),
              onClick: () => {
                points = p;
                rerender();
              },
              variant: points === p ? 'primary' : 'secondary',
            }),
          )}
        </fieldset>
        <fieldset>
          <legend>Timer</legend>
          ${TIMER_PRESETS.map((t, i) =>
            button({
              label: t.label,
              onClick: () => {
                timerIdx = i;
                rerender();
              },
              variant: timerIdx === i ? 'primary' : 'secondary',
            }),
          )}
        </fieldset>
        ${button({
          label: 'Create',
          onClick: () => {
            errorMsg = '';
            sse = openSse(
              '/challenges',
              {
                max_points: points,
                creator_name: name || undefined,
                creator_seat: seat ?? undefined,
                timer_config: TIMER_PRESETS[timerIdx]!.value ?? undefined,
              },
              {
                onEvent: (type, data) => {
                  try {
                    const parsed = JSON.parse(data);
                    if (type === 'challenge_created') {
                      saveSession(
                        parsed.short_id,
                        parsed.challenge_id,
                        parsed.creator_player_id ?? '',
                      );
                      sse?.close();
                      sse = null;
                      navigateTo(`/play/${parsed.short_id}`);
                    } else if (type === 'cancelled') {
                      errorMsg = 'Challenge cancelled.';
                      rerender();
                    }
                  } catch {
                    // ignore
                  }
                },
                onError: () => {
                  errorMsg = 'Failed to create challenge.';
                  rerender();
                },
              },
            );
            rerender();
          },
          variant: 'primary',
        })}
      `);

    const rerender = (): void => render(template(), root);
    rerender();

    return () => {
      sse?.close();
    };
  },
};
```

- [ ] **Step 2: Register the `/create` route**

In `src/main.ts`:

```ts
import { create } from './routes/create';
// ...
const router = createRouter({
  '/': home,
  '/create': create,
  '/play/:shortId': play,
  '*': notFound,
});
```

- [ ] **Step 3: Wire "Play with Friends" in `src/routes/home.ts`**

```ts
function onFriends(): void {
  navigateTo('/create');
}
```

- [ ] **Step 4: Extend `src/routes/play.ts` to render a lobby when boot finds a challenge**

In `bootFromUrl`, when status is `open`, return the challenge state rather than an error. Add a lobby renderer that:

- Shows the four seat slots (open / taken / mine)
- Provides a join modal for open seats
- Subscribes to `/challenges/:id/join/:seat` SSE for joining and `seat_update` events
- Renders the share link `${origin}/play/${shortId}`
- Cancel button for creator (DELETE /challenges/:id)
- On `game_start`, transition to `renderInGame`.

This is mechanically a translation of `index.html` lines 84-131 (template) + 1031-1100 (`joinChallenge`/`handleJoinSSE`/`cancelChallenge`). Faithful port — preserve seat-A/B/C/D mapping and team labels. Keep the lobby renderer inside `play.ts` (consistent with the design's "lobby is a sub-render" decision).

The full ported listing for the lobby is too long to inline here verbatim; the engineer should port it methodically using these rules:

- Replace `x-show` / `x-for` with conditional `html` and `.map`.
- Replace `@click` with `@click=${...}` (lit-html event binding).
- Replace `this.X` with `store.X.value` for reactive bits; non-reactive UI state (joinNameValue, joiningSeat) lives as local `let` in the route, with `rerender()` on changes.
- The SSE event types match exactly (`joined`, `seat_update`, `game_start`, `cancelled`).

The tests in Task 19 (E2E friends) pin behavior.

- [ ] **Step 5: Commit (4 commits as you go)**

```bash
git add src/routes/create.ts src/main.ts src/routes/home.ts
git commit -m "feat: create challenge route + SSE flow"

# After the lobby renderer is in:
git add src/routes/play.ts
git commit -m "feat: challenge lobby + join SSE in play route"
```

---

## Task 17: WS event handler (presence + game_aborted)

**Files:**

- Modify: `src/routes/play.ts`

Today's WS payloads include `event: 'presence_changed'` and `event: 'game_aborted'` in addition to `StateChanged` snapshots. Handle them.

- [ ] **Step 1: Replace the WS `onEvent` callback in `play.ts`**

```ts
onEvent: (data) => {
  const obj = data as { event?: string; reason?: string; players?: { player_id: string; connected: boolean }[] } & Record<string, unknown>;
  if (obj.event === 'presence_changed' && obj.players) {
    store!.applyPresence(obj.players);
    return;
  }
  if (obj.event === 'game_aborted') {
    console.warn('game aborted:', obj.reason);
    resources.orchestrator?.clearAll();
    clearSession(params.shortId);
    navigateTo('/');
    return;
  }
  // Otherwise: state snapshot. Fetch hand and apply.
  void (async () => {
    try {
      const hand = await request<never>(
        `/games/${gameId}/players/${playerId}/hand`,
        { method: 'GET' },
      );
      store!.applyState(data as never, hand);
    } catch {
      // ignore
    }
  })();
},
```

- [ ] **Step 2: Commit**

```bash
git add src/routes/play.ts
git commit -m "feat: WS handler covers presence + game_aborted"
```

---

## Task 18: E2E — anonymous AI happy path + reload reconnect

**Files:**

- Create: `tests/e2e/ai-game.spec.ts`

Each E2E test in this plan needs `rust-spades` running. CI runs it from a pinned git sha; locally, the test assumes it's on `http://localhost:3000`. The `playwright.config.ts` already starts the frontend dev server.

- [ ] **Step 1: Add a Playwright fixture/setup for the API check**

Update `playwright.config.ts` `webServer` block — leave Vite as-is. Add a setup file:

`tests/e2e/setup.ts`:

```ts
import { test as base } from '@playwright/test';

export const test = base.extend<{ apiUp: void }>({
  apiUp: [
    async ({}, use) => {
      const url = process.env.VITE_API_URL ?? 'http://localhost:3000';
      const res = await fetch(`${url}/games`).catch(() => null);
      if (!res || !res.ok) throw new Error(`rust-spades not reachable at ${url}/games`);
      await use();
    },
    { auto: true },
  ],
});
export { expect } from '@playwright/test';
```

- [ ] **Step 2: Write the test**

```ts
import { test, expect } from './setup';

test('anonymous AI game — bet and play a card', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Play with Computers' }).click();

  // Wait for navigation to /play/:shortId
  await page.waitForURL(/\/play\/[^/]+$/);

  // BETTING phase should show bet buttons.
  await page.waitForSelector('.spades-bets button', { state: 'visible', timeout: 10_000 });
  await page.locator('.spades-bets button').nth(3).click(); // bet 3

  // Wait for PLAYING phase (cards become clickable).
  await page.waitForSelector('.cm-clickable', { state: 'visible', timeout: 10_000 });

  // Reload should preserve game state.
  await page.reload();
  await expect(page.locator('.hand-container .card')).toHaveCount(13, { timeout: 5_000 });
});
```

- [ ] **Step 3: Run it**

Run (with rust-spades running): `pnpm test:e2e -- ai-game`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e/ai-game.spec.ts tests/e2e/setup.ts
git commit -m "test: e2e AI game happy path + reconnect"
```

---

## Task 19: E2E — quickplay 4-context match and friends challenge

**Files:**

- Create: `tests/e2e/quickplay.spec.ts`, `tests/e2e/friends.spec.ts`

- [ ] **Step 1: `tests/e2e/quickplay.spec.ts`**

```ts
import { test, expect } from './setup';

test('four players matched via quickplay', async ({ browser }) => {
  const contexts = await Promise.all([
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
  ]);
  const pages = await Promise.all(contexts.map((c) => c.newPage()));

  try {
    await Promise.all(pages.map((p) => p.goto('/')));
    await Promise.all(pages.map((p) => p.getByRole('button', { name: '5+3' }).click()));
    // All four should reach a /play/:shortId URL within a few seconds.
    await Promise.all(pages.map((p) => p.waitForURL(/\/play\/[^/]+$/, { timeout: 10_000 })));
    // And reach BETTING phase.
    await Promise.all(
      pages.map((p) =>
        expect(p.locator('.spades-bets, .hand-container')).toBeVisible({ timeout: 5_000 }),
      ),
    );
  } finally {
    await Promise.all(contexts.map((c) => c.close()));
  }
});
```

- [ ] **Step 2: `tests/e2e/friends.spec.ts`**

```ts
import { test, expect } from './setup';

test('create + join via friends challenge', async ({ browser }) => {
  const ctxs = await Promise.all([
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
  ]);
  const pages = await Promise.all(ctxs.map((c) => c.newPage()));
  const [creator, ...joiners] = pages;

  try {
    await creator.goto('/');
    await creator.getByRole('button', { name: 'Play with Friends' }).click();
    // Pick seat A and submit
    await creator.getByRole('button', { name: 'Seat A' }).click();
    await creator.getByRole('button', { name: 'Create' }).click();
    // Wait for navigation; capture the share link
    await creator.waitForURL(/\/play\/[^/]+$/);
    const shareUrl = creator.url();

    // Three joiners navigate to the same URL and join different seats.
    for (let i = 0; i < 3; i++) {
      await joiners[i]!.goto(shareUrl);
      // The lobby exposes seat buttons; pick the first open one.
      await joiners[i]!.locator('.seat-option:not(.taken)').first().click();
      await joiners[i]!.getByRole('button', { name: 'Join' }).click();
    }

    // Everyone reaches BETTING.
    await Promise.all(
      pages.map((p) =>
        expect(p.locator('.spades-bets, .hand-container')).toBeVisible({ timeout: 10_000 }),
      ),
    );
  } finally {
    await Promise.all(ctxs.map((c) => c.close()));
  }
});
```

- [ ] **Step 3: Run them**

Run: `pnpm test:e2e -- quickplay friends`
Expected: both pass.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e/quickplay.spec.ts tests/e2e/friends.spec.ts
git commit -m "test: e2e quickplay + friends"
```

---

## Self-review

**Spec coverage (Phase 2 of the design doc):**

- `state/helpers.ts` → Task 5 ✓
- Card layer (`cards/*`) → Tasks 6-10 ✓
- `api/{client,sse,ws}.ts` → Tasks 2-4 ✓
- `state/game.ts` → Task 12 ✓
- `state/menu.ts` → Task 13 ✓
- `routes/play.ts` reconnect chain + in-game + lobby → Tasks 14, 16, 17 ✓
- Play with Computers → Task 14 ✓
- Quickplay (SSE) → Task 15 ✓
- Friends (challenge SSE create + join) → Task 16 ✓
- E2E AI happy path + reconnect → Task 18 ✓
- E2E quickplay + friends → Task 19 ✓

**Out of scope, deferred to Plan 3:** session store, login/signup, settings, profile, OAuth.

**Placeholder scan:**

- Task 16 Step 4 says "The full ported listing for the lobby is too long to inline here verbatim" — this is the one deliberate exception, with explicit guidance for the porter (rules + source line ranges + behavior pinned by E2E in Task 19). All other tasks have full code.
- "Plan 2 Task 17 ('WS event handler') expands this" comment in Task 14 — points forward, not a placeholder.

**Type consistency:**

- `Seat = 'south' | 'north' | 'east' | 'west'` — used identically in `hand-manager.ts`, `trick-manager.ts`, `orchestrator.ts`.
- `RelativeSeat` (helpers) ≡ `Seat` (cards) — same string union. Two names are intentional: `helpers.ts` doesn't import from `cards/`, so they're kept structural.
- `Card`, `Phase`, `Suit`, `Rank` defined once in `state/helpers.ts`; re-exported by `state/game.ts` indirectly via TypeScript's structural typing.
- `RouteModule.render` signature in `home`, `play`, `notfound`, `create` all return `() => void`.

**Open caveats for the reviewer:**

- Task 14 includes a fair bit of inline logic (~250 LOC). Splitting into smaller routes is possible but would multiply boilerplate; I kept it together because state is shared.
- The challenge lobby ported in Task 16 Step 4 is the one place this plan defers code volume to the implementer. The pattern is mechanical and the E2E covers it; if you want every line spelled out, that's a 5-commit task on its own (~300 lines of template + handlers).
- I did not write a separate component test for the orchestrator. happy-dom's lack of real layout + the rAF mocking complexity make it more cost than benefit; E2E covers it.
- The hand-update inside the WS handler currently does an extra `fetch /hand` per event. That matches today's behavior. If the rust-spades patch exposes per-player hand in the WS payload (post-Phase 0), drop the extra request — single-line change in Task 14 Step 4.
