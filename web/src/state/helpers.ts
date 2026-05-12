export type Suit = 'Spade' | 'Heart' | 'Diamond' | 'Club';
export type Rank =
  | 'Two'
  | 'Three'
  | 'Four'
  | 'Five'
  | 'Six'
  | 'Seven'
  | 'Eight'
  | 'Nine'
  | 'Ten'
  | 'Jack'
  | 'Queen'
  | 'King'
  | 'Ace';

export type Card = { suit: Suit; rank: Rank };

export type Phase = 'MENU' | 'CREATE' | 'WAITING' | 'LOBBY' | 'BETTING' | 'PLAYING' | 'GAME_OVER';

const SUIT_ORDER: Suit[] = ['Spade', 'Heart', 'Diamond', 'Club'];
const RANK_ORDER: Rank[] = [
  'Two',
  'Three',
  'Four',
  'Five',
  'Six',
  'Seven',
  'Eight',
  'Nine',
  'Ten',
  'Jack',
  'Queen',
  'King',
  'Ace',
];

export function cardEq(a: Card | null | undefined, b: Card | null | undefined): boolean {
  if (!a || !b) return false;
  return a.suit === b.suit && a.rank === b.rank;
}

export function sortCards(cards: readonly Card[]): Card[] {
  return [...cards].sort((a, b) => {
    const si = SUIT_ORDER.indexOf(a.suit) - SUIT_ORDER.indexOf(b.suit);
    if (si !== 0) return si;
    return RANK_ORDER.indexOf(b.rank) - RANK_ORDER.indexOf(a.rank);
  });
}

export type RelativeSeat = 'south' | 'east' | 'north' | 'west';

export function seatRel(absIdx: number, myIdx: number): RelativeSeat {
  const rel = (((absIdx - myIdx) % 4) + 4) % 4;
  return (['south', 'east', 'north', 'west'] as const)[rel]!;
}

export function formatClock(ms: number | null | undefined): string {
  if (ms == null) return '--:--';
  const totalSec = Math.max(0, Math.ceil(ms / 1000));
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s < 10 ? '0' : ''}${s}`;
}

/**
 * Lead suit derived from the table.
 *
 * The current player's seat (`currentPlayerSeatIdx`) is *next to play*; the
 * leader sat `n` seats before, where `n` is the number of cards already on
 * the table.
 */
export function getLeadSuit(
  tableCards: readonly (Card | null)[],
  currentPlayerSeatIdx: number,
): Suit | null {
  let n = 0;
  for (const c of tableCards) {
    if (c && (c as { suit?: string }).suit !== 'Blank') n++;
  }
  if (n === 0) return null;
  const leaderSeat = (((currentPlayerSeatIdx - n) % 4) + 4) % 4;
  const leadCard = tableCards[leaderSeat];
  return leadCard ? leadCard.suit : null;
}

export function isCardValid(args: {
  hand: readonly Card[];
  leadSuit: Suit | null;
  spadesBroken: boolean;
  card: Card;
  isMyTurn: boolean;
  phase: Phase;
}): boolean {
  if (!args.isMyTurn || args.phase !== 'PLAYING') return true;
  if (args.leadSuit) {
    if (args.hand.some((c) => c.suit === args.leadSuit)) {
      return args.card.suit === args.leadSuit;
    }
    return true;
  }
  if (args.spadesBroken) return true;
  if (args.hand.every((c) => c.suit === 'Spade')) return true;
  return args.card.suit !== 'Spade';
}

type GameStateValue = string | { Betting?: number; Trick?: number; Completed?: unknown };

export function oppCardCount(
  phase: Phase,
  gameState: GameStateValue | null,
  tableCards: readonly (Card | null)[],
  seatIdx: number,
): number {
  if (phase === 'BETTING') return 13;
  if (phase !== 'PLAYING') return 0;
  const trickNum =
    typeof gameState === 'object' && gameState !== null && 'Trick' in gameState
      ? (gameState.Trick as number)
      : 0;
  let count = 13 - trickNum;
  const tc = tableCards[seatIdx];
  if (tc && (tc as { suit?: string }).suit !== 'Blank') count--;
  return Math.max(0, count);
}
