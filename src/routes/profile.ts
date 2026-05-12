import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { request, ApiError } from '../api/client';
import type { ProfileResponse, GameHistoryItem } from '../state/user-types';
import type { RouteModule } from '../router';

export const profile: RouteModule = {
  render: (params) => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    const username = params['username'] ?? '';

    const prof = signal<ProfileResponse | null>(null);
    const games = signal<GameHistoryItem[]>([]);
    const loading = signal(true);
    const notFound = signal(false);
    const error = signal<string | null>(null);

    const dispose = effect(() => {
      // Read all signals eagerly before building the template so lit-html
      // sees concrete values rather than lazy accessors. This is required to
      // avoid a happy-dom / lit-html bug where nested ternary conditionals
      // fail to re-render inner conditional parts when an outer signal changes.
      const l = loading.value;
      const nf = notFound.value;
      const err = error.value;
      const p = prof.value;
      const g = games.value;

      render(
        appShell(html`
          <section class="profile-page">
            ${l ? html`<p>Loading…</p>` : nothing}
            ${!l && nf
              ? html`<h2>Not found</h2>
                  <p>No player named <code>${username}</code>.</p>`
              : nothing}
            ${!l && !nf && err ? html`<p class="field-error">${err}</p>` : nothing}
            ${!l && !nf && !err && p
              ? html`
                  <h2>${p.display_name || p.username}</h2>
                  <p class="profile-username">@${p.username}</p>
                  <p>${p.games_played} games played</p>
                  <h3>Recent games</h3>
                `
              : nothing}
            ${!l && !nf && !err && p && g.length === 0 ? html`<p>No games yet.</p>` : nothing}
            ${!l && !nf && !err && p && g.length > 0
              ? html`<ul class="profile-games">
                  ${g.map(
                    (item) =>
                      html`<li>
                        <a href=${`/play/${item.game_id}`} data-link>${item.game_id}</a>
                        <span> — ${item.won ? 'Won' : 'Lost'} (Team ${item.team})</span>
                      </li>`,
                  )}
                </ul>`
              : nothing}
          </section>
        `),
        root,
      );
    });

    void (async () => {
      try {
        const [profData, gamesData] = await Promise.all([
          request<ProfileResponse>(`/users/${encodeURIComponent(username)}`, { method: 'GET' }),
          request<GameHistoryItem[]>(`/users/${encodeURIComponent(username)}/games`, {
            method: 'GET',
          }),
        ]);
        prof.value = profData;
        games.value = gamesData;
      } catch (e) {
        if (e instanceof ApiError && e.status === 404) {
          notFound.value = true;
        } else {
          error.value = e instanceof Error ? e.message : 'Failed to load profile.';
        }
      } finally {
        loading.value = false;
      }
    })();

    return () => {
      dispose();
      render(nothing, root);
    };
  },
};
