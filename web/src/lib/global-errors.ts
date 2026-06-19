import { toast } from '../state/toast';

/**
 * Install process-wide safety nets for failures that escape local handling.
 *
 * Uncaught *synchronous* window errors are logged only — never toasted. The
 * browser routes benign, spec-defined notifications through `window.onerror`
 * with no real `Error` object; the loudest is "ResizeObserver loop completed
 * with undelivered notifications", emitted whenever a ResizeObserver callback
 * writes layout (see `cards/hand-manager.ts`). On the game table that fired
 * dozens of times while the responsive layout settled, each one surfacing a
 * spurious "Something went wrong." toast. Genuine failures are surfaced at
 * their call sites instead (e.g. "Bet failed." / "Play failed.").
 *
 * Unhandled promise rejections remain the async safety net and still toast:
 * an awaited request that nobody caught is a real failure worth telling the
 * user about, and rejections don't carry the benign-notification noise.
 *
 * Returns a disposer that removes both listeners (used by tests).
 */
export function installGlobalErrorHandlers(): () => void {
  const onError = (e: ErrorEvent): void => {
    console.error('Uncaught error', e.error ?? e.message);
  };
  const onRejection = (e: PromiseRejectionEvent): void => {
    console.error('Unhandled rejection', e.reason);
    toast.error('Something went wrong.');
  };

  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);

  return () => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
  };
}
