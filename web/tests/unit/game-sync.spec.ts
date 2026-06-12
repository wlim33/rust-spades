import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createGameStore } from '../../src/state/game';
import type { GameStateResponse, HandResponse } from '../../src/state/game';
import { applyStateWithHand, createPollLoop } from '../../src/state/game-sync';
import { request } from '../../src/api/client';

vi.mock('../../src/api/client', () => ({ request: vi.fn() }));
const requestMock = vi.mocked(request);

const names = ['p1', 'p2', 'p3', 'p4'].map((id) => ({ player_id: id, name: id }));

const snapshot = (seq: number): GameStateResponse => ({
  game_id: 'g1',
  state: { Trick: 0 },
  team_a_score: 0,
  team_b_score: 0,
  team_a_bags: 0,
  team_b_bags: 0,
  current_player_id: 'p1',
  player_names: names,
  seq,
});

const aceOfSpades: HandResponse = {
  player_id: 'p1',
  cards: [{ suit: 'Spade', rank: 'Ace' }],
};

describe('applyStateWithHand', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('applies the snapshot together with the fetched hand', async () => {
    const store = createGameStore('p1');
    requestMock.mockResolvedValueOnce(aceOfSpades);

    await applyStateWithHand(store, 'g1', 'p1', snapshot(1));

    expect(store.hand.value).toEqual(aceOfSpades.cards);
    expect(store.phase.value).toBe('PLAYING');
  });

  it('retries a failed hand fetch once', async () => {
    const store = createGameStore('p1');
    requestMock.mockRejectedValueOnce(new Error('net')).mockResolvedValueOnce(aceOfSpades);

    await applyStateWithHand(store, 'g1', 'p1', snapshot(1));

    expect(requestMock).toHaveBeenCalledTimes(2);
    expect(store.hand.value).toEqual(aceOfSpades.cards);
  });

  it('applies the snapshot with the previous hand when fetches keep failing', async () => {
    // Dropping the event instead would freeze the whole table (scores, turn,
    // table cards) until another event arrives — and nothing arrives if this
    // was the last one. A one-event-stale hand is the lesser evil.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const store = createGameStore('p1');
    store.hand.value = [{ suit: 'Heart', rank: 'Two' }];
    requestMock.mockRejectedValue(new Error('net'));

    await applyStateWithHand(store, 'g1', 'p1', snapshot(2));

    expect(store.gameState.value).toEqual({ Trick: 0 });
    expect(store.hand.value).toEqual([{ suit: 'Heart', rank: 'Two' }]);
    warn.mockRestore();
  });
});

describe('createPollLoop', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('polls on the interval while polls succeed', async () => {
    const poll = vi.fn().mockResolvedValue(undefined);
    const loop = createPollLoop({
      poll,
      isDone: () => false,
      intervalMs: 1000,
      maxConsecutiveFailures: 3,
      onGiveUp: () => {},
    });
    loop.start();

    await vi.advanceTimersByTimeAsync(3500);
    expect(poll).toHaveBeenCalledTimes(3);
    loop.stop();
  });

  it('gives up after maxConsecutiveFailures and stops polling', async () => {
    const poll = vi.fn().mockRejectedValue(new Error('down'));
    const onGiveUp = vi.fn();
    const loop = createPollLoop({
      poll,
      isDone: () => false,
      intervalMs: 1000,
      maxConsecutiveFailures: 3,
      onGiveUp,
    });
    loop.start();

    await vi.advanceTimersByTimeAsync(10_000);
    expect(poll).toHaveBeenCalledTimes(3);
    expect(onGiveUp).toHaveBeenCalledTimes(1);
  });

  it('resets the failure budget on success', async () => {
    // fail, fail, succeed — repeatedly. More than maxConsecutiveFailures
    // total failures, but never consecutive, so the loop must keep going.
    let call = 0;
    const poll = vi.fn().mockImplementation(() => {
      call++;
      return call % 3 === 0 ? Promise.resolve() : Promise.reject(new Error('blip'));
    });
    const onGiveUp = vi.fn();
    const loop = createPollLoop({
      poll,
      isDone: () => false,
      intervalMs: 1000,
      maxConsecutiveFailures: 3,
      onGiveUp,
    });
    loop.start();

    await vi.advanceTimersByTimeAsync(9_500);
    expect(poll).toHaveBeenCalledTimes(9);
    expect(onGiveUp).not.toHaveBeenCalled();
    loop.stop();
  });

  it('stops once isDone reports true after a successful poll', async () => {
    let done = false;
    const poll = vi.fn().mockImplementation(() => {
      done = true;
      return Promise.resolve();
    });
    const loop = createPollLoop({
      poll,
      isDone: () => done,
      intervalMs: 1000,
      maxConsecutiveFailures: 3,
      onGiveUp: () => {},
    });
    loop.start();

    await vi.advanceTimersByTimeAsync(5000);
    expect(poll).toHaveBeenCalledTimes(1);
  });

  it('start is idempotent while the loop is running', async () => {
    const poll = vi.fn().mockResolvedValue(undefined);
    const loop = createPollLoop({
      poll,
      isDone: () => false,
      intervalMs: 1000,
      maxConsecutiveFailures: 3,
      onGiveUp: () => {},
    });
    loop.start();
    loop.start(); // e.g. WS onClose firing again

    await vi.advanceTimersByTimeAsync(1000);
    expect(poll).toHaveBeenCalledTimes(1);
    loop.stop();
  });
});
