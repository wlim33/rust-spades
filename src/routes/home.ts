import { html, render, nothing } from 'lit-html';
import { signal, effect } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { navigateTo } from '../lib/util';
import { openSse } from '../api/sse';
import { saveSession } from '../lib/storage';
import { toast } from '../state/toast';
import type { RouteModule } from '../router';
import type { TemplateResult } from 'lit-html';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;

type QuickplayState = { waiting: number; cancel: () => void } | null;

export const quickplay = signal<QuickplayState>(null);

const QUICKPLAY_TIMERS: { label: string; value: TimerCfg }[] = [
  { label: '5+3', value: { initial_time_secs: 300, increment_secs: 3 } },
  { label: '10+5', value: { initial_time_secs: 600, increment_secs: 5 } },
  { label: '15+10', value: { initial_time_secs: 900, increment_secs: 10 } },
];

function onSeek(timer: TimerCfg): void {
  if (quickplay.value) return;
  let handle: ReturnType<typeof openSse> | null = null;
  const cancel = (): void => {
    handle?.close();
    quickplay.value = null;
  };
  handle = openSse(
    '/matchmaking/seek',
    { max_points: 500, timer_config: timer },
    {
      onEvent: (eventType, data) => {
        try {
          const parsed = JSON.parse(data) as Record<string, unknown>;
          if (eventType === 'queue_status') {
            quickplay.value = { waiting: parsed.waiting as number, cancel };
          } else if (eventType === 'game_start') {
            const shortId =
              (parsed.short_id as string | undefined) ??
              (parsed.player_url as string | undefined) ??
              (parsed.game_id as string | undefined) ??
              '';
            const playerId =
              (parsed.player_short_id as string | undefined) ??
              (parsed.player_id as string | undefined) ??
              '';
            const gameId = (parsed.game_id as string | undefined) ?? '';
            saveSession(shortId, gameId, playerId);
            cancel();
            navigateTo(`/play/${shortId}`);
          }
        } catch {
          // ignore parse errors
        }
      },
      onError: () => {
        toast.error('Failed to find match.');
        cancel();
      },
    },
  );
  quickplay.value = { waiting: 0, cancel };
}

function onFriends(): void {
  navigateTo('/create');
}

function onComputers(): void {
  navigateTo('/play/new-ai');
}

function template(): TemplateResult {
  if (quickplay.value) {
    const q = quickplay.value;
    return appShell(html`
      <h1>Spades</h1>
      <div class="quickplay-wait">
        <p>Finding players… (${q.waiting}/4)</p>
        ${button({ label: 'Cancel', onClick: q.cancel, variant: 'secondary' })}
      </div>
    `);
  }
  return appShell(html`
    <h1>Spades</h1>
    <div class="menu" data-testid="home-menu">
      <p class="menu__label">Quick Play</p>
      <div class="menu__quickplay">
        ${QUICKPLAY_TIMERS.map((t) =>
          button({
            label: t.label,
            onClick: () => onSeek(t.value),
            variant: 'primary',
          }),
        )}
      </div>
      ${button({ label: 'Play with Friends', onClick: onFriends, variant: 'secondary' })}
      ${button({ label: 'Play with Computers', onClick: onComputers, variant: 'secondary' })}
    </div>
  `);
}

export const home: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    const dispose = effect(() => {
      render(template(), root);
    });
    return () => {
      if (quickplay.value) quickplay.value.cancel();
      dispose();
      render(nothing, root);
    };
  },
};
