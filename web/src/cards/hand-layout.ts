/** Layout math for the hand fans. Pure — unit-tested in isolation. */

/** Baseline visible strip of an overlapped card at the 46px base card width. */
const BASE_STRIP = 24;
const BASE_CARD_W = 46;
/** Maximum air between fully spread cards, px — keeps endgame hands fan-like. */
const MAX_GAP = 4;

/**
 * Per-card overlap margin for a fan: spread to fill the container, clamped
 * between full compression (minStrip visible) and a small positive gap.
 * Works for either axis; pass height-based sizes for vertical fans.
 * minStrip defaults to the corner-index strip, scaled up with the card so
 * bigger cards keep their rank readable (floor: the 24px baseline).
 */
export function computeHandOverlap(
  containerSize: number,
  cardSize: number,
  count: number,
  minStrip: number = Math.max(BASE_STRIP, Math.round(cardSize * (BASE_STRIP / BASE_CARD_W))),
): number {
  if (count <= 1) return 0;
  const ideal = (containerSize - count * cardSize) / (count - 1);
  return Math.min(MAX_GAP, Math.max(-(cardSize - minStrip), ideal));
}
