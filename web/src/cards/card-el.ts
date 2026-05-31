import type { Card, Rank, Suit } from '../state/helpers';

const RANK_FILE: Record<Rank, string> = {
  Two: '2', Three: '3', Four: '4', Five: '5', Six: '6', Seven: '7', Eight: '8',
  Nine: '9', Ten: 'T', Jack: 'J', Queen: 'Q', King: 'K', Ace: 'A',
};
const SUIT_FILE: Record<Suit, string> = { Spade: 'S', Heart: 'H', Diamond: 'D', Club: 'C' };

export function cardFaceUrl(card: Card): string {
  return `/cards/${RANK_FILE[card.rank]}${SUIT_FILE[card.suit]}.svg`;
}

const SUIT_SYMBOL = { Spade: '♠', Heart: '♥', Diamond: '♦', Club: '♣' } as const;
const RANK_DISPLAY = {
  Two: '2',
  Three: '3',
  Four: '4',
  Five: '5',
  Six: '6',
  Seven: '7',
  Eight: '8',
  Nine: '9',
  Ten: '10',
  Jack: 'J',
  Queen: 'Q',
  King: 'K',
  Ace: 'A',
} as const;
const SUIT_COLOR = { Spade: 'black', Heart: 'red', Diamond: 'red', Club: 'black' } as const;

export type CardPos = { x: number; y: number };
export type CardEl = HTMLDivElement & { _cm: CardPos };

export function cardText(card: Card): string {
  return RANK_DISPLAY[card.rank] + SUIT_SYMBOL[card.suit];
}

export function createFront(card: Card): CardEl {
  const el = document.createElement('div') as CardEl;
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
  el.dataset.rank = RANK_DISPLAY[card.rank];
  el.dataset.suit = SUIT_SYMBOL[card.suit];
  el.setAttribute('role', 'button');
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);
  el._cm = { x: 0, y: 0 };
  return el;
}

export function createBack(): CardEl {
  const el = document.createElement('div') as CardEl;
  el.className = 'card card-back';
  el.setAttribute('aria-hidden', 'true');
  el._cm = { x: 0, y: 0 };
  return el;
}

export function setFront(el: CardEl, card: Card): void {
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
  el.dataset.rank = RANK_DISPLAY[card.rank];
  el.dataset.suit = SUIT_SYMBOL[card.suit];
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);
}

export function setPos(el: CardEl, x: number, y: number): void {
  el._cm.x = x;
  el._cm.y = y;
  el.style.transform = `translate(${x}px, ${y}px)`;
}
