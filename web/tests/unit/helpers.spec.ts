import { describe, it, expect } from 'vitest';
import {
  type Card,
  type Suit,
  cardEq,
  sortCards,
  seatRel,
  formatClock,
  getLeadSuit,
  isCardValid,
  oppCardCount,
  trickNumber,
} from '../../src/state/helpers';

const c = (suit: Suit, rank: Card['rank']): Card => ({ suit, rank });

describe('cardEq', () => {
  it('matches identical cards', () => {
    expect(cardEq(c('Spade', 'Ace'), c('Spade', 'Ace'))).toBe(true);
  });
  it('rejects differing rank', () => {
    expect(cardEq(c('Spade', 'Ace'), c('Spade', 'King'))).toBe(false);
  });
  it('handles null inputs', () => {
    expect(cardEq(null, c('Spade', 'Ace'))).toBe(false);
    expect(cardEq(null, null)).toBe(false);
  });
});

describe('sortCards', () => {
  it('groups by suit (Spade, Heart, Diamond, Club) and high rank first', () => {
    const hand: Card[] = [
      c('Club', 'Two'),
      c('Spade', 'Three'),
      c('Heart', 'King'),
      c('Spade', 'Ace'),
    ];
    expect(sortCards(hand)).toEqual([
      c('Spade', 'Ace'),
      c('Spade', 'Three'),
      c('Heart', 'King'),
      c('Club', 'Two'),
    ]);
  });
  it('does not mutate input', () => {
    const hand: Card[] = [c('Club', 'Two'), c('Spade', 'Ace')];
    const copy = [...hand];
    sortCards(hand);
    expect(hand).toEqual(copy);
  });
});

describe('seatRel', () => {
  it('south for self', () => expect(seatRel(2, 2)).toBe('south'));
  it('east for +1', () => expect(seatRel(3, 2)).toBe('east'));
  it('north for +2', () => expect(seatRel(0, 2)).toBe('north'));
  it('west for +3', () => expect(seatRel(1, 2)).toBe('west'));
});

describe('formatClock', () => {
  it('null is --:--', () => expect(formatClock(null)).toBe('--:--'));
  it('formats m:ss', () => expect(formatClock(65_000)).toBe('1:05'));
  it('rounds up sub-second', () => expect(formatClock(500)).toBe('0:01'));
  it('floors negative to 0:00', () => expect(formatClock(-1000)).toBe('0:00'));
});

describe('getLeadSuit', () => {
  it('returns null when table is empty', () => {
    const tc: (Card | null)[] = [null, null, null, null];
    expect(getLeadSuit(tc, 0)).toBe(null);
  });
  it('returns the leader suit', () => {
    // 2 cards on the table; current player is at seat 0; leader was 2 seats back = seat 2
    const tc: (Card | null)[] = [c('Heart', 'Ace'), null, c('Heart', 'Five'), null];
    expect(getLeadSuit(tc, 0)).toBe('Heart');
  });
});

describe('isCardValid', () => {
  const hand = (cards: Card[]) => cards;
  it('always valid when not your turn', () => {
    expect(
      isCardValid({
        hand: hand([c('Heart', 'Ace')]),
        leadSuit: null,
        spadesBroken: false,
        card: c('Heart', 'Ace'),
        isMyTurn: false,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
  it('always valid in BETTING', () => {
    expect(
      isCardValid({
        hand: [c('Heart', 'Ace')],
        leadSuit: null,
        spadesBroken: false,
        card: c('Heart', 'Ace'),
        isMyTurn: true,
        phase: 'BETTING',
      }),
    ).toBe(true);
  });
  it('must follow lead suit if held', () => {
    const myHand: Card[] = [c('Heart', 'Two'), c('Spade', 'Ace')];
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: 'Heart',
        spadesBroken: true,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(false);
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: 'Heart',
        spadesBroken: true,
        card: c('Heart', 'Two'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
  it('any suit if void in lead suit', () => {
    const myHand: Card[] = [c('Diamond', 'Two')];
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: 'Heart',
        spadesBroken: false,
        card: c('Diamond', 'Two'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
  it('cannot lead spade unless broken or hand is spades-only', () => {
    const myHand: Card[] = [c('Spade', 'Ace'), c('Heart', 'Two')];
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: null,
        spadesBroken: false,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(false);
    expect(
      isCardValid({
        hand: myHand,
        leadSuit: null,
        spadesBroken: true,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
    expect(
      isCardValid({
        hand: [c('Spade', 'Ace')],
        leadSuit: null,
        spadesBroken: false,
        card: c('Spade', 'Ace'),
        isMyTurn: true,
        phase: 'PLAYING',
      }),
    ).toBe(true);
  });
});

describe('oppCardCount', () => {
  it('returns 13 during BETTING', () => {
    expect(oppCardCount('BETTING', 0, [null, null, null, null], 1)).toBe(13);
  });
  it('returns 0 outside PLAYING/BETTING', () => {
    expect(oppCardCount('MENU', 0, [null, null, null, null], 1)).toBe(0);
  });
  it('counts down by completed tricks, not by State::Trick rotation', () => {
    // Two tricks completed, nothing on the table: every opponent holds 11.
    expect(oppCardCount('PLAYING', 2, [null, null, null, null], 1)).toBe(11);
  });
  it('decrements for a played card at the seat', () => {
    expect(oppCardCount('PLAYING', 2, [null, c('Spade', 'Two'), null, null], 1)).toBe(10);
  });
  it('never goes negative', () => {
    expect(oppCardCount('PLAYING', 13, [null, null, null, null], 1)).toBe(0);
  });
});

describe('trickNumber', () => {
  it('is 1 before any trick completes', () => {
    expect(trickNumber([0, 0, 0, 0])).toBe(1);
  });
  it('is completed tricks + 1 mid-round', () => {
    expect(trickNumber([2, 1, 0, 1])).toBe(5);
  });
  it('caps at 13 when the round is over', () => {
    expect(trickNumber([5, 3, 3, 2])).toBe(13);
  });
});
