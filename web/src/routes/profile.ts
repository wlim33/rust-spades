import './profile.css';

import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { request, ApiError } from '../api/client';
import { session } from '../state/session';
import type {
  PublicProfile,
  ProfileGames,
  ProfileGameEntry,
  SeatPlayer,
} from '../state/user-types';
import type { RouteModule } from '../router';

/** "Member since" granularity — month + year is enough for a profile header. */
function formatMonthYear(iso: string): string {
  return new Intl.DateTimeFormat(undefined, { year: 'numeric', month: 'short' }).format(
    new Date(iso),
  );
}

const REL = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
const REL_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 31_536_000],
  ['month', 2_592_000],
  ['week', 604_800],
  ['day', 86_400],
  ['hour', 3_600],
  ['minute', 60],
];

/** Coarse relative time ("3 days ago") for last-seen. */
function formatRelative(iso: string): string {
  const diffSec = Math.round((new Date(iso).getTime() - Date.now()) / 1000);
  const abs = Math.abs(diffSec);
  for (const [unit, secs] of REL_UNITS) {
    if (abs >= secs) return REL.format(Math.round(diffSec / secs), unit);
  }
  return 'just now';
}

// Partnerships pair the even seats {0, 2} against the odd seats {1, 3}
// (see crates/spades-server seat semantics / ui/components/scores.ts).
const isTeamA = (p: SeatPlayer): boolean => p.seat_index % 2 === 0;

/** Plain-text partnership names ("Alice & Carol") for aria-labels. */
function teamText(players: SeatPlayer[]): string {
  return players.map((p) => p.name).join(' & ') || '—';
}

const STATE_LABEL: Partial<Record<ProfileGameEntry['state'], string>> = {
  won: 'Won',
  lost: 'Lost',
  tied: 'Tied',
  aborted: 'Aborted',
  in_progress: 'In progress',
  // `unknown` has no label — nothing to show.
};

const hasScore = (e: ProfileGameEntry): boolean =>
  (e.state === 'won' || e.state === 'lost' || e.state === 'tied') &&
  e.team_score != null &&
  e.opp_score != null;

/** Outcome + score for one match, from the profile owner's perspective. Sits
 * where the game id used to. `unknown` (old/pruned games) shows a muted dash. */
function renderState(entry: ProfileGameEntry) {
  const label = STATE_LABEL[entry.state] ?? '—';
  return html`<span class="profile-games__state">
    <span class="profile-games__result is-${entry.state}">${label}</span>
    ${hasScore(entry)
      ? html`<span class="profile-games__score">${entry.team_score}–${entry.opp_score}</span>`
      : nothing}
  </span>`;
}

/** Plain-text outcome appended to the row's aria-label. */
function stateAria(entry: ProfileGameEntry): string {
  const label = STATE_LABEL[entry.state];
  if (!label) return '';
  return hasScore(entry) ? ` — ${label} ${entry.team_score}–${entry.opp_score}` : ` — ${label}`;
}

/** One partnership, with the profile owner's own seat emphasized. */
function renderTeam(players: SeatPlayer[], selfSeat: number) {
  return html`<span class="profile-games__team">
    ${players.map(
      (p, i) =>
        html`${i > 0 ? html`<span class="profile-games__amp">&</span>` : nothing}<span
            class="profile-games__player ${p.seat_index === selfSeat ? 'is-self' : ''}"
            >${p.name}</span
          >`,
    )}
  </span>`;
}

export const profile: RouteModule = {
  render: (params) => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    const username = params['username'] ?? '';
    const prevTitle = document.title;

    const prof = signal<PublicProfile | null>(null);
    const games = signal<ProfileGameEntry[]>([]);
    const total = signal(0);
    const loading = signal(true);
    const loadingMore = signal(false);
    const notFound = signal(false);
    const error = signal<string | null>(null);

    function renderGameRow(entry: ProfileGameEntry) {
      const ordered = [...entry.players].sort((a, b) => a.seat_index - b.seat_index);
      const teamA = ordered.filter(isTeamA);
      const teamB = ordered.filter((p) => !isTeamA(p));
      return html`<li>
        <a
          href="/replay/${entry.game_id}"
          data-link
          class="profile-games__link"
          aria-label="View replay: ${teamText(teamA)} versus ${teamText(
            teamB,
          )}${stateAria(entry)}"
        >
          ${renderState(entry)}
          <span class="profile-games__teams">
            ${renderTeam(teamA, entry.seat_index)}
            <span class="profile-games__vs">vs</span>
            ${renderTeam(teamB, entry.seat_index)}
          </span>
        </a>
      </li>`;
    }

    const dispose = effect(() => {
      // Read all signals eagerly before building the template so lit-html
      // sees concrete values rather than lazy accessors. This is required to
      // avoid a happy-dom / lit-html bug where nested ternary conditionals
      // fail to re-render inner conditional parts when an outer signal changes.
      const l = loading.value;
      const lm = loadingMore.value;
      const nf = notFound.value;
      const err = error.value;
      const p = prof.value;
      const g = games.value;
      const tot = total.value;
      const me = session.currentUser.value?.username ?? null;
      const isMe = p != null && me != null && p.username.toLowerCase() === me.toLowerCase();

      const showEmpty = !l && !nf && !err && p !== null && g.length === 0;
      const showList = !l && !nf && !err && p !== null && g.length > 0;
      const showMore = showList && g.length < tot;

      if (p && !l && !nf) document.title = `${p.username} · Spades`;

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
                  <header class="profile-head">
                    <h2>
                      ${p.username}${isMe
                        ? html`<span class="profile-head__you">You</span>`
                        : nothing}
                    </h2>
                    <p class="profile-head__meta">
                      Member since ${formatMonthYear(p.created_at)}${p.last_seen_at
                        ? html` · Active ${formatRelative(p.last_seen_at)}`
                        : nothing}
                    </p>
                    <p class="profile-head__rating">
                      Rating ${Math.round(p.rating)}<span
                        class="profile-head__rd"
                        title="Rating deviation — lower means a more settled rating"
                        >±${Math.round(p.rd)}</span
                      >
                      <span class="profile-head__games">· ${p.games_played} games</span>
                    </p>
                    ${isMe
                      ? html`<a class="profile-head__edit" href="/me" data-link>Edit profile</a>`
                      : nothing}
                  </header>
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
                  ${g.map(renderGameRow)}
                </ul>`
              : nothing}
            ${showMore
              ? html`<button
                  class="profile-games__more"
                  ?disabled=${lm}
                  @click=${() => void loadMore()}
                >
                  ${lm ? 'Loading…' : `Load more (${g.length} of ${tot})`}
                </button>`
              : nothing}
          </section>
        `),
        root,
      );
    });

    async function loadMore(): Promise<void> {
      if (loadingMore.value) return;
      loadingMore.value = true;
      try {
        const wrapped = await request<ProfileGames>(
          `/users/${encodeURIComponent(username)}/games?offset=${games.value.length}`,
          { method: 'GET' },
        );
        games.value = [...games.value, ...wrapped.games];
        total.value = wrapped.total;
      } catch (e) {
        error.value = e instanceof Error ? e.message : 'Failed to load more games.';
      } finally {
        loadingMore.value = false;
      }
    }

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
        total.value = wrapped.total;
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
      document.title = prevTitle;
      render(nothing, root);
    };
  },
};
