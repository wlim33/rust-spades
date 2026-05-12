import type { Card } from '../state/helpers';
import { cardEq } from '../state/helpers';
import type { CardEl } from './card-el';
import { animateTo } from './animation';
import { HandManager, type Seat, type Containers } from './hand-manager';
import { TrickManager } from './trick-manager';
import { attachDrag } from './drag';

type WinnerOffset = { x: number; y: number };
const TRICK_OFFSETS: Record<Seat, WinnerOffset> = {
  south: { x: 0, y: 60 },
  north: { x: 0, y: -60 },
  west: { x: -80, y: 0 },
  east: { x: 80, y: 0 },
};

export type OrchestratorOpts = {
  containers: Containers;
};

export class CardOrchestrator {
  private hand = new HandManager();
  private trick = new TrickManager();
  private containers: Containers;
  private dragCleanups: Array<() => void> = [];
  private collectingTrick = false;
  private lastPlayRect: DOMRect | null = null;
  private initialized = false;

  constructor(opts: OrchestratorOpts) {
    this.containers = opts.containers;
    this.hand.setContainers(this.containers);
    this.trick.init(this.containers.trick);
  }

  /** First-time setup or reconnect: place everything immediately, no deal animation. */
  setupImmediate(args: {
    playerHand: readonly Card[];
    oppCounts: { north: number; west: number; east: number };
    tableCards: readonly (Card | null)[];
    myIdx: number;
    northIdx: number;
    westIdx: number;
    eastIdx: number;
    currentPlayerSeatIdx: number;
  }): void {
    this.clearAll();
    this.hand.setPlayerHand(args.playerHand);
    this.hand.setOpponentCount('north', args.oppCounts.north);
    this.hand.setOpponentCount('west', args.oppCounts.west);
    this.hand.setOpponentCount('east', args.oppCounts.east);

    const seatMap: Record<number, Seat> = {
      [args.myIdx]: 'south',
      [args.northIdx]: 'north',
      [args.westIdx]: 'west',
      [args.eastIdx]: 'east',
    };
    this.trick.clear();
    const n = args.tableCards.filter(
      (tc) => tc && (tc as { suit?: string }).suit !== 'Blank',
    ).length;
    const leaderSeat = (((args.currentPlayerSeatIdx - n) % 4) + 4) % 4;
    for (let i = 0; i < 4; i++) {
      const absIdx = (leaderSeat + i) % 4;
      const tc = args.tableCards[absIdx];
      const seat = seatMap[absIdx];
      if (tc && (tc as { suit?: string }).suit !== 'Blank' && seat) {
        this.trick.fillNextSlot(tc, seat);
      }
    }
    this.initialized = true;
  }

  isInitialized(): boolean {
    return this.initialized;
  }

  updatePlayerHand(cards: readonly Card[]): void {
    this.hand.setPlayerHand(cards);
  }

  updateOpponentCount(seat: Exclude<Seat, 'south'>, count: number): void {
    this.hand.setOpponentCount(seat, count);
  }

  /** Fly the south player's card from hand → next trick slot. */
  async playCardToCenter(card: Card): Promise<void> {
    const removed = this.hand.removeCard(card);
    const slot = this.trick.fillNextSlot(card, 'south');
    if (!removed || !slot) return;

    const srcRect = this.lastPlayRect ?? removed.getBoundingClientRect();
    this.lastPlayRect = null;

    slot.el.style.visibility = 'hidden';
    // Note: if clearAll() races this 250 ms animation, `removed` lingers in
    // document.body until animateTo resolves and removed.remove() runs below.
    // Matches the reference card-manager.js behavior — acceptable trade-off.
    document.body.appendChild(removed);
    removed.style.position = 'fixed';
    removed.style.left = srcRect.left + 'px';
    removed.style.top = srcRect.top + 'px';
    removed.style.width = srcRect.width + 'px';
    removed.style.height = srcRect.height + 'px';
    removed.style.zIndex = '1000';
    removed.style.margin = '0';
    removed.style.transform = '';
    removed._cm = { x: 0, y: 0 };

    const targetRect = slot.el.getBoundingClientRect();
    await animateTo(removed, {
      x: targetRect.left - srcRect.left,
      y: targetRect.top - srcRect.top,
      duration: 250,
      ease: 'quartOut',
    });
    removed.remove();
    slot.el.style.visibility = '';
  }

  /** Drop an opponent's card into the next trick slot (no fly animation today). */
  playOpponentCardToCenter(card: Card, seat: Exclude<Seat, 'south'>): void {
    this.hand.popOpponentBack(seat);
    this.trick.fillNextSlot(card, seat);
  }

  /** Force-place a card that should be in the trick (used to backfill on AI fast-play). */
  placeCardInTrick(card: Card, seat: Seat): void {
    this.trick.fillNextSlot(card, seat);
  }

  /** Pause → stack → slide toward winner → fade. */
  async collectTrick(winnerSeat: Seat): Promise<void> {
    if (this.trick.count() === 0) return;
    if (this.collectingTrick) return;
    this.collectingTrick = true;
    try {
      const container = this.containers.trick;
      const containerRect = container.getBoundingClientRect();
      const filled = [...this.trick.slots()];
      const positions = filled.map((entry) => {
        const r = entry.el.getBoundingClientRect();
        return {
          left: r.left - containerRect.left,
          top: r.top - containerRect.top,
          width: r.width,
          height: r.height,
        };
      });

      // Absolutely position all slots so layout doesn't shift during fade
      const allSlots = [0, 1, 2, 3]
        .map((i) => this.trick.slotEl(i))
        .filter((x): x is CardEl => !!x);
      const allPositions = allSlots.map((el) => {
        const r = el.getBoundingClientRect();
        return {
          left: r.left - containerRect.left,
          top: r.top - containerRect.top,
          width: r.width,
          height: r.height,
        };
      });
      allSlots.forEach((el, i) => {
        el.style.position = 'absolute';
        el.style.left = allPositions[i]!.left + 'px';
        el.style.top = allPositions[i]!.top + 'px';
        el.style.width = allPositions[i]!.width + 'px';
        el.style.height = allPositions[i]!.height + 'px';
      });
      filled.forEach((entry) => {
        entry.el.style.transform = '';
        entry.el._cm = { x: 0, y: 0 };
      });
      allSlots.forEach((el) => {
        if (el.classList.contains('trick-placeholder')) el.style.visibility = 'hidden';
      });

      await new Promise((r) => setTimeout(r, 400));

      const cw = positions[0]?.width ?? 46;
      const ch = positions[0]?.height ?? 64;
      const centerX = containerRect.width / 2 - cw / 2;
      const centerY = containerRect.height / 2 - ch / 2;

      await Promise.all(
        filled.map((entry, i) => {
          const pos = positions[i]!;
          const targetX = centerX - pos.left + (i - 1.5) * 2;
          const targetY = centerY - pos.top + (i - 1.5) * 1;
          return animateTo(entry.el, { x: targetX, y: targetY, duration: 200, ease: 'quartOut' });
        }),
      );

      const offset = TRICK_OFFSETS[winnerSeat];
      await Promise.all(
        filled.map((entry) =>
          animateTo(entry.el, {
            x: entry.el._cm.x + offset.x,
            y: entry.el._cm.y + offset.y,
            duration: 300,
            ease: 'quartIn',
            onProgress: (raw) => {
              entry.el.style.opacity = `${1 - raw}`;
            },
            onComplete: () => {
              entry.el.style.opacity = '';
            },
          }),
        ),
      );

      this.trick.clear();
    } finally {
      this.collectingTrick = false;
    }
  }

  clearTrick(): void {
    this.trick.clear();
  }

  enableInteraction(validCards: readonly Card[], onPlay: (card: Card) => void): void {
    this.disableInteraction();
    for (const entry of this.hand.cards('south')) {
      if (!entry.card) continue;
      const isValid = validCards.some((vc) => cardEq(vc, entry.card!));
      if (isValid) {
        entry.el.classList.add('cm-clickable');
        entry.el.classList.remove('cm-invalid');
        entry.el.style.opacity = '';
        const card = entry.card;
        const cleanup = attachDrag(entry.el, {
          threshold: 60,
          onPlay: (rect) => {
            this.lastPlayRect = rect;
            onPlay(card);
          },
        });
        this.dragCleanups.push(cleanup);
      } else {
        entry.el.classList.remove('cm-clickable');
        entry.el.classList.add('cm-invalid');
        entry.el.style.opacity = '0.35';
      }
    }
  }

  disableInteraction(): void {
    for (const fn of this.dragCleanups) fn();
    this.dragCleanups = [];
    this.lastPlayRect = null;
    for (const entry of this.hand.cards('south')) {
      entry.el.classList.remove('cm-clickable', 'cm-invalid', 'dragging', 'card-will-play');
      entry.el.style.opacity = '';
      entry.el.style.left = '';
      entry.el.style.top = '';
      entry.el.style.width = '';
      entry.el.style.height = '';
      entry.el.style.transform = '';
    }
  }

  trickCount(): number {
    return this.trick.count();
  }

  clearAll(): void {
    this.disableInteraction();
    this.hand.clear();
    this.trick.clear();
    this.initialized = false;
  }

  destroy(): void {
    this.clearAll();
  }
}
