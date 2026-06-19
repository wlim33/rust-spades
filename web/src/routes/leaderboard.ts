import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { request } from '../api/client';
import { session } from '../state/session';
import type { Leaderboard, LeaderboardPeriod } from '../state/user-types';
import type { RouteModule } from '../router';

export const leaderboard: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    const period = signal<LeaderboardPeriod>('all-time');
    const board = signal<Leaderboard | null>(null);
    const loading = signal(true);
    const error = signal<string | null>(null);

    let loadEpoch = 0;
    async function load(p: LeaderboardPeriod): Promise<void> {
      const epoch = ++loadEpoch;
      loading.value = true;
      error.value = null;
      try {
        const data = await request<Leaderboard>(`/leaderboard?period=${p}`, {
          method: 'GET',
        });
        if (epoch !== loadEpoch) return; // a newer load superseded this one
        board.value = data;
      } catch (e) {
        if (epoch !== loadEpoch) return;
        error.value = e instanceof Error ? e.message : 'Failed to load leaderboard.';
      } finally {
        if (epoch === loadEpoch) loading.value = false;
      }
    }

    function selectPeriod(p: LeaderboardPeriod): void {
      if (period.value === p) return;
      period.value = p;
      void load(p);
    }

    const dispose = effect(() => {
      // Read signals eagerly before building the template (see profile.ts:
      // happy-dom/lit-html nested-conditional re-render quirk).
      const l = loading.value;
      const err = error.value;
      const b = board.value;
      const cur = period.value;
      const me = session.currentUser.value?.username ?? null;
      const entries = b?.entries ?? [];
      const showEmpty = !l && !err && entries.length === 0;
      const showList = !l && !err && entries.length > 0;

      render(
        appShell(html`
          <section class="leaderboard panel">
            <h2>Leaderboard</h2>
            <p class="leaderboard__caption">
              Ranked by a conservative Glicko-2 score — each rating discounted by its uncertainty
              (RD), so settled ratings outrank volatile ones. A minimum number of games is required
              to appear.
            </p>
            <div class="leaderboard__tabs" role="group" aria-label="Leaderboard period">
              <button
                class="leaderboard__tab ${cur === 'all-time' ? 'is-active' : ''}"
                data-testid="tab-all-time"
                aria-pressed=${cur === 'all-time'}
                @click=${() => selectPeriod('all-time')}
              >
                All-time
              </button>
              <button
                class="leaderboard__tab ${cur === 'this-month' ? 'is-active' : ''}"
                data-testid="tab-this-month"
                aria-pressed=${cur === 'this-month'}
                @click=${() => selectPeriod('this-month')}
              >
                This month
              </button>
            </div>
            ${l ? html`<p>Loading…</p>` : nothing}
            ${err ? html`<p class="field-error">${err}</p>` : nothing}
            ${showEmpty
              ? html`<div class="empty-state"><p>No ranked players yet.</p></div>`
              : nothing}
            ${showList
              ? html`<div class="leaderboard__scroll">
                  <table class="leaderboard__list">
                    <thead>
                      <tr>
                        <th scope="col" class="leaderboard__rank">#</th>
                        <th scope="col">Player</th>
                        <th scope="col" class="leaderboard__rating">Rating</th>
                        <th scope="col" class="leaderboard__games">Games</th>
                        <th scope="col" class="leaderboard__score">Score</th>
                      </tr>
                    </thead>
                    <tbody>
                      ${entries.map((e) => {
                        const isMe = me != null && e.username.toLowerCase() === me.toLowerCase();
                        return html`<tr class="leaderboard__row ${isMe ? 'is-me' : ''}">
                          <td class="leaderboard__rank ${e.rank <= 3 ? 'is-top' : ''}">
                            ${e.rank}
                          </td>
                          <td class="leaderboard__player">
                            <a
                              class="leaderboard__name"
                              href="/u/${encodeURIComponent(e.username)}"
                              data-link
                              >${e.username}</a
                            >${isMe ? html`<span class="leaderboard__you">You</span>` : nothing}
                          </td>
                          <td class="leaderboard__rating">
                            ${Math.round(e.rating)}<span
                              class="leaderboard__rd"
                              title="Rating deviation — lower means a more settled rating"
                              >±${Math.round(e.rd)}</span
                            >
                          </td>
                          <td class="leaderboard__games">${e.games_played}</td>
                          <td class="leaderboard__score">${Math.round(e.score)}</td>
                        </tr>`;
                      })}
                    </tbody>
                  </table>
                </div>`
              : nothing}
          </section>
        `),
        root,
      );
    });

    void load(period.value);

    return () => {
      dispose();
      render(nothing, root);
    };
  },
};
