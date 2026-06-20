import { request } from '../api/client';
import type { GameStore, GameStateResponse, HandResponse } from './game';

/**
 * Decide whether the caller's hand could have changed in `state`, given the
 * hand we already hold. The south hand only shrinks — by exactly one card when
 * south plays — and is replaced wholesale only when a new hand is dealt. Both
 * change its *length*, so a length match against the expected size means the
 * held hand is still correct and the /hand fetch can be skipped.
 *
 * Expected size during a trick = 13 − tricks already completed − (1 if south
 * currently has a card on the table). When any input needed for that arithmetic
 * is missing (unknown seat, no table/trick arrays), we can't prove the hand is
 * current, so we conservatively refetch. Snapshots always refetch: they are the
 * authoritative resync after a (re)connect.
 */
function handMayHaveChanged(
  store: GameStore,
  playerId: string,
  state: GameStateResponse,
  isSnapshot: boolean,
): boolean {
  if (isSnapshot) return true;
  const southIdx = (state.player_names ?? []).findIndex((e) => e.player_id === playerId);
  const table = state.table_cards;
  const tricks = state.player_tricks_won;
  if (southIdx < 0 || !table || !tricks) return true;
  const tricksCompleted = tricks.reduce((a, b) => a + b, 0);
  const southHasCardDown = table[southIdx] != null;
  const expected = 13 - tricksCompleted - (southHasCardDown ? 1 : 0);
  return store.hand.value.length !== expected;
}

/**
 * Fetch the caller's hand and apply `state` + hand to the store as one
 * update. The hand fetch gets one retry; if both attempts fail the snapshot
 * is applied with the hand we already hold. A one-event-stale hand beats
 * dropping the event entirely: a dropped event freezes the whole table
 * (scores, turn, table cards) until another event arrives — and nothing
 * arrives if this was the last one.
 *
 * The fetch is skipped entirely when the held hand provably can't have changed
 * (see {@link handMayHaveChanged}) — opponent plays and trick collection touch
 * only the table. Skipping that redundant round-trip is what keeps animation
 * cadence off the network latency path: previously every event (≈4 per trick)
 * blocked the WS event queue on a /hand fetch, so latency/jitter showed up as
 * choppy, irregular play pacing.
 */
export async function applyStateWithHand(
  store: GameStore,
  gameId: string,
  playerId: string,
  state: GameStateResponse,
  isSnapshot = false,
): Promise<void> {
  if (!handMayHaveChanged(store, playerId, state, isSnapshot)) {
    store.applyState(state, { player_id: playerId, cards: store.hand.value }, isSnapshot);
    return;
  }
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const hand = await request<HandResponse>(`/games/${gameId}/players/${playerId}/hand`, {
        method: 'GET',
      });
      store.applyState(state, hand, isSnapshot);
      return;
    } catch {
      // retry once, then fall through to the stale-hand fallback
    }
  }
  console.warn('hand fetch failed twice; applying snapshot with the previous hand');
  store.applyState(state, { player_id: playerId, cards: store.hand.value }, isSnapshot);
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
