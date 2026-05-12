import { setPos, type CardEl } from './card-el';

export type EaseFn = (t: number) => number;

export const EASE: Record<'linear' | 'quartIn' | 'quartOut', EaseFn> = {
  linear: (t) => t,
  quartOut: (t) => {
    const u = t - 1;
    return 1 - u * u * u * u;
  },
  quartIn: (t) => t * t * t * t,
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
      const tick = (now: number): void => {
        const elapsed = now - startTime;
        const raw = Math.min(elapsed / duration, 1);
        const t = easeFn(raw);
        const cx = startX + (opts.x - startX) * t;
        const cy = startY + (opts.y - startY) * t;
        setPos(el, cx, cy);
        if (opts.onProgress) opts.onProgress(raw, t);
        if (raw < 1) requestAnimationFrame(tick);
        else {
          if (opts.onComplete) opts.onComplete();
          resolve();
        }
      };
      requestAnimationFrame(tick);
    };
    if (opts.delay && opts.delay > 0) setTimeout(run, opts.delay);
    else run();
  });
}
