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
