import type { Card } from '../state/helpers';

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
  el._cm = { x: 0, y: 0 };
  return el;
}

export function createBack(): CardEl {
  const el = document.createElement('div') as CardEl;
  el.className = 'card card-back';
  el._cm = { x: 0, y: 0 };
  return el;
}

export function setFront(el: CardEl, card: Card): void {
  el.className = `card card-front ${SUIT_COLOR[card.suit] === 'red' ? 'card-red' : 'card-black'}`;
  el.textContent = cardText(card);
}

export function setPos(el: CardEl, x: number, y: number): void {
  el._cm.x = x;
  el._cm.y = y;
  el.style.transform = `translate(${x}px, ${y}px)`;
}
