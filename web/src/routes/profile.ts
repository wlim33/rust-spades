import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { request, ApiError } from '../api/client';
import type { PublicProfile, ProfileGames, ProfileGameEntry } from '../state/user-types';
import type { RouteModule } from '../router';

export const profile: RouteModule = {
  render: (params) => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    const username = params['username'] ?? '';

    const prof = signal<PublicProfile | null>(null);
    const games = signal<ProfileGameEntry[]>([]);
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

      const showEmpty = !l && !nf && !err && p !== null && g.length === 0;
      const showList = !l && !nf && !err && p !== null && g.length > 0;

      render(
        appShell(html`
          <section class="profile-page panel">
            ${l ? html`<p>Loading…</p>` : nothing}
            ${!l && nf
              ? html`<h2>Not found</h2>
                  <p>No player named <code>${username}</code>.</p>`
              : nothing}
            ${!l && !nf && err ? html`<p class="field-error">${err}</p>` : nothing}
            ${p && !l && !nf
              ? html`
                  <h2>${p.username}</h2>
                  <p>${p.games_played} games played · Rating ${p.rating}</p>
                  <h3>Recent games</h3>
                `
              : nothing}
            ${showEmpty
              ? html`<div class="empty-state">
                  <p><strong>${p!.username}</strong> hasn't finished any games yet.</p>
                </div>`
              : nothing}
            ${showList
              ? html`<ul class="profile-games">
                  ${g.map(
                    (entry) =>
                      html`<li>
                        <code>${entry.game_id.slice(0, 8)}</code>
                        <span class="profile-games__seat">Seat ${entry.seat_index}</span>
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
        prof.value = await request<PublicProfile>(`/users/${encodeURIComponent(username)}`, {
          method: 'GET',
        });
        const wrapped = await request<ProfileGames>(
          `/users/${encodeURIComponent(username)}/games`,
          { method: 'GET' },
        );
        games.value = wrapped.games;
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
