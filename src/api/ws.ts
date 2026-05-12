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
