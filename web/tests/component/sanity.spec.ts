import { describe, it, expect } from 'vitest';

describe('sanity', () => {
  it('runs component tests in a DOM environment', () => {
    expect(typeof window).toBe('object');
    const el = document.createElement('div');
    el.textContent = 'hi';
    expect(el.textContent).toBe('hi');
  });
});
