/** Layout math for the south hand fan. Pure — unit-tested in isolation. */

/** Minimum visible strip of an overlapped card, px (the same value the pre-adaptive CSS used). */
const MIN_STRIP = 24;
/** Maximum air between fully spread cards, px — keeps endgame hands fan-like. */
const MAX_GAP = 4;

/**
 * Per-card margin-left for the hand fan: spread to fill the container,
 * clamped between full compression (24px strip) and a small positive gap.
 */
export function computeHandOverlap(
  containerWidth: number,
  cardWidth: number,
  count: number,
): number {
  if (count <= 1) return 0;
  const ideal = (containerWidth - count * cardWidth) / (count - 1);
  return Math.min(MAX_GAP, Math.max(-(cardWidth - MIN_STRIP), ideal));
}
