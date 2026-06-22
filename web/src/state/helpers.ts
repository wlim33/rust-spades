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
export const RANK_ORDER: Rank[] = [
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
  // Clockwise like the server's seating: the next player to act sits to
  // your left (screen west), exactly as at a physical table.
  return (['south', 'west', 'north', 'east'] as const)[rel]!;
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
    if (c) n++;
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

/**
 * Cards an opponent still holds. `tricksCompleted` is the number of finished
 * tricks this round (sum of player_tricks_won) — NOT the payload of
 * `State::Trick(n)`, which is the rotation position of the next player.
 */
export function oppCardCount(
  phase: Phase,
  tricksCompleted: number,
  tableCards: readonly (Card | null)[],
  seatIdx: number,
): number {
  if (phase === 'BETTING') return 13;
  if (phase !== 'PLAYING') return 0;
  let count = 13 - tricksCompleted;
  const tc = tableCards[seatIdx];
  if (tc) count--;
  return Math.max(0, count);
}

/** 1-based number of the trick currently being played, capped at 13. */
export function trickNumber(playerTricksWon: readonly number[]): number {
  const completed = playerTricksWon.reduce((a, b) => a + b, 0);
  return Math.min(completed + 1, 13);
}

/**
 * True when the turn just passed to `myId` — the rising edge that triggers
 * the turn chime. Phases without a turn never chime.
 */
export function becameMyTurn(
  prev: string | null,
  current: string | null,
  myId: string,
  phase: Phase,
): boolean {
  if (!myId) return false;
  if (phase !== 'BETTING' && phase !== 'PLAYING') return false;
  return current === myId && prev !== current;
}
