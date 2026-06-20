/**
 * Local-only network chaos harness. NEVER ships: it is dynamically imported by
 * main.ts solely under `import.meta.env.DEV`, so Vite tree-shakes it out of the
 * production bundle entirely.
 *
 * It monkeypatches `globalThis.fetch` and `globalThis.WebSocket` to inject the
 * connection faults that surface gameplay-animation choppiness:
 *   1. Latency + jitter   — delays every API fetch and every inbound WS frame
 *   2. Packet loss/reorder — drops and shuffles inbound WS frames (seq guard)
 *   3. Hard disconnects    — force-closes live sockets mid-trick
 *   4. Half-open sockets   — socket stays OPEN but delivers nothing
 *
 * Activation: append `?chaos=1` (optionally with tuning params) to any /play
 * URL, or call `chaos.config({...})` from the console. Drive an AI game and
 * read `chaos.report()` — `handGapMs` is the table's animation cadence (gap
 * between consecutive /hand-fetch completions); ballooning/irregular gaps are
 * the choppiness, quantified.
 */

export type ChaosConfig = {
  /** Base latency added to every matching API fetch, ms. */
  httpLatencyMs: number;
  /** Uniform +/- jitter on top of httpLatencyMs, ms. */
  httpJitterMs: number;
  /** Probability [0..1] a matching fetch is failed outright (network error). */
  httpFailRate: number;
  /** Base delay added before delivering an inbound WS frame, ms. */
  wsLatencyMs: number;
  /** Uniform +/- jitter on top of wsLatencyMs, ms. */
  wsJitterMs: number;
  /** Probability [0..1] an inbound WS frame is dropped. */
  wsDropRate: number;
  /** Probability [0..1] an inbound WS frame is held back so a later one overtakes it. */
  wsReorderRate: number;
};

const DEFAULTS: ChaosConfig = {
  httpLatencyMs: 150,
  httpJitterMs: 80,
  httpFailRate: 0,
  wsLatencyMs: 60,
  wsJitterMs: 40,
  wsDropRate: 0,
  wsReorderRate: 0,
};

let cfg: ChaosConfig = { ...DEFAULTS };

type Stats = {
  fetches: number;
  fetchFails: number;
  wsFramesIn: number;
  wsDropped: number;
  wsReordered: number;
  wsDelayedMs: number;
  forcedDisconnects: number;
  halfOpenWindows: number;
  /** Completion timestamps of /hand fetches — consecutive gaps == animation cadence. */
  handCompletions: number[];
};

const stats: Stats = {
  fetches: 0,
  fetchFails: 0,
  wsFramesIn: 0,
  wsDropped: 0,
  wsReordered: 0,
  wsDelayedMs: 0,
  forcedDisconnects: 0,
  halfOpenWindows: 0,
  handCompletions: [],
};

const rng = (): number => Math.random();
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
const jitter = (base: number, spread: number): number =>
  Math.max(0, base + (rng() * 2 - 1) * spread);

/**
 * ───────────────────────────────────────────────────────────────────────────
 * YOUR CONTRIBUTION — the per-frame WS fault policy.
 * ───────────────────────────────────────────────────────────────────────────
 *
 * This decides the fate of ONE inbound WebSocket frame: drop it, deliver it
 * after some delay, or hold it so a later frame overtakes it (reorder). This is
 * where the loss *model* lives, and the model changes what bug you reproduce:
 *
 *   - Independent (Bernoulli) loss: each frame coin-flips against wsDropRate.
 *     Simple; models a steadily lossy link.
 *   - Bursty (Gilbert-Elliott) loss: the link flips between "good" and "bad"
 *     states and drops happen in clusters. Far more realistic for mobile /
 *     wifi, and far harsher on the seq guard + completeTrick backfill, because
 *     it knocks out *consecutive* plays of a single trick.
 *   - Jitter distribution: uniform (current `jitter` helper) is mild; a
 *     long-tailed delay (occasional big spike) is what actually overtakes
 *     earlier frames and exercises out-of-order delivery.
 *
 * Return `{ drop: true }` to discard the frame, or `{ drop: false, delayMs }`
 * for how long to hold it before delivery (reorder emerges when one frame's
 * delayMs exceeds a later frame's). Use `cfg`, `rng()`, and the helpers above.
 *
 * Keep it ~5-10 lines. Start with independent loss + jittered delay; reach for
 * Gilbert-Elliott if you want to stress the backfill path (fault mode #2).
 */
function decideWsFate(): { drop: boolean; delayMs: number } {
  // Independent (Bernoulli) model: each frame coin-flips on its own, no memory.
  if (cfg.wsDropRate > 0 && rng() < cfg.wsDropRate) return { drop: true, delayMs: 0 };
  // Reorder = a rare long-tail delay so a normally-delayed later frame overtakes
  // this one (the base jittered delay is added on top in handleFrame).
  if (cfg.wsReorderRate > 0 && rng() < cfg.wsReorderRate) {
    return { drop: false, delayMs: jitter(cfg.wsLatencyMs * 4, cfg.wsLatencyMs * 2) };
  }
  return { drop: false, delayMs: 0 };
}

// ── HTTP: wrap global fetch ────────────────────────────────────────────────

let realFetch: typeof fetch | null = null;

function isApiUrl(url: string): boolean {
  // Same-origin API calls and the dev API_URL both hit these paths.
  return /\/(games|auth|matchmaking|challenges|leaderboard)\b/.test(url) || /\/hand\b/.test(url);
}

function installFetch(): void {
  if (realFetch) return;
  realFetch = globalThis.fetch.bind(globalThis);
  globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    if (!isApiUrl(url)) return realFetch!(input, init);

    stats.fetches++;
    await sleep(jitter(cfg.httpLatencyMs, cfg.httpJitterMs));
    if (cfg.httpFailRate > 0 && rng() < cfg.httpFailRate) {
      stats.fetchFails++;
      throw new TypeError('chaos: simulated network failure');
    }
    const res = await realFetch!(input, init);
    if (/\/hand\b/.test(url)) stats.handCompletions.push(performance.now());
    return res;
  };
}

// ── WS: wrap global WebSocket ──────────────────────────────────────────────

const liveSockets = new Set<ChaosWebSocket>();

/**
 * Drop-in WebSocket replacement. Presents exactly the surface ws.ts uses
 * (onopen/onmessage/onclose/onerror, readyState, close, static OPEN) while a
 * delivery scheduler applies per-frame faults to inbound messages. Out-of-order
 * delivery emerges naturally from differing per-frame delays.
 */
class ChaosWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  onopen: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;

  private inner: WebSocket;
  /** When true, the socket is OPEN but we swallow every frame (half-open). */
  private muted = false;

  constructor(url: string | URL, protocols?: string | string[]) {
    this.inner = new realWebSocket!(url, protocols);
    liveSockets.add(this);
    this.inner.onopen = (ev) => this.onopen?.(ev);
    this.inner.onerror = (ev) => this.onerror?.(ev);
    this.inner.onclose = (ev) => {
      liveSockets.delete(this);
      this.onclose?.(ev);
    };
    this.inner.onmessage = (ev) => this.handleFrame(ev);
  }

  private handleFrame(ev: MessageEvent): void {
    if (this.muted) return;
    stats.wsFramesIn++;
    const fate = decideWsFate();
    if (fate.drop) {
      stats.wsDropped++;
      return;
    }
    // One delay, used for both the stat and the actual dispatch, so the report
    // reflects what really happened. Reorder emerges when this exceeds a later
    // frame's delay; fate.delayMs (from the policy) is the deliberate lever.
    const delay = jitter(cfg.wsLatencyMs, cfg.wsJitterMs) + fate.delayMs;
    if (delay > 0) stats.wsDelayedMs += delay;
    if (fate.delayMs > 0) stats.wsReordered++;
    setTimeout(() => {
      if (this.muted) return;
      this.onmessage?.(ev);
    }, delay);
  }

  /** Force-close as if the network dropped (fault mode #3). */
  forceClose(): void {
    stats.forcedDisconnects++;
    try {
      this.inner.close();
    } catch {
      // already closing
    }
  }

  /** Keep the socket OPEN but stop delivering frames (fault mode #4). */
  setMuted(m: boolean): void {
    if (m && !this.muted) stats.halfOpenWindows++;
    this.muted = m;
  }

  get readyState(): number {
    return this.inner.readyState;
  }
  get url(): string {
    return this.inner.url;
  }
  send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
    this.inner.send(data as never);
  }
  close(code?: number, reason?: string): void {
    liveSockets.delete(this);
    this.inner.close(code, reason);
  }
  addEventListener(): void {
    /* ws.ts uses onX handlers only; no-op for API completeness */
  }
  removeEventListener(): void {
    /* no-op */
  }
}

let realWebSocket: typeof WebSocket | null = null;

function installWebSocket(): void {
  if (realWebSocket) return;
  realWebSocket = globalThis.WebSocket;
  // ChaosWebSocket presents the surface ws.ts depends on; cast through unknown.
  globalThis.WebSocket = ChaosWebSocket as unknown as typeof WebSocket;
}

// ── Public control API (window.chaos) ──────────────────────────────────────

function quantile(sorted: number[], q: number): number {
  if (sorted.length === 0) return 0;
  const i = Math.min(sorted.length - 1, Math.floor(q * sorted.length));
  return sorted[i]!;
}

export const chaos = {
  /** Merge partial config over the current settings. */
  config(next: Partial<ChaosConfig>): ChaosConfig {
    cfg = { ...cfg, ...next };
    console.info('[chaos] config', cfg);
    return cfg;
  },
  /** Force-close every live socket now (one hard disconnect each). */
  disconnect(): void {
    for (const s of liveSockets) s.forceClose();
  },
  /** Mute all live sockets for `ms`, then unmute — a half-open window. */
  halfOpen(ms = 12_000): void {
    const targets = [...liveSockets];
    for (const s of targets) s.setMuted(true);
    setTimeout(() => {
      for (const s of targets) s.setMuted(false);
    }, ms);
  },
  /** Summary stats. `handGapMs` is the table animation cadence. */
  report(): Record<string, unknown> {
    const gaps: number[] = [];
    const t = stats.handCompletions;
    for (let i = 1; i < t.length; i++) gaps.push(t[i]! - t[i - 1]!);
    gaps.sort((a, b) => a - b);
    const mean = gaps.length ? gaps.reduce((a, b) => a + b, 0) / gaps.length : 0;
    return {
      config: cfg,
      fetches: stats.fetches,
      fetchFails: stats.fetchFails,
      // Hand fetches gate the event pipeline; one per trick is healthy, one per
      // play (≈4×) is the redundant-fetch regression that caused the choppiness.
      handFetches: stats.handCompletions.length,
      wsFramesIn: stats.wsFramesIn,
      wsDropped: stats.wsDropped,
      wsReordered: stats.wsReordered,
      forcedDisconnects: stats.forcedDisconnects,
      halfOpenWindows: stats.halfOpenWindows,
      handGapMs: {
        count: gaps.length,
        mean: Math.round(mean),
        p50: Math.round(quantile(gaps, 0.5)),
        p95: Math.round(quantile(gaps, 0.95)),
        max: Math.round(gaps[gaps.length - 1] ?? 0),
      },
    };
  },
  /** Reset counters (config is kept). */
  reset(): void {
    Object.assign(stats, {
      fetches: 0,
      fetchFails: 0,
      wsFramesIn: 0,
      wsDropped: 0,
      wsReordered: 0,
      wsDelayedMs: 0,
      forcedDisconnects: 0,
      halfOpenWindows: 0,
      handCompletions: [],
    });
  },
};

/** Parse `?chaos=...&lat=..&jit=..&wsdrop=..&wsreorder=..` into config overrides. */
function configFromUrl(): Partial<ChaosConfig> | null {
  if (typeof location === 'undefined') return null;
  const p = new URLSearchParams(location.search);
  if (!p.has('chaos')) return null;
  const num = (k: string): number | undefined => (p.has(k) ? Number(p.get(k)) : undefined);
  const out: Partial<ChaosConfig> = {};
  const lat = num('lat');
  if (lat !== undefined) out.httpLatencyMs = lat;
  const jit = num('jit');
  if (jit !== undefined) out.httpJitterMs = jit;
  const fail = num('fail');
  if (fail !== undefined) out.httpFailRate = fail;
  const wslat = num('wslat');
  if (wslat !== undefined) out.wsLatencyMs = wslat;
  const wsdrop = num('wsdrop');
  if (wsdrop !== undefined) out.wsDropRate = wsdrop;
  const wsreorder = num('wsreorder');
  if (wsreorder !== undefined) out.wsReorderRate = wsreorder;
  return out;
}

/**
 * Install the wrappers. Idempotent. Returns true if chaos is active. Call this
 * once, early in boot, only under import.meta.env.DEV.
 */
export function installChaos(force = false): boolean {
  const fromUrl = configFromUrl();
  if (!fromUrl && !force) return false;
  if (fromUrl) cfg = { ...cfg, ...fromUrl };
  installFetch();
  installWebSocket();
  (globalThis as Record<string, unknown>)['chaos'] = chaos;
  console.info(
    '%c[chaos] active — drive a game, then call chaos.report()',
    'color:#e0b341;font-weight:bold',
    cfg,
  );
  return true;
}
