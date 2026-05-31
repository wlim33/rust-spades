import type { Card, Rank, Suit } from '../state/helpers';

const RANK_FILE: Record<Rank, string> = {
  Two: '2', Three: '3', Four: '4', Five: '5', Six: '6', Seven: '7', Eight: '8',
  Nine: '9', Ten: 'T', Jack: 'J', Queen: 'Q', King: 'K', Ace: 'A',
};
const SUIT_FILE: Record<Suit, string> = { Spade: 'S', Heart: 'H', Diamond: 'D', Club: 'C' };

export function cardFaceUrl(card: Card): string {
  return `/cards/${RANK_FILE[card.rank]}${SUIT_FILE[card.suit]}.svg`;
}

export type CardPos = { x: number; y: number };
export type CardEl = HTMLDivElement & { _cm: CardPos };

function faceImg(card: Card): HTMLImageElement {
  const img = document.createElement('img');
  img.className = 'card-face';
  img.src = cardFaceUrl(card);
  img.alt = '';
  img.loading = 'lazy';
  img.draggable = false;
  return img;
}

/** Turn any `.card` element into a face-up card for `card`. Shared by the hand and the trick slots. */
export function setCardFace(el: CardEl, card: Card): void {
  el.className = 'card card-front';
  el.setAttribute('aria-label', `${card.rank} of ${card.suit}s`);
  el.replaceChildren(faceImg(card));
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
