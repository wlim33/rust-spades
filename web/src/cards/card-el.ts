import type { Card, Rank, Suit } from '../state/helpers';

const RANK_LABEL: Record<Rank, string> = {
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
};
const SUIT_GLYPH: Record<Suit, string> = {
  Spade: '♠',
  Heart: '♥',
  Diamond: '♦',
  Club: '♣',
};

export type CardPos = { x: number; y: number };
export type CardEl = HTMLDivElement & { _cm: CardPos };

/**
 * Turn any `.card` element into a face-up card for `card`. Shared by the hand
 * and the trick slots. Faces are plain DOM typography (corner index + center
 * pip) rather than scaled-down scans of a full deck: at 40-46px wide only a
 * bold index is readable, and in an overlapped fan the top-left corner is the
 * only strip of the card that stays visible.
 */
export function setCardFace(el: CardEl, card: Card): void {
  const tone = card.suit === 'Heart' || card.suit === 'Diamond' ? 'suit-red' : 'suit-black';
  el.className = `card card-front ${tone}`;
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);

  const corner = document.createElement('span');
  corner.className = 'card-corner';
  const rank = document.createElement('span');
  rank.className = 'card-corner-rank';
  rank.textContent = RANK_LABEL[card.rank];
  const suit = document.createElement('span');
  suit.className = 'card-corner-suit';
  suit.textContent = SUIT_GLYPH[card.suit];
  corner.append(rank, suit);

  const pip = document.createElement('span');
  pip.className = 'card-pip';
  pip.setAttribute('aria-hidden', 'true');
  pip.textContent = SUIT_GLYPH[card.suit];

  el.replaceChildren(corner, pip);
}

export function createFront(card: Card): CardEl {
  const el = document.createElement('div') as CardEl;
  el.setAttribute('role', 'button');
  setCardFace(el, card);
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
  setCardFace(el, card);
}

export function setPos(el: CardEl, x: number, y: number): void {
  el._cm.x = x;
  el._cm.y = y;
  el.style.transform = `translate(${x}px, ${y}px)`;
}
