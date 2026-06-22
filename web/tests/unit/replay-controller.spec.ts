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
      meta: {
        game_hint: 'spades',
        seats: ['N', 'E', 'S', 'W'],
        dealer: 'N',
        players: ['Ann', 'Bo', 'Cy', 'Di'],
        partnerships: [
          ['N', 'S'],
          ['E', 'W'],
        ],
        caps: [],
        version: 1,
        extra: [],
      },
      deck: {
        suits: ['S', 'H', 'D', 'C'],
        ranks: ['2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A'],
      },
      events: [
        {
          type: 'deal',
          hands: [
            { target: 'N', cards: [{ suit: 'C', rank: 'K' }] },
            { target: 'E', cards: [{ suit: 'C', rank: '5' }] },
            { target: 'S', cards: [{ suit: 'C', rank: '2' }] },
            { target: 'W', cards: [{ suit: 'C', rank: 'T' }] },
          ],
        },
        { type: 'call', start: 'E', values: ['3', '4', 'nil', '4'] },
        {
          type: 'play',
          leader: 'E',
          cards: [
            { suit: 'C', rank: '5' },
            { suit: 'C', rank: '2' },
            { suit: 'C', rank: 'T' },
            { suit: 'C', rank: 'K' },
          ],
        },
      ],
    },
    cumulative_by_round: [[84, 56]],
    viewer_seat: 2, // S
    termination: 'completed',
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

  it('hand depletes as cards are played and played card appears in trick', () => {
    const c = new ReplayController(fixture());
    // Each seat has 1 card dealt. Before any step the hand is full.
    expect(c.state().hands.south.length).toBe(1);

    // Advance through 4 bid steps (steps 0–3); no cards played yet.
    c.next(); // step 0: bid N
    c.next(); // step 1: bid E
    c.next(); // step 2: bid S
    c.next(); // step 3: bid W
    expect(c.state().hands.south.length).toBe(1);
    expect(c.state().trick.length).toBe(0);

    // fixture play event: leader=E, cards=[C5(E), C2(S), CT(W), CK(N)]
    // Step 4: E plays C5
    c.next();
    {
      const s = c.state();
      // S (viewer=south) has not played yet — hand still full.
      expect(s.hands.south.length).toBe(1);
      // E (east) played C5 — hand should be empty.
      expect(s.hands.east.length).toBe(0);
      // C5 is in the in-progress trick.
      expect(s.trick.length).toBe(1);
      expect(s.trick[0]!.seat).toBe('east');
      expect(s.trick[0]!.card).toEqual({ suit: 'Club', rank: 'Five' });
    }

    // Step 5: S plays C2
    c.next();
    {
      const s = c.state();
      // S played C2 — hand now empty.
      expect(s.hands.south.length).toBe(0);
      // C2 is the second card in the trick.
      expect(s.trick.length).toBe(2);
      expect(s.trick[1]!.seat).toBe('south');
      expect(s.trick[1]!.card).toEqual({ suit: 'Club', rank: 'Two' });
    }
  });

  it('aborted: termination="aborted" sets state().aborted to true', () => {
    const f = fixture();
    const abortedFixture = { ...f, termination: 'aborted' } as unknown as ReplayResponse;
    const c = new ReplayController(abortedFixture);
    expect(c.state().aborted).toBe(true);
  });

  it('aborted: termination="completed" sets state().aborted to false', () => {
    const c = new ReplayController(fixture());
    expect(c.state().aborted).toBe(false);
  });

  it('multi-round score progression', () => {
    // Two rounds: scores [10,20] after round 1, [30,50] after round 2
    const twoRound: ReplayResponse = {
      model: {
        meta: {
          game_hint: 'spades',
          seats: ['N', 'E', 'S', 'W'],
          dealer: 'N',
          players: ['Ann', 'Bo', 'Cy', 'Di'],
          partnerships: [
            ['N', 'S'],
            ['E', 'W'],
          ],
          caps: [],
          version: 1,
          extra: [],
        },
        deck: {
          suits: ['S', 'H', 'D', 'C'],
          ranks: ['2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A'],
        },
        events: [
          // Round 1
          {
            type: 'deal',
            hands: [
              { target: 'N', cards: [{ suit: 'C', rank: 'K' }] },
              { target: 'E', cards: [{ suit: 'C', rank: '5' }] },
              { target: 'S', cards: [{ suit: 'C', rank: '2' }] },
              { target: 'W', cards: [{ suit: 'C', rank: 'T' }] },
            ],
          },
          { type: 'call', start: 'N', values: ['2', '2', '2', '2'] },
          {
            type: 'play',
            leader: 'N',
            cards: [
              { suit: 'C', rank: 'K' },
              { suit: 'C', rank: '5' },
              { suit: 'C', rank: '2' },
              { suit: 'C', rank: 'T' },
            ],
          },
          // Round 2
          {
            type: 'deal',
            hands: [
              { target: 'N', cards: [{ suit: 'S', rank: 'A' }] },
              { target: 'E', cards: [{ suit: 'S', rank: '2' }] },
              { target: 'S', cards: [{ suit: 'S', rank: 'K' }] },
              { target: 'W', cards: [{ suit: 'S', rank: 'Q' }] },
            ],
          },
          { type: 'call', start: 'N', values: ['1', '1', '1', '1'] },
          {
            type: 'play',
            leader: 'N',
            cards: [
              { suit: 'S', rank: 'A' },
              { suit: 'S', rank: '2' },
              { suit: 'S', rank: 'K' },
              { suit: 'S', rank: 'Q' },
            ],
          },
        ],
      },
      cumulative_by_round: [
        [10, 20],
        [30, 50],
      ],
      viewer_seat: 2, // S
      termination: 'completed',
    } as unknown as ReplayResponse;

    const c = new ReplayController(twoRound);
    expect(c.state().totalRounds).toBe(2);

    // jumpRound(1) now lands at the START of round 1 (one step before the first
    // bid of round 1), so cursor = -1 = atStart(). Score is [0,0] (no completed
    // rounds yet) and the round display is 1.
    c.jumpRound(1);
    const s1 = c.state();
    expect(s1.score).toEqual([0, 0]);
    expect(s1.round).toBe(1);
    expect(c.atStart()).toBe(true);

    // jumpRound(2) positions cursor one step before the first step of round 2,
    // i.e. at the last step of round 1. state() reflects the end of round 1:
    // round=1, score=[10,20] (round-1 cumulative, isLastStepOfRound=true).
    c.jumpRound(2);
    const s1b = c.state();
    expect(s1b.round).toBe(1);
    expect(s1b.score).toEqual([10, 20]);

    // Seek to end of round 2
    c.seekEnd();
    const s2 = c.state();
    expect(s2.score).toEqual([30, 50]);
    expect(s2.round).toBe(2);
  });
});
