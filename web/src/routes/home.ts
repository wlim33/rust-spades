import { html, render, nothing } from 'lit-html';
import { signal, effect } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { navigateTo } from '../lib/util';
import { openSse } from '../api/sse';
import { saveSession } from '../lib/storage';
import { toast } from '../state/toast';
import { queueSizes, startQueuePoll, stopQueuePoll, queueCountFor } from '../state/menu';
import type { RouteModule } from '../router';
import type { TemplateResult } from 'lit-html';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;

type QuickplayState = { waiting: number; cancel: () => void } | null;

export const quickplay = signal<QuickplayState>(null);

const oauthBanner = signal<boolean>(false);

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

function dismissOauthBanner(): void {
  try {
    sessionStorage.removeItem('spades_oauth_lingering');
  } catch {
    // ignore
  }
  oauthBanner.value = false;
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
    <div class="banner" ?hidden=${!oauthBanner.value}>
      <span>Finish signing in to keep your account.</span>
      <a class="btn btn--primary" href="/auth/oauth/complete" data-link>Continue</a>
      <button class="btn btn--secondary" type="button" @click=${dismissOauthBanner}>Dismiss</button>
    </div>
    <div class="menu" data-testid="home-menu">
      <p class="menu__label">Quick Play</p>
      <div class="menu__quickplay">
        ${QUICKPLAY_TIMERS.map((t) => {
          const count = t.value ? queueCountFor(t.value) : 0;
          return html`<div class="quickplay-col">
            ${button({
              label: t.label,
              onClick: () => onSeek(t.value),
              variant: 'primary',
            })}
            <span class="queue-count">${count > 0 ? `${count} waiting` : 'No one waiting'}</span>
          </div>`;
        })}
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
    try {
      oauthBanner.value = sessionStorage.getItem('spades_oauth_lingering') === '1';
    } catch {
      oauthBanner.value = false;
    }
    startQueuePoll();
    const dispose = effect(() => {
      void queueSizes.value;
      render(template(), root);
    });
    return () => {
      if (quickplay.value) quickplay.value.cancel();
      dispose();
      stopQueuePoll();
      render(nothing, root);
    };
  },
};
