import type { Card } from '../state/helpers';
import type { Seat } from './hand-manager';
import { setCardFace, type CardEl } from './card-el';

export type TrickSlot = { card: Card; seat: Seat; el: CardEl };

export class TrickManager {
  private container: HTMLElement | null = null;
  private slotEls: CardEl[] = [];
  private filled: TrickSlot[] = [];

  init(container: HTMLElement): void {
    this.container = container;
    this.clear();
  }

  fillNextSlot(card: Card, seat: Seat): TrickSlot | null {
    const slot = this.slotEls.find((el) => el.classList.contains('trick-placeholder'));
    if (!slot) return null;
    setCardFace(slot, card);
    const entry: TrickSlot = { card, seat, el: slot };
    this.filled.push(entry);
    return entry;
  }

  slots(): readonly TrickSlot[] {
    return this.filled;
  }

  slotEl(idx: number): CardEl | undefined {
    return this.slotEls[idx];
  }

  count(): number {
    return this.filled.length;
  }

  clear(): void {
    if (!this.container) return;
    this.container.innerHTML = '';
    this.slotEls = [];
    this.filled = [];
    for (let i = 0; i < 4; i++) {
      const el = document.createElement('div') as CardEl;
      el.className = 'card trick-placeholder';
      el._cm = { x: 0, y: 0 };
      this.container.appendChild(el);
      this.slotEls.push(el);
    }
  }
}
