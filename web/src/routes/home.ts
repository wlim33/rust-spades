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

// Decorative line icons (aria-hidden); they inherit the per-tier accent via `color`.
const TIER_ICONS: Record<string, TemplateResult> = {
  blitz: html`<svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M13 2 3 14h9l-1 8 9-12h-9z" />
  </svg>`,
  rapid: html`<svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3.5 2" />
  </svg>`,
  classic: html`<svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M6 3h12M6 21h12M8 3v4l4 4 4-4V3M8 21v-4l4-4 4 4v4" />
  </svg>`,
};

const ROW_ICONS: Record<string, TemplateResult> = {
  friends: html`<svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
    <circle cx="9" cy="7" r="4" />
    <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
    <path d="M16 3.13a4 4 0 0 1 0 7.75" />
  </svg>`,
  computers: html`<svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <rect x="4" y="4" width="16" height="16" rx="2" />
    <rect x="9" y="9" width="6" height="6" />
    <path d="M9 2v2M15 2v2M9 20v2M15 20v2M2 9h2M2 15h2M20 9h2M20 15h2" />
  </svg>`,
};

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
      <div class="quickplay-wait">
        <p>Finding players… (${q.waiting}/4)</p>
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
            <button class="quickplay-tile" type="button" @click=${() => onSeek(t.value)}>
              ${TIER_ICONS[t.key]}
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
        <span class="menu__row-icon">${ROW_ICONS.friends}</span>
        <span class="menu__row-text">
          <span class="menu__row-title">Play with friends</span>
          <span class="menu__row-meta">Create a table · share a link · 500 pts</span>
        </span>
      </button>
      <button
        class="menu__row menu__row--computers"
        type="button"
        @click=${onComputers}
        data-testid="play-computers"
      >
        <span class="menu__row-icon">${ROW_ICONS.computers}</span>
        <span class="menu__row-text">
          <span class="menu__row-title">Play with computers</span>
          <span class="menu__row-meta">1 vs 3 bots · 500 pts · no timer</span>
        </span>
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
