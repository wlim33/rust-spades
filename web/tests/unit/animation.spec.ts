import { describe, it, expect } from 'vitest';
import { EASE } from '../../src/cards/animation';

describe('easings', () => {
  it('linear is identity at endpoints', () => {
    expect(EASE.linear(0)).toBe(0);
    expect(EASE.linear(1)).toBe(1);
  });
  it('quartOut at 0 is 0', () => {
    expect(EASE.quartOut(0)).toBe(0);
  });
  it('quartOut at 1 is 1', () => {
    expect(EASE.quartOut(1)).toBe(1);
  });
  it('quartIn at 0.5 is 0.0625', () => {
    expect(EASE.quartIn(0.5)).toBeCloseTo(0.0625, 5);
  });
});
