/**
 * ReplayBoard — DOM renderer for the replay viewer.
 *
 * Shows all four hands face-up and the current trick. Reuses the card-animation
 * primitives (createFront, setPos, animateTo) but does NOT use HandManager,
 * TrickManager, or CardOrchestrator — those assume one face-up hand + opponent
 * card-backs. Here all four seats show faces.
 *
 * render(prev, next, opts?):
 *   - If opts.animate !== false AND reduced-motion is OFF AND the delta from
 *     prev→next is exactly one newly-played card (one seat gained one trick
 *     card / lost one hand card): animate that card flying hand→trick slot via
 *     animateTo (ease 'backOut'), then reconcile. Otherwise SNAP.
 * clear(): empty all five containers.
 */

import type { Card } from '../state/helpers';
import { cardEq } from '../state/helpers';
import { createFront, setPos, type CardEl } from '../cards/card-el';
import { animateTo } from '../cards/animation';
import { computeHandOverlap } from '../cards/hand-layout';
import type { Seat, Containers } from '../cards/hand-manager';
import type { ViewState } from './controller';

// ---------------------------------------------------------------------------
// Constants

/** Default card dimensions (px) — used when the container has no measured size. */
const CARD_W = 46;
const CARD_H = 64;

/**
 * Minimum visible strip for side (vertical) fans.
 * Matches the SIDE_MIN_STRIP in HandManager.
 */
const SIDE_MIN_STRIP = 4;

/**
 * Offset from the container centre toward each seat for trick cards (px).
 * Mirrors the convention in orchestrator.ts (TRICK_OFFSETS) adapted for
 * placement rather than collect-animation direction.
 */
const TRICK_SLOT_OFFSETS: Record<Seat, { x: number; y: number }> = {
  south: { x: 0, y: 40 },
  north: { x: 0, y: -40 },
  west:  { x: -56, y: 0 },
  east:  { x: 56, y: 0 },
};

// ---------------------------------------------------------------------------
// Helpers

/** Mirror orchestrator.ts skipAnims() exactly. */
function skipAnims(): boolean {
  if (typeof requestAnimationFrame !== 'function') return true;
  if (typeof document !== 'undefined' && document.hidden) return true;
  return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/**
 * Lay out a fan of card elements in a container.
 *
 * For south/north (horizontal) fans: cards are positioned left→right using
 * margin-left overlap; we translate each card along the x axis so the fan is
 * centred. The CSS class `replay-hand-card` controls the card's position:
 * absolute within the container — the container must be relatively positioned
 * (Task 4's stylesheet handles that).
 *
 * For west/east (vertical) fans: cards fan top→bottom along the y axis using
 * the same computeHandOverlap logic with height parameters.
 */
function layoutHand(
  container: HTMLElement,
  cards: readonly Card[],
  seat: Seat,
): void {
  container.innerHTML = '';
  if (cards.length === 0) return;

  const isVertical = seat === 'west' || seat === 'east';

  const containerW = container.clientWidth  || (isVertical ? CARD_W  : CARD_W * 5);
  const containerH = container.clientHeight || (isVertical ? CARD_H * 5 : CARD_H);

  if (isVertical) {
    const cardH = CARD_H;
    const cardW = CARD_W;
    const mt = computeHandOverlap(containerH, cardH, cards.length, SIDE_MIN_STRIP);
    // Total fan height: first card full + rest only show strip
    const totalH = cardH + (cards.length - 1) * (cardH + mt);
    const startY = Math.max(0, (containerH - totalH) / 2);
    const centreX = Math.max(0, (containerW - cardW) / 2);

    for (let i = 0; i < cards.length; i++) {
      const el = createFront(cards[i]!);
      el.classList.add('replay-hand-card');
      container.appendChild(el);
      const y = startY + i * (cardH + mt);
      setPos(el, centreX, y);
    }
  } else {
    const cardW = CARD_W;
    const cardH = CARD_H;
    const ml = computeHandOverlap(containerW, cardW, cards.length);
    const totalW = cardW + (cards.length - 1) * (cardW + ml);
    const startX = Math.max(0, (containerW - totalW) / 2);
    const centreY = Math.max(0, (containerH - cardH) / 2);

    for (let i = 0; i < cards.length; i++) {
      const el = createFront(cards[i]!);
      el.classList.add('replay-hand-card');
      container.appendChild(el);
      const x = startX + i * (cardW + ml);
      setPos(el, x, centreY);
    }
  }
}

/**
 * Render the current trick into `container`.
 * Each card is offset from the container's centre toward its seat.
 * If `winnerSeat` is set, the card belonging to that seat gets the
 * `replay-trick-winner` class.
 */
function layoutTrick(
  container: HTMLElement,
  trick: ViewState['trick'],
  winnerSeat: Seat | null,
): void {
  container.innerHTML = '';
  if (trick.length === 0) return;

  const cw = container.clientWidth  || CARD_W * 4;
  const ch = container.clientHeight || CARD_H * 3;
  const centreX = (cw - CARD_W) / 2;
  const centreY = (ch - CARD_H) / 2;

  for (const { seat, card } of trick) {
    const el = createFront(card);
    el.classList.add('replay-trick-card');
    if (seat === winnerSeat) {
      el.classList.add('replay-trick-winner');
    }
    container.appendChild(el);
    const off = TRICK_SLOT_OFFSETS[seat];
    setPos(el, centreX + off.x, centreY + off.y);
  }
}

// ---------------------------------------------------------------------------
// Delta detection

type CardDelta =
  | { kind: 'single-play'; seat: Seat; card: Card }
  | { kind: 'other' };

const SEATS: Seat[] = ['south', 'north', 'east', 'west'];

/**
 * Determine whether the transition from `prev` to `next` represents exactly
 * one card being played: a single seat lost one card from its hand and the
 * same card appeared in the trick.
 *
 * Returns { kind: 'single-play', seat, card } when true, { kind: 'other' } otherwise.
 */
function detectDelta(prev: ViewState, next: ViewState): CardDelta {
  // Trick must have grown by exactly 1
  if (next.trick.length !== prev.trick.length + 1) return { kind: 'other' };

  // Identify the new trick card (the last one added)
  const newTrickEntry = next.trick[next.trick.length - 1];
  if (!newTrickEntry) return { kind: 'other' };

  const { seat, card: newCard } = newTrickEntry;

  // That seat's hand must have shrunk by exactly 1
  const prevHand = prev.hands[seat];
  const nextHand = next.hands[seat];
  if (nextHand.length !== prevHand.length - 1) return { kind: 'other' };

  // The removed card must be the one that appeared in the trick
  const removedIdx = prevHand.findIndex((c) => cardEq(c, newCard));
  if (removedIdx === -1) return { kind: 'other' };

  // All other seats' hands must be unchanged
  for (const s of SEATS) {
    if (s === seat) continue;
    const ph = prev.hands[s];
    const nh = next.hands[s];
    if (ph.length !== nh.length) return { kind: 'other' };
    if (!ph.every((c, i) => cardEq(c, nh[i]!))) return { kind: 'other' };
  }

  return { kind: 'single-play', seat, card: newCard };
}

// ---------------------------------------------------------------------------
// ReplayBoard

const SEATS_ALL = ['south', 'west', 'north', 'east'] as const;

export class ReplayBoard {
  private readonly containers: Containers;
  /**
   * Map of seat → array of CardEl currently in that seat's container.
   * Used to look up the source element for the animate path.
   */
  private handEls: Record<Seat, CardEl[]> = { south: [], north: [], east: [], west: [] };

  constructor(containers: Containers) {
    this.containers = containers;
  }

  /**
   * Render `next` state.
   *
   * opts.animate defaults to true. When true, prefers-reduced-motion is off,
   * and the delta is a single card play, the card flies hand→trick slot. In
   * all other cases the full state is snapped into place.
   */
  async render(
    prev: ViewState | null,
    next: ViewState,
    opts: { animate?: boolean } = {},
  ): Promise<void> {
    const wantAnimate = opts.animate !== false;

    // Try the animated single-play path
    if (wantAnimate && !skipAnims() && prev !== null) {
      const delta = detectDelta(prev, next);
      if (delta.kind === 'single-play') {
        await this.animateSinglePlay(prev, next, delta.seat, delta.card);
        return;
      }
    }

    // Snap path: clear and re-render everything
    this.snapAll(next);
  }

  clear(): void {
    for (const seat of SEATS_ALL) {
      this.containers[seat].innerHTML = '';
      this.handEls[seat] = [];
    }
    this.containers.trick.innerHTML = '';
  }

  // -------------------------------------------------------------------------
  // Private

  private snapAll(next: ViewState): void {
    // Render all four hands
    for (const seat of SEATS_ALL) {
      layoutHand(this.containers[seat], next.hands[seat], seat);
      // Update tracked element refs
      this.handEls[seat] = Array.from(
        this.containers[seat].querySelectorAll<CardEl>('.card-front'),
      );
    }
    // Render trick
    layoutTrick(this.containers.trick, next.trick, next.trickWinner);
  }

  private async animateSinglePlay(
    prev: ViewState,
    next: ViewState,
    seat: Seat,
    card: Card,
  ): Promise<void> {
    // Find the source element in the current (pre-render) DOM
    const prevHandEls = this.handEls[seat];
    const prevCards = prev.hands[seat];
    const cardIdx = prevCards.findIndex((c) => cardEq(c, card));
    const srcEl: CardEl | undefined = prevHandEls[cardIdx];

    if (!srcEl || !srcEl.isConnected) {
      // Can't find the source element — fall back to snap
      this.snapAll(next);
      return;
    }

    // Measure source position before any DOM changes
    const srcRect = srcEl.getBoundingClientRect();

    // Render the destination (trick) state with all hands snapped to next,
    // but leave a placeholder so we know where to fly to
    const trickContainer = this.containers.trick;
    trickContainer.innerHTML = '';

    // Render next trick excluding the animated card, determine target coords
    const newTrickEntry = next.trick[next.trick.length - 1]!;
    const cw = trickContainer.clientWidth  || CARD_W * 4;
    const ch = trickContainer.clientHeight || CARD_H * 3;
    const centreX = (cw - CARD_W) / 2;
    const centreY = (ch - CARD_H) / 2;
    const trickOff = TRICK_SLOT_OFFSETS[seat];
    const destX = centreX + trickOff.x;
    const destY = centreY + trickOff.y;

    // Place the earlier trick cards
    for (const entry of next.trick.slice(0, -1)) {
      const el = createFront(entry.card);
      el.classList.add('replay-trick-card');
      if (entry.seat === next.trickWinner) el.classList.add('replay-trick-winner');
      trickContainer.appendChild(el);
      const off = TRICK_SLOT_OFFSETS[entry.seat];
      setPos(el, centreX + off.x, centreY + off.y);
    }

    // Update all four hands to next state immediately (snap)
    for (const s of SEATS_ALL) {
      layoutHand(this.containers[s], next.hands[s], s);
      this.handEls[s] = Array.from(
        this.containers[s].querySelectorAll<CardEl>('.card-front'),
      );
    }

    // Create the flying clone and reparent it to body for viewport-relative fly
    const flying = createFront(card);
    flying.classList.add('replay-trick-card');
    if (seat === next.trickWinner) flying.classList.add('replay-trick-winner');
    document.body.appendChild(flying);
    flying.style.position = 'fixed';
    flying.style.left  = `${srcRect.left}px`;
    flying.style.top   = `${srcRect.top}px`;
    flying.style.width  = `${srcRect.width || CARD_W}px`;
    flying.style.height = `${srcRect.height || CARD_H}px`;
    flying.style.zIndex = '1000';
    flying.style.margin = '0';
    flying.style.transform = '';
    flying._cm = { x: 0, y: 0 };

    // Determine viewport-relative destination
    const trickRect = trickContainer.getBoundingClientRect();
    const flyDestX = trickRect.left + destX - srcRect.left;
    const flyDestY = trickRect.top  + destY - srcRect.top;

    await animateTo(flying, {
      x: flyDestX,
      y: flyDestY,
      duration: 300,
      ease: 'backOut',
    });

    flying.remove();

    // Now place the final trick card at its resting position
    const landEl = createFront(card);
    landEl.classList.add('replay-trick-card');
    if (seat === next.trickWinner) landEl.classList.add('replay-trick-winner');
    trickContainer.appendChild(landEl);
    setPos(landEl, destX, destY);
  }
}
