import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { installGlobalErrorHandlers } from '../../src/lib/global-errors';
import { toast } from '../../src/state/toast';

describe('installGlobalErrorHandlers', () => {
  let dispose: () => void;

  beforeEach(() => {
    toast.toasts.value = [];
    vi.spyOn(console, 'error').mockImplementation(() => {});
    dispose = installGlobalErrorHandlers();
  });
  afterEach(() => {
    dispose();
    vi.restoreAllMocks();
  });

  it('does NOT toast on uncaught window errors (incl. benign ResizeObserver loop noise)', () => {
    window.dispatchEvent(
      new ErrorEvent('error', {
        message: 'ResizeObserver loop completed with undelivered notifications.',
      }),
    );
    expect(toast.toasts.value).toHaveLength(0);
    expect(console.error).toHaveBeenCalled();
  });

  it('still toasts on unhandled promise rejections (real async failures)', () => {
    const ev = new Event('unhandledrejection') as Event & { reason: unknown };
    ev.reason = new Error('boom');
    window.dispatchEvent(ev);
    expect(toast.toasts.value).toHaveLength(1);
    expect(toast.toasts.value[0]?.kind).toBe('error');
  });
});
