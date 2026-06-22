import type { components } from '../api/schema';
import type { Card, Suit, Rank } from '../state/helpers';

export type ReplayResponse = components['schemas']['GameReplayResponse'];
export type ReplayModel = ReplayResponse['model'];
export type TnEvent = ReplayModel['events'][number];
export type TnCard = { suit: string; rank: string };

const SUIT_MAP: Record<string, Suit> = { S: 'Spade', H: 'Heart', D: 'Diamond', C: 'Club' };
const RANK_MAP: Record<string, Rank> = {
  '2': 'Two',
  '3': 'Three',
  '4': 'Four',
  '5': 'Five',
  '6': 'Six',
  '7': 'Seven',
  '8': 'Eight',
  '9': 'Nine',
  T: 'Ten',
  J: 'Jack',
  Q: 'Queen',
  K: 'King',
  A: 'Ace',
};

/** Map a trick-notation card (single-char syms) to the app's Card type. */
export function tnCardToApp(c: TnCard): Card {
  const suit = SUIT_MAP[c.suit];
  const rank = RANK_MAP[c.rank];
  if (!suit || !rank) throw new Error(`unmappable card: ${c.rank}${c.suit}`);
  return { suit, rank };
}
