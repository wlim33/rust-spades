import { html, render, nothing } from 'lit-html';
import { signal, effect } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { navigateTo } from '../lib/util';
import { openSse } from '../api/sse';
import { saveSession } from '../lib/storage';
import { toast } from '../state/toast';
import { queueSizes, startQueuePoll, stopQueuePoll, queueCountFor } from '../state/menu';
import { icon } from '../ui/icon';
import type { RouteModule } from '../router';
import type { TemplateResult } from 'lit-html';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;

type QuickplayState = { waiting: number; cancel: () => void; tier: string } | null;

export const quickplay = signal<QuickplayState>(null);

const oauthBanner = signal<boolean>(false);

const QUICKPLAY_TIMERS: { label: string; tier: string; key: string; value: TimerCfg }[] = [
  {
    label: '5+3',
    tier: 'Blitz',
    key: 'blitz',
    value: { initial_time_secs: 300, increment_secs: 3 },
  },
  {
    label: '10+5',
    tier: 'Rapid',
    key: 'rapid',
    value: { initial_time_secs: 600, increment_secs: 5 },
  },
  {
    label: '15+10',
    tier: 'Classic',
    key: 'classic',
    value: { initial_time_secs: 900, increment_secs: 10 },
  },
];

const TIER_ICON: Record<string, string> = {
  blitz: 'flashlight-fill',
  rapid: 'timer-flash-fill',
  classic: 'hourglass-fill',
};

function onSeek(timer: TimerCfg, tier: string): void {
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
            quickplay.value = { waiting: parsed.waiting as number, cancel, tier };
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
  quickplay.value = { waiting: 0, cancel, tier };
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
      <div class="home-searching" data-testid="home-searching">
        <div class="home-searching__dots" aria-hidden="true"><i></i><i></i><i></i><i></i></div>
        <p class="home-searching__msg">Finding players…</p>
        <p class="home-searching__sub">${q.waiting} of 4 seated · ${q.tier}</p>
        ${button({ label: 'Cancel', onClick: q.cancel, variant: 'secondary' })}
      </div>
    `);
  }
  return appShell(html`
    <div class="banner" ?hidden=${!oauthBanner.value}>
      <span>Finish signing in to keep your account.</span>
      <a class="btn btn--primary" href="/auth/oauth/complete" data-link>Continue</a>
      <button class="btn btn--secondary" type="button" @click=${dismissOauthBanner}>Dismiss</button>
    </div>
    <div class="menu" data-testid="home-menu">
      <p class="menu__label">Quick play</p>
      <div class="menu__quickplay">
        ${QUICKPLAY_TIMERS.map((t) => {
          const count = t.value ? queueCountFor(t.value) : 0;
          return html`<div class="quickplay-col quickplay-col--${t.key}">
            <button class="quickplay-tile" type="button" @click=${() => onSeek(t.value, t.tier)}>
              ${icon(TIER_ICON[t.key]!)}
              <span class="quickplay-tile__time">${t.label}</span>
            </button>
            <span class="quickplay-tile__tier">${t.tier}</span>
            <span class="queue-count">${count > 0 ? `${count} waiting` : 'No one waiting'}</span>
          </div>`;
        })}
      </div>
      <p class="menu__label">Other ways to play</p>
      <button
        class="menu__row menu__row--friends"
        type="button"
        @click=${onFriends}
        data-testid="play-friends"
      >
        <span class="menu__row-icon">${icon('group-fill')}</span>
        <span class="menu__row-text">
          <span class="menu__row-title">Play with friends</span>
          <span class="menu__row-meta">Create a table · share a link · 500 pts</span>
        </span>
        <span class="menu__row-go">${icon('arrow-right-s-line')}</span>
      </button>
      <button
        class="menu__row menu__row--computers"
        type="button"
        @click=${onComputers}
        data-testid="play-computers"
      >
        <span class="menu__row-icon">${icon('robot-2-fill')}</span>
        <span class="menu__row-text">
          <span class="menu__row-title">Play with computers</span>
          <span class="menu__row-meta">1 vs 3 bots · 500 pts · no timer</span>
        </span>
        <span class="menu__row-go">${icon('arrow-right-s-line')}</span>
      </button>
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
