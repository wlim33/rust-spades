import { describe, it, expect, vi } from 'vitest';
import { attachKeyboard } from '../../src/cards/keyboard';

describe('attachKeyboard', () => {
  it('makes the element focusable and activates on Enter and Space', () => {
    const el = document.createElement('div');
    const onActivate = vi.fn();
    attachKeyboard(el, onActivate);

    expect(el.tabIndex).toBe(0);
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    el.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
    expect(onActivate).toHaveBeenCalledTimes(2);
  });

  it('stops activating and is no longer focusable after cleanup', () => {
    const el = document.createElement('div');
    const onActivate = vi.fn();
    const cleanup = attachKeyboard(el, onActivate);

    cleanup();
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(onActivate).not.toHaveBeenCalled();
    expect(el.hasAttribute('tabindex')).toBe(false);
  });

  it('ignores other keys', () => {
    const el = document.createElement('div');
    const onActivate = vi.fn();
    attachKeyboard(el, onActivate);

    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }));
    expect(onActivate).not.toHaveBeenCalled();
  });
});
