import { request } from '../api/client';
import type { GameStore, GameStateResponse, HandResponse } from './game';

/**
 * Fetch the caller's hand and apply `state` + hand to the store as one
 * update. The hand fetch gets one retry; if both attempts fail the snapshot
 * is applied with the hand we already hold. A one-event-stale hand beats
 * dropping the event entirely: a dropped event freezes the whole table
 * (scores, turn, table cards) until another event arrives — and nothing
 * arrives if this was the last one.
 */
export async function applyStateWithHand(
  store: GameStore,
  gameId: string,
  playerId: string,
  state: GameStateResponse,
): Promise<void> {
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const hand = await request<HandResponse>(`/games/${gameId}/players/${playerId}/hand`, {
        method: 'GET',
      });
      store.applyState(state, hand);
      return;
    } catch {
      // retry once, then fall through to the stale-hand fallback
    }
  }
  console.warn('hand fetch failed twice; applying snapshot with the previous hand');
  store.applyState(state, { player_id: playerId, cards: store.hand.value });
}

export type PollLoop = { start(): void; stop(): void };

/**
 * Run `poll` every `intervalMs` until `isDone()` reports true after a
 * successful poll, or `poll` fails `maxConsecutiveFailures` times in a row
 * — then stop and call `onGiveUp` so the UI can tell the user instead of
 * hammering a dead server forever. Successes reset the failure budget.
 * Ticks never stack: a poll still in flight when the next tick fires is
 * left to finish and that tick is skipped.
 */
export function createPollLoop(opts: {
  poll: () => Promise<void>;
  isDone: () => boolean;
  intervalMs: number;
  maxConsecutiveFailures: number;
  onGiveUp: () => void;
}): PollLoop {
  let timer: ReturnType<typeof setInterval> | null = null;
  let failures = 0;
  let inFlight = false;

  const stop = (): void => {
    if (timer) clearInterval(timer);
    timer = null;
  };

  const tick = async (): Promise<void> => {
    if (inFlight) return;
    inFlight = true;
    try {
      await opts.poll();
      failures = 0;
      if (opts.isDone()) stop();
    } catch {
      failures++;
      if (failures >= opts.maxConsecutiveFailures) {
        stop();
        opts.onGiveUp();
      }
    } finally {
      inFlight = false;
    }
  };

  return {
    start: (): void => {
      if (timer) return;
      failures = 0;
      timer = setInterval(() => void tick(), opts.intervalMs);
    },
    stop,
  };
}
