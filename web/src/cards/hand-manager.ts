import type { Card } from '../state/helpers';
import { createBack, createFront, type CardEl } from './card-el';
import { sortCards, cardEq } from '../state/helpers';
import { computeHandOverlap } from './hand-layout';

export type Seat = 'south' | 'north' | 'east' | 'west';
export type Containers = Record<Seat | 'trick', HTMLElement>;

export type HandEntry = { card: Card | null; el: CardEl };

export class HandManager {
  private containers: Containers | null = null;
  private hands: Record<Seat, HandEntry[]> = { south: [], north: [], east: [], west: [] };
  private resizeObs: ResizeObserver | null = null;

  setContainers(containers: Containers): void {
    this.containers = containers;
    this.resizeObs?.disconnect();
    // happy-dom (component tests) has no ResizeObserver; spacing still updates
    // on every hand change, so only live-resize reactivity is lost there.
    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObs = new ResizeObserver(() => this.updateHandSpacing());
      this.resizeObs.observe(containers.south);
    }
  }

  setPlayerHand(cards: readonly Card[]): void {
    if (!this.containers) return;
    const sorted = sortCards(cards);
    const existing = this.hands.south;

    // No-op when the hand is unchanged: rebuilding re-appends every element,
    // which churns the DOM (and trips e2e "element is not stable" checks)
    // on every state event.
    if (sorted.length === existing.length && sorted.every((c, i) => cardEq(c, existing[i]!.card))) {
      return;
    }
    const kept: HandEntry[] = [];

    // Unmount any cards no longer in the hand
    for (const entry of existing) {
      if (!sorted.some((c) => cardEq(c, entry.card))) {
        entry.el.remove();
      }
    }

    // Build the new ordered list, reusing nodes when possible
    for (const card of sorted) {
      const found = existing.find((e) => cardEq(e.card, card));
      if (found) kept.push(found);
      else kept.push({ card, el: createFront(card) });
    }

    const container = this.containers.south;
    container.innerHTML = '';
    for (const entry of kept) container.appendChild(entry.el);
    this.hands.south = kept;
    this.updateHandSpacing();
  }

  /** Measure and publish the per-card overlap as --hand-ml on the south container. */
  private updateHandSpacing(): void {
    if (!this.containers) return;
    const container = this.containers.south;
    const cardW = this.hands.south[0]?.el.offsetWidth || 46;
    const ml = computeHandOverlap(container.clientWidth, cardW, this.hands.south.length);
    container.style.setProperty('--hand-ml', `${ml}px`);
  }

  setOpponentCount(seat: Exclude<Seat, 'south'>, count: number): void {
    if (!this.containers) return;
    const container = this.containers[seat];
    const entries = this.hands[seat];
    if (count < entries.length) {
      const removed = entries.splice(count);
      for (const e of removed) e.el.remove();
    } else {
      for (let i = entries.length; i < count; i++) {
        const el = createBack();
        container.appendChild(el);
        entries.push({ card: null, el });
      }
    }
  }

  removeCard(card: Card): CardEl | null {
    const entries = this.hands.south;
    const idx = entries.findIndex((e) => cardEq(e.card, card));
    if (idx === -1) return null;
    const [entry] = entries.splice(idx, 1);
    entry!.el.remove();
    this.updateHandSpacing();
    return entry!.el;
  }

  popOpponentBack(seat: Exclude<Seat, 'south'>): CardEl | null {
    const entries = this.hands[seat];
    const entry = entries.pop();
    if (!entry) return null;
    entry.el.remove();
    return entry.el;
  }

  cards(seat: Seat): readonly HandEntry[] {
    return this.hands[seat];
  }

  clear(): void {
    this.resizeObs?.disconnect();
    this.resizeObs = null;
    for (const seat of ['south', 'north', 'east', 'west'] as Seat[]) {
      for (const e of this.hands[seat]) e.el.remove();
      this.hands[seat] = [];
    }
  }
}
