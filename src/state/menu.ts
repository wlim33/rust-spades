import { signal } from '@preact/signals-core';
import { request } from '../api/client';

export type QueueSize = {
  max_points: number;
  timer_config: { initial_time_secs: number; increment_secs: number };
  waiting: number;
};

export const queueSizes = signal<QueueSize[]>([]);

let timer: ReturnType<typeof setInterval> | null = null;

export async function refreshQueueSizes(): Promise<void> {
  try {
    const data = await request<QueueSize[]>('/matchmaking/queue-sizes', { method: 'GET' });
    if (Array.isArray(data)) queueSizes.value = data;
  } catch {
    // best-effort; ignore
  }
}

export function startQueuePoll(intervalMs = 10_000): void {
  stopQueuePoll();
  void refreshQueueSizes();
  timer = setInterval(() => void refreshQueueSizes(), intervalMs);
}

export function stopQueuePoll(): void {
  if (timer) clearInterval(timer);
  timer = null;
}

export function queueCountFor(timerCfg: QueueSize['timer_config']): number {
  const e = queueSizes.value.find(
    (q) =>
      q.max_points === 500 &&
      q.timer_config.initial_time_secs === timerCfg.initial_time_secs &&
      q.timer_config.increment_secs === timerCfg.increment_secs,
  );
  return e?.waiting ?? 0;
}
