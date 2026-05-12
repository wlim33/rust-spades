import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { render } from 'lit-html';
import { toast } from '../../src/state/toast';
import { toastStack } from '../../src/ui/components/toast';

describe('toast', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    toast.toasts.value = [];
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it('renders nothing initially', () => {
    render(toastStack(), document.getElementById('root')!);
    expect(document.querySelectorAll('[data-testid=toast]').length).toBe(0);
  });

  it('error() pushes a toast', () => {
    toast.error('Boom');
    render(toastStack(), document.getElementById('root')!);
    expect(document.querySelectorAll('[data-testid=toast]').length).toBe(1);
    expect(document.querySelector('[data-testid=toast]')?.textContent).toContain('Boom');
  });

  it('auto-dismisses after 4 seconds', () => {
    toast.info('Howdy');
    vi.advanceTimersByTime(4001);
    expect(toast.toasts.value.length).toBe(0);
  });

  it('stacks multiple toasts in order', () => {
    toast.info('A');
    toast.info('B');
    expect(toast.toasts.value.map((t) => t.message)).toEqual(['A', 'B']);
  });
});
