import { setPos, type CardEl } from './card-el';

export type EaseFn = (t: number) => number;

export const EASE: Record<'linear' | 'quartIn' | 'quartOut' | 'backOut', EaseFn> = {
  linear: (t) => t,
  quartOut: (t) => {
    const u = t - 1;
    return 1 - u * u * u * u;
  },
  quartIn: (t) => t * t * t * t,
  // Back ease out: overshoots past 1 and settles back.
  // Formula: 1 + c3 * u³ + OVERSHOOT * u²  where u = t − 1, c3 = OVERSHOOT + 1.
  backOut: (t) => {
    const OVERSHOOT = 1.2;
    const u = t - 1;
    const c3 = OVERSHOOT + 1;
    return 1 + c3 * u * u * u + OVERSHOOT * u * u;
  },
};

export type AnimateOpts = {
  x: number;
  y: number;
  duration?: number;
  delay?: number;
  ease?: keyof typeof EASE;
  onStart?: () => void;
  onProgress?: (raw: number, eased: number) => void;
  onComplete?: () => void;
  /** Stop early and settle at the final position when this returns true. */
  cancelled?: () => boolean;
};

export function animateTo(el: CardEl, opts: AnimateOpts): Promise<void> {
  return new Promise((resolve) => {
    const startX = el._cm.x;
    const startY = el._cm.y;
    const easeFn = EASE[opts.ease ?? 'quartOut'];
    const duration = opts.duration ?? 300;
    const run = (): void => {
      const startTime = performance.now();
      if (opts.onStart) opts.onStart();
      let settled = false;
      const finish = (snapToEnd: boolean): void => {
        if (settled) return;
        settled = true;
        clearTimeout(watchdog);
        if (snapToEnd) setPos(el, opts.x, opts.y);
        if (opts.onComplete) opts.onComplete();
        resolve();
      };
      // Wall-clock safety net: requestAnimationFrame is paused in background
      // tabs, so a flight started while visible can stall forever and block the
      // whole serial animation chain (and the player's next turn). setTimeout
      // still fires (throttled) when hidden, so it guarantees settlement.
      const watchdog = setTimeout(() => finish(true), duration + 1000);
      const tick = (now: number): void => {
        if (settled) return;
        if (opts.cancelled?.()) {
          finish(true);
          return;
        }
        const elapsed = now - startTime;
        const raw = Math.min(elapsed / duration, 1);
        const t = easeFn(raw);
        const cx = startX + (opts.x - startX) * t;
        const cy = startY + (opts.y - startY) * t;
        setPos(el, cx, cy);
        if (opts.onProgress) opts.onProgress(raw, t);
        if (raw < 1) requestAnimationFrame(tick);
        else finish(false);
      };
      requestAnimationFrame(tick);
    };
    if (opts.delay && opts.delay > 0) setTimeout(run, opts.delay);
    else run();
  });
}
