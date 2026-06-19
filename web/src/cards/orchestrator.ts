import type { Card } from '../state/helpers';
import { cardEq } from '../state/helpers';
import { createFront, type CardEl } from './card-el';
import { animateTo } from './animation';
import { HandManager, type Seat, type Containers } from './hand-manager';
import { TrickManager } from './trick-manager';
import { attachDrag } from './drag';
import { attachKeyboard } from './keyboard';
import { announce } from '../ui/announce';

type WinnerOffset = { x: number; y: number };
const TRICK_OFFSETS: Record<Seat, WinnerOffset> = {
  south: { x: 0, y: 60 },
  north: { x: 0, y: -60 },
  west: { x: -80, y: 0 },
  east: { x: 80, y: 0 },
};

const SEAT_LABEL: Record<Seat, string> = {
  south: 'You',
  north: 'North',
  west: 'West',
  east: 'East',
};

export type OrchestratorOpts = {
  containers: Containers;
};

export type TrickPlay = { card: Card; seat: Seat };

export class CardOrchestrator {
  private hand = new HandManager();
  private trick = new TrickManager();
  private containers: Containers;
  private dragCleanups: Array<() => void> = [];
  private lastPlayRect: DOMRect | null = null;
  private initialized = false;
  // Flight clones currently parented to document.body, so a clearAll() that
  // races a flight can remove them instead of leaving them orphaned.
  private flyingClones = new Set<CardEl>();

  // Visual steps run strictly one after another through this promise chain,
  // so a burst of state updates (AI plays resolve in milliseconds) still
  // animates each play and each trick collection visibly, in order. State
  // application is never delayed — only its presentation.
  private chain: Promise<void> = Promise.resolve();
  private generation = 0;

  constructor(opts: OrchestratorOpts) {
    this.containers = opts.containers;
    this.hand.setContainers(this.containers);
    this.trick.init(this.containers.trick);
  }

  private enqueue(op: () => void | Promise<void>): Promise<void> {
    const gen = this.generation;
    const next = this.chain
      .then(async () => {
        // clearAll() bumps the generation: steps queued before it are stale
        // (their DOM is gone) and must become no-ops.
        if (gen !== this.generation) return;
        await op();
      })
      .catch((e) => {
        console.error('card animation step failed', e);
      });
    this.chain = next;
    return next;
  }

  /** Resolves when every queued visual step has finished. */
  whenIdle(): Promise<void> {
    return this.chain;
  }

  /** Skip animation flights when they can't be seen or aren't wanted. */
  private skipAnims(): boolean {
    if (typeof requestAnimationFrame !== 'function') return true;
    if (typeof document !== 'undefined' && document.hidden) return true;
    return (
      typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches
    );
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
    const n = args.tableCards.filter((tc) => tc != null).length;
    const leaderSeat = (((args.currentPlayerSeatIdx - n) % 4) + 4) % 4;
    for (let i = 0; i < 4; i++) {
      const absIdx = (leaderSeat + i) % 4;
      const tc = args.tableCards[absIdx];
      const seat = seatMap[absIdx];
      if (tc && seat) {
        this.trick.fillNextSlot(tc, seat);
      }
    }
    this.initialized = true;
  }

  isInitialized(): boolean {
    return this.initialized;
  }

  updatePlayerHand(cards: readonly Card[]): void {
    void this.enqueue(() => this.hand.setPlayerHand(cards));
  }

  updateOpponentCount(seat: Exclude<Seat, 'south'>, count: number): void {
    void this.enqueue(() => this.hand.setOpponentCount(seat, count));
  }

  /** Fly a card clone between two rects, hiding the destination slot meanwhile. */
  private async flyToSlot(flying: CardEl, srcRect: DOMRect, slotEl: CardEl): Promise<void> {
    const gen = this.generation;
    slotEl.style.visibility = 'hidden';
    // Track the clone so a racing clearAll() removes it (and cancels the flight
    // via the generation check below) instead of leaving it orphaned in body.
    this.flyingClones.add(flying);
    document.body.appendChild(flying);
    flying.style.position = 'fixed';
    flying.style.left = srcRect.left + 'px';
    flying.style.top = srcRect.top + 'px';
    flying.style.width = srcRect.width + 'px';
    flying.style.height = srcRect.height + 'px';
    flying.style.zIndex = '1000';
    flying.style.margin = '0';
    flying.style.transform = '';
    flying._cm = { x: 0, y: 0 };

    const targetRect = slotEl.getBoundingClientRect();
    await animateTo(flying, {
      x: targetRect.left - srcRect.left,
      y: targetRect.top - srcRect.top,
      duration: 250,
      ease: 'quartOut',
      cancelled: () => gen !== this.generation,
    });
    flying.remove();
    this.flyingClones.delete(flying);
    slotEl.style.visibility = '';
  }

  /**
   * Fly the south player's card from hand → next trick slot, then run
   * `submit` (the server request) as part of the same queued step. Putting
   * the request inside the step means every later step — in particular the
   * enable for the player's next turn, whose WS events can arrive before
   * this request's HTTP response — is chain-ordered strictly after the play
   * has settled.
   */
  playCardToCenter(card: Card, submit?: () => Promise<void>): Promise<void> {
    // The pointer rect pairs with the click that triggered this play; capture
    // it now, not when the queued step eventually runs.
    const pointerRect = this.lastPlayRect;
    this.lastPlayRect = null;

    return this.enqueue(async () => {
      // Measure while the element is still in the hand: keyboard plays carry
      // no pointer rect, and a detached element measures 0,0 (card flies in
      // from the viewport origin).
      const entry = this.hand.cards('south').find((e) => cardEq(e.card, card));
      const liveRect = entry?.el.isConnected ? entry.el.getBoundingClientRect() : null;

      const removed = this.hand.removeCard(card);
      const slot = this.trick.fillNextSlot(card, 'south');
      if (removed && slot) {
        const srcRect = pointerRect ?? liveRect;
        if (!this.skipAnims() && srcRect && srcRect.width > 0) {
          await this.flyToSlot(removed, srcRect, slot.el);
        }
      }
      if (submit) await submit();
    });
  }

  /** Fly an opponent's card from their stack into the next trick slot. */
  playOpponentCardToCenter(card: Card, seat: Exclude<Seat, 'south'>): void {
    void this.enqueue(async () => {
      const backs = this.hand.cards(seat);
      const lastBack = backs[backs.length - 1]?.el ?? null;
      const srcRect = lastBack?.isConnected ? lastBack.getBoundingClientRect() : null;

      this.hand.popOpponentBack(seat);
      const slot = this.trick.fillNextSlot(card, seat);
      announce(`${SEAT_LABEL[seat]} played ${card.rank} of ${card.suit}s`);
      if (!slot) return;

      if (this.skipAnims() || !srcRect || srcRect.width === 0) return;
      await this.flyToSlot(createFront(card), srcRect, slot.el);
    });
  }

  /** Force-place a card that should be in the trick (no animation). */
  placeCardInTrick(card: Card, seat: Seat): void {
    void this.enqueue(() => {
      this.trick.fillNextSlot(card, seat);
    });
  }

  /**
   * Finish a trick: backfill whichever of the four plays the queued steps have
   * not rendered yet (the server clears the table within the same event as the
   * final card, so the per-card diff misses it), then collect toward the
   * winner. The backfill decision happens when the step RUNS — earlier queued
   * plays have filled their slots by then.
   */
  completeTrick(plays: readonly TrickPlay[], winnerSeat: Seat): void {
    void this.enqueue(async () => {
      const placed = new Set(this.trick.slots().map((s) => s.seat));
      for (const p of plays) {
        if (!placed.has(p.seat)) this.trick.fillNextSlot(p.card, p.seat);
      }
      await this.runCollect(winnerSeat);
    });
  }

  /** Pause → stack → slide toward winner → fade. */
  private async runCollect(winnerSeat: Seat): Promise<void> {
    if (this.trick.count() === 0) return;
    const gen = this.generation;
    announce(`${SEAT_LABEL[winnerSeat]} won the trick`);
    if (this.skipAnims()) {
      this.trick.clear();
      return;
    }
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
    const allSlots = [0, 1, 2, 3].map((i) => this.trick.slotEl(i)).filter((x): x is CardEl => !!x);
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
        return animateTo(entry.el, {
          x: targetX,
          y: targetY,
          duration: 200,
          ease: 'quartOut',
          cancelled: () => gen !== this.generation,
        });
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
          cancelled: () => gen !== this.generation,
        }),
      ),
    );

    this.trick.clear();
  }

  clearTrick(): void {
    void this.enqueue(() => this.trick.clear());
  }

  enableInteraction(
    validCards: readonly Card[],
    onPlay: (card: Card) => void,
    stillMyTurn?: () => boolean,
  ): void {
    // Queued: input opens only after the pending animations have shown the
    // state the player is reacting to. Because the step runs later, it must
    // re-check `stillMyTurn` at execution — an enable captured before the
    // player's own play would otherwise reopen input on a turn that's over.
    void this.enqueue(() => {
      this.detachInteraction();
      if (stillMyTurn && !stillMyTurn()) return;
      for (const entry of this.hand.cards('south')) {
        if (!entry.card) continue;
        const isValid = validCards.some((vc) => cardEq(vc, entry.card!));
        if (isValid) {
          entry.el.classList.add('cm-clickable');
          entry.el.classList.remove('cm-invalid');
          entry.el.style.opacity = '';
          entry.el.removeAttribute('aria-disabled');
          const card = entry.card;
          const cleanup = attachDrag(entry.el, {
            threshold: 60,
            onPlay: (rect) => {
              this.lastPlayRect = rect;
              onPlay(card);
            },
          });
          this.dragCleanups.push(cleanup);
          // Keyboard parity: Tab to a legal card, Enter/Space to play it.
          this.dragCleanups.push(attachKeyboard(entry.el, () => onPlay(card)));
        } else {
          entry.el.classList.remove('cm-clickable');
          entry.el.classList.add('cm-invalid');
          entry.el.style.opacity = '0.35';
          entry.el.setAttribute('aria-disabled', 'true');
        }
      }
    });
  }

  /**
   * Immediate (not queued): revoking input must take effect at the moment of
   * the call — e.g. the instant a card is played — or stale clicks could
   * slip in behind pending animations.
   */
  disableInteraction(): void {
    this.detachInteraction();
  }

  private detachInteraction(): void {
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
      entry.el.removeAttribute('aria-disabled');
      entry.el.removeAttribute('tabindex');
    }
  }

  trickCount(): number {
    return this.trick.count();
  }

  /** Relative seats that already have a card in the current trick. */
  trickSeats(): Seat[] {
    return this.trick.slots().map((s) => s.seat);
  }

  clearAll(): void {
    // Invalidate queued steps: they belong to DOM this method is wiping.
    this.generation++;
    this.detachInteraction();
    // Remove flight clones still parented to body: their animateTo sees the new
    // generation and resolves, but we drop the orphaned nodes now.
    for (const el of this.flyingClones) el.remove();
    this.flyingClones.clear();
    this.hand.clear();
    this.trick.clear();
    this.initialized = false;
  }

  destroy(): void {
    this.clearAll();
    this.hand.dispose();
  }
}
