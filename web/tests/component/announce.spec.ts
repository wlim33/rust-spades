import { describe, it, expect, beforeEach } from 'vitest';
import { announce } from '../../src/ui/announce';

describe('announce', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('creates a polite live region containing the message', () => {
    announce('North played Queen of Spades');
    const region = document.querySelector('[aria-live="polite"]');
    expect(region).not.toBeNull();
    expect(region?.textContent).toBe('North played Queen of Spades');
  });

  it('reuses a single region across calls', () => {
    announce('first');
    announce('second');
    const regions = document.querySelectorAll('[aria-live="polite"]');
    expect(regions).toHaveLength(1);
    expect(regions[0]?.textContent).toBe('second');
  });
});
