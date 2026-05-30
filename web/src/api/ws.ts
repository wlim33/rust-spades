import { API_URL } from '../lib/util';

export type WsHandle = { close(): void };

export type WsOptions = {
  onEvent: (data: unknown) => void;
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

  const queue: unknown[] = [];
  let draining = false;
  let closed = false;
  let attempts = 0;
  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

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

  const connect = (): void => {
    ws = new WebSocket(wsUrl);
    ws.onmessage = (evt) => {
      try {
        const data = JSON.parse(evt.data as string);
        queue.push(data);
        void drain();
      } catch (e) {
        opts.onError?.(e);
      }
    };
    ws.onopen = () => {
      attempts = 0;
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

  connect();

  return {
    close: () => {
      if (closed) return;
      closed = true;
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
