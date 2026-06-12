import { describe, it, expect } from 'vitest';
import { computeHandOverlap } from '../../src/cards/hand-layout';

describe('computeHandOverlap', () => {
  it('caps the spread at a 4px gap on wide containers', () => {
    // ideal = (900 - 13*46) / 12 = 25.2 -> capped
    expect(computeHandOverlap(900, 46, 13)).toBe(4);
  });

  it('uses the exact fit when between the clamps', () => {
    expect(computeHandOverlap(500, 46, 13)).toBeCloseTo((500 - 13 * 46) / 12, 5);
  });

  it('never compresses below a 24px visible strip', () => {
    expect(computeHandOverlap(200, 46, 13)).toBe(-22); // -(46 - 24)
    expect(computeHandOverlap(0, 40, 13)).toBe(-16); // -(40 - 24), mobile card width
  });

  it('returns 0 for empty and single-card hands', () => {
    expect(computeHandOverlap(670, 46, 0)).toBe(0);
    expect(computeHandOverlap(670, 46, 1)).toBe(0);
  });

  it('handles two cards (denominator 1): full leftover, capped at the gap', () => {
    expect(computeHandOverlap(200, 46, 2)).toBe(4); // ideal = 108, capped
  });
});
