import { html, type TemplateResult } from 'lit-html';
import { signal } from '@preact/signals-core';
import { request } from '../../api/client';
import { icon } from '../icon';
import type { Leaderboard, LeaderboardPeriod } from '../../state/user-types';

// The landing preview shows the top few; the API already caps at 10.
const PREVIEW_SIZE = 5;

// Module-level signals (mirrors home.ts's `quickplay` pattern).
const period = signal<LeaderboardPeriod>('all-time');
const board = signal<Leaderboard | null>(null);
const loading = signal(true);
const error = signal<string | null>(null);

// Epoch guard: a slow response must not overwrite a newer one
// (same technique as routes/leaderboard.ts).
let loadEpoch = 0;

async function load(p: LeaderboardPeriod): Promise<void> {
  const epoch = ++loadEpoch;
  loading.value = true;
  error.value = null;
  try {
    const data = await request<Leaderboard>(`/leaderboard?period=${p}`, { method: 'GET' });
    if (epoch !== loadEpoch) return; // a newer load superseded this one
    board.value = data;
  } catch (e) {
    if (epoch !== loadEpoch) return;
    error.value = e instanceof Error ? e.message : 'Failed to load leaderboard.';
  } finally {
    if (epoch === loadEpoch) loading.value = false;
  }
}

/** Begin loading. Call BEFORE the host's render effect runs so the first paint
 *  is in a loading posture (no empty-state flash). */
export function startLeaderboardPreview(): void {
  void load(period.value);
}

/** Tear down: invalidate any in-flight load and reset to initial state, so a
 *  late response can't write into a torn-down root and the next mount is clean. */
export function stopLeaderboardPreview(): void {
  loadEpoch++;
  period.value = 'all-time';
  board.value = null;
  loading.value = true;
  error.value = null;
}

export function leaderboardPreview(): TemplateResult {
  const entries = (board.value?.entries ?? []).slice(0, PREVIEW_SIZE);
  return html`
    <section
      class="home-leaderboard panel"
      aria-labelledby="home-lb-title"
      data-testid="home-leaderboard"
    >
      <div class="home-leaderboard__head">
        <h2 id="home-lb-title" class="home-leaderboard__title">Top players</h2>
        <a class="home-leaderboard__more" href="/leaderboard" data-link
          >View full leaderboard ${icon('arrow-right-s-line')}</a
        >
      </div>
      <ol class="leaderboard__list">
        ${entries.map(
          (e) =>
            html`<li class="leaderboard__row">
              <span class="leaderboard__rank">${e.rank}</span>
              <a class="leaderboard__name" href="/u/${encodeURIComponent(e.username)}" data-link
                >${e.username}</a
              >
              <span class="leaderboard__rating">${e.rating}</span>
            </li>`,
        )}
      </ol>
    </section>
  `;
}
