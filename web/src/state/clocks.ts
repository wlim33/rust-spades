import { signal } from '@preact/signals-core';

/** Active player's clock is shown in warning color at/below this. */
export const LOW_CLOCK_MS = 15_000;

/** Bumped by the ticker so subscribed renders refresh while a clock runs. */
export const clockTick = signal(0);

let snapshotMs: number | null = null;
let capturedAt = 0;
let timer: ReturnType<typeof setInterval> | null = null;

/** Record the server's active-clock value and when we received it. */
export function captureActiveClock(ms: number | null): void {
  snapshotMs = ms;
  capturedAt = performance.now();
}

/** Active player's remaining ms right now (null when no timed clock). */
export function liveActiveMs(): number | null {
  if (snapshotMs == null) return null;
  return Math.max(0, snapshotMs - (performance.now() - capturedAt));
}

export function startClockTicker(): void {
  if (timer != null) return;
  timer = setInterval(() => {
    clockTick.value = clockTick.value + 1;
  }, 250);
}

export function stopClockTicker(): void {
  if (timer != null) {
    clearInterval(timer);
    timer = null;
  }
}
