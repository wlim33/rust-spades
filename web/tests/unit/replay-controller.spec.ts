import { describe, it, expect } from 'vitest';
import { tnCardToApp } from '../../src/replay/types';
import { ReplayController } from '../../src/replay/controller';
import type { ReplayResponse } from '../../src/replay/types';

describe('tnCardToApp', () => {
  it('maps single-char syms to app card', () => {
    expect(tnCardToApp({ suit: 'S', rank: 'A' })).toEqual({ suit: 'Spade', rank: 'Ace' });
    expect(tnCardToApp({ suit: 'C', rank: 'T' })).toEqual({ suit: 'Club', rank: 'Ten' });
    expect(tnCardToApp({ suit: 'H', rank: '2' })).toEqual({ suit: 'Heart', rank: 'Two' });
    expect(tnCardToApp({ suit: 'D', rank: 'K' })).toEqual({ suit: 'Diamond', rank: 'King' });
  });

  it('throws on unmappable card', () => {
    expect(() => tnCardToApp({ suit: 'X', rank: 'A' })).toThrow('unmappable card');
    expect(() => tnCardToApp({ suit: 'S', rank: 'Z' })).toThrow('unmappable card');
  });
});

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

  it('jumpRound clamps to valid round range', () => {
    const c = new ReplayController(fixture());
    c.jumpRound(0); // below valid, clamp to start
    expect(c.atStart()).toBe(true);
    c.jumpRound(999); // above valid, clamp to end
    expect(c.atEnd()).toBe(true);
  });

  it('nil bid renders as 0', () => {
    const c = new ReplayController(fixture());
    c.seekEnd();
    const s = c.state();
    // call.values: ['3','4','nil','4'] → N=3, E=4, S=0 (nil), W=4
    // viewer_seat=2 (S), so S→'south', bids.south should be 0
    expect(s.bids.south).toBe(0);
    expect(s.bids.north).toBe(3);
    expect(s.bids.east).toBe(4);
    expect(s.bids.west).toBe(4);
  });

  it('multi-round score progression', () => {
    // Two rounds: scores [10,20] after round 1, [30,50] after round 2
    const twoRound: ReplayResponse = {
      model: {
        meta: { game_hint: 'spades', seats: ['N', 'E', 'S', 'W'], dealer: 'N',
                players: ['Ann', 'Bo', 'Cy', 'Di'], partnerships: [['N','S'],['E','W']], caps: [], version: 1, extra: [] },
        deck: { suits: ['S','H','D','C'], ranks: ['2','3','4','5','6','7','8','9','T','J','Q','K','A'] },
        events: [
          // Round 1
          { type: 'deal', hands: [
            { target: 'N', cards: [{ suit: 'C', rank: 'K' }] },
            { target: 'E', cards: [{ suit: 'C', rank: '5' }] },
            { target: 'S', cards: [{ suit: 'C', rank: '2' }] },
            { target: 'W', cards: [{ suit: 'C', rank: 'T' }] },
          ] },
          { type: 'call', start: 'N', values: ['2', '2', '2', '2'] },
          { type: 'play', leader: 'N', cards: [
            { suit: 'C', rank: 'K' }, { suit: 'C', rank: '5' },
            { suit: 'C', rank: '2' }, { suit: 'C', rank: 'T' },
          ] },
          // Round 2
          { type: 'deal', hands: [
            { target: 'N', cards: [{ suit: 'S', rank: 'A' }] },
            { target: 'E', cards: [{ suit: 'S', rank: '2' }] },
            { target: 'S', cards: [{ suit: 'S', rank: 'K' }] },
            { target: 'W', cards: [{ suit: 'S', rank: 'Q' }] },
          ] },
          { type: 'call', start: 'N', values: ['1', '1', '1', '1'] },
          { type: 'play', leader: 'N', cards: [
            { suit: 'S', rank: 'A' }, { suit: 'S', rank: '2' },
            { suit: 'S', rank: 'K' }, { suit: 'S', rank: 'Q' },
          ] },
        ],
      },
      cumulative_by_round: [[10, 20], [30, 50]],
      viewer_seat: 2, // S
    } as unknown as ReplayResponse;

    const c = new ReplayController(twoRound);
    expect(c.state().totalRounds).toBe(2);

    // Seek to end of round 1 (after all bids + 1 trick of round 1)
    // Round 1: 4 bid steps + 4 card steps = 8 steps (0-indexed: 0..7)
    // After round 1 score should be [10, 20]
    c.jumpRound(1);
    // jumpRound(1) moves to the last step of round 1
    const s1 = c.state();
    expect(s1.score).toEqual([10, 20]);
    expect(s1.round).toBe(1);

    // Seek to end of round 2
    c.seekEnd();
    const s2 = c.state();
    expect(s2.score).toEqual([30, 50]);
    expect(s2.round).toBe(2);
  });
});
