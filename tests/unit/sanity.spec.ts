import { describe, it, expect } from 'vitest';

describe('sanity', () => {
  it('runs unit tests in a node environment', () => {
    expect(typeof window).toBe('undefined');
    expect(2 + 2).toBe(4);
  });
});
