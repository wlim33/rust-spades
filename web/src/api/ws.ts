import { API_URL } from '../lib/util';

export type WsHandle = { close(): void };

export type WsOptions = {
  /**
   * May return a promise: the event queue awaits it, so per-event work is
   * serialized. `isSnapshot` is true for the first message after each (re)open
   * — the server's initial state snapshot, whose seq is the cursor the next
   * streamed event will carry (so the consumer can seed without dropping it).
   */
  onEvent: (data: unknown, isSnapshot: boolean) => void | Promise<void>;
  onOpen?: () => void;
  onClose?: () => void;
  onError?: (e: unknown) => void;
};

const MAX_RECONNECT_ATTEMPTS = 6;
const BASE_RECONNECT_MS = 500;
const MAX_RECONNECT_MS = 15_000;

/** Capped exponential backoff with full jitter. */
function reconnectDelay(attempt: number): number {
  const ceiling = Math.min(MAX_RECONNECT_MS, BASE_RECONNECT_MS * 2 ** attempt);
  return Math.random() * ceiling;
}

/**
 * Connects to /games/:gameId/ws?player_id=:playerId and keeps the connection
 * alive: if the socket drops unexpectedly it reconnects with capped exponential
 * backoff (full jitter). `onClose` fires only once the reconnection attempts are
 * exhausted or the caller closes the handle — at which point the caller may fall
 * back to polling. A successful reconnect resets the backoff.
 *
 * Maintains an internal async queue so consumers can `await` per-event work
 * without dropping subsequent messages.
 */
export function openGameWs(gameId: string, playerId: string | null, opts: WsOptions): WsHandle {
  const wsBase =
    API_URL ||
    (typeof location !== 'undefined'
      ? `${location.protocol.replace('http', 'ws')}//${location.host}`
      : '');
  const wsUrl = `${wsBase.replace(/^https/, 'wss').replace(/^http/, 'ws')}/games/${encodeURIComponent(
    gameId,
  )}/ws${playerId ? `?player_id=${encodeURIComponent(playerId)}` : ''}`;

  const queue: Array<{ data: unknown; snapshot: boolean }> = [];
  let draining = false;
  let closed = false;
  let attempts = 0;
  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  // The first message after each (re)open is the server's initial snapshot.
  let expectSnapshot = false;

  const drain = async (): Promise<void> => {
    if (draining) return;
    draining = true;
    try {
      while (queue.length > 0) {
        const item = queue.shift()!;
        try {
          await opts.onEvent(item.data, item.snapshot);
        } catch (e) {
          opts.onError?.(e);
        }
      }
    } finally {
      draining = false;
    }
  };

  const connect = (): void => {
    ws = new WebSocket(wsUrl);
    ws.onmessage = (evt) => {
      try {
        const data = JSON.parse(evt.data as string);
        const snapshot = expectSnapshot;
        expectSnapshot = false;
        queue.push({ data, snapshot });
        void drain();
      } catch (e) {
        opts.onError?.(e);
      }
    };
    ws.onopen = () => {
      attempts = 0;
      // The server sends the state snapshot first; tag it so the consumer can
      // seed its seq cursor without dropping the first streamed event.
      expectSnapshot = true;
      opts.onOpen?.();
    };
    ws.onclose = () => {
      if (closed) return;
      if (attempts < MAX_RECONNECT_ATTEMPTS) {
        reconnectTimer = setTimeout(connect, reconnectDelay(attempts));
        attempts++;
      } else {
        opts.onClose?.();
      }
    };
    ws.onerror = (e) => opts.onError?.(e);
  };

  // The server sends no app-level heartbeat, so a half-open socket (e.g. after a
  // network switch or device sleep) won't fire `onclose` and the game would
  // freeze with no recovery. We can't detect a dead-but-OPEN socket passively,
  // so proactively reconnect on the browser signals that usually accompany one.
  const forceReconnect = (): void => {
    if (closed) return;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    attempts = 0;
    // Detach handlers so the stale socket's eventual close can't also schedule a
    // reconnect — we're starting a fresh one right now.
    if (ws) {
      ws.onopen = ws.onclose = ws.onmessage = ws.onerror = null;
      try {
        ws.close();
      } catch {
        // already closing
      }
    }
    connect();
  };

  let hiddenAt = 0;
  const onOnline = (): void => forceReconnect();
  const onVisibility = (): void => {
    if (typeof document === 'undefined') return;
    if (document.visibilityState === 'hidden') {
      hiddenAt = Date.now();
      return;
    }
    // Back in the foreground: reconnect if the socket isn't healthy, or if we
    // were hidden long enough that it likely died silently during sleep.
    const wasHiddenLong = hiddenAt > 0 && Date.now() - hiddenAt > 10_000;
    hiddenAt = 0;
    if (!ws || ws.readyState !== WebSocket.OPEN || wasHiddenLong) forceReconnect();
  };

  const hasWindow = typeof window !== 'undefined';
  const hasDocument = typeof document !== 'undefined';
  if (hasWindow) window.addEventListener('online', onOnline);
  if (hasDocument) document.addEventListener('visibilitychange', onVisibility);

  connect();

  return {
    close: () => {
      if (closed) return;
      closed = true;
      if (hasWindow) window.removeEventListener('online', onOnline);
      if (hasDocument) document.removeEventListener('visibilitychange', onVisibility);
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = null;
      try {
        ws?.close();
      } catch {
        // already closed
      }
    },
  };
}
