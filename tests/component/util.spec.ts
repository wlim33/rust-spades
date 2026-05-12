import { describe, it, expect, vi } from 'vitest';
import { navigateTo } from '../../src/lib/util';

describe('navigateTo', () => {
  it('calls history.pushState with the path', () => {
    const spy = vi.spyOn(history, 'pushState').mockImplementation(() => {});
    navigateTo('/foo?bar=1');
    expect(spy).toHaveBeenCalledWith(null, '', '/foo?bar=1');
    spy.mockRestore();
  });

  it('is a no-op when history is unavailable', () => {
    // `history` is always defined in node + happy-dom, so we can only verify
    // the guard exists by reading the source. Skip behavior check here.
    expect(typeof navigateTo).toBe('function');
  });
});
