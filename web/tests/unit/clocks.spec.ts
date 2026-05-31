import { describe, it, expect, afterEach, vi } from 'vitest';
import { captureActiveClock, liveActiveMs } from '../../src/state/clocks';

describe('clocks', () => {
  afterEach(() => vi.restoreAllMocks());

  it('counts the active clock down from the captured snapshot', () => {
    let t = 1000;
    vi.spyOn(performance, 'now').mockImplementation(() => t);
    captureActiveClock(10_000);
    t = 4000; // 3s elapsed
    expect(liveActiveMs()).toBe(7000);
  });

  it('clamps at zero', () => {
    let t = 0;
    vi.spyOn(performance, 'now').mockImplementation(() => t);
    captureActiveClock(2000);
    t = 5000;
    expect(liveActiveMs()).toBe(0);
  });

  it('returns null when there is no active snapshot', () => {
    captureActiveClock(null);
    expect(liveActiveMs()).toBe(null);
  });
});
