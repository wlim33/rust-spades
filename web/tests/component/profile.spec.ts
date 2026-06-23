import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { profile } from '../../src/routes/profile';

describe('profile route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    vi.unstubAllGlobals();
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders the username and games list on success', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (url.endsWith('/users/alice')) {
          return new Response(
            JSON.stringify({
              username: 'alice',
              created_at: '2026-01-01',
              games_played: 7,
              last_seen_at: null,
              rating: 1500,
              rd: 100,
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        if (url.endsWith('/users/alice/games')) {
          return new Response(
            JSON.stringify({
              username: 'alice',
              limit: 20,
              offset: 0,
              total: 1,
              games: [
                {
                  game_id: 'g1abcdef-0000-0000-0000-000000000000',
                  seat_index: 0,
                  player_id: 'p1',
                  players: [
                    { seat_index: 0, name: 'alice', is_bot: false },
                    { seat_index: 1, name: 'Bob', is_bot: false },
                    { seat_index: 2, name: 'Bot', is_bot: true },
                    { seat_index: 3, name: 'Guest', is_bot: false },
                  ],
                  state: 'won',
                  team_score: 312,
                  opp_score: 245,
                },
              ],
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        return new Response('not found', { status: 404 });
      }),
    );
    const cleanup = profile.render(
      { username: 'alice' },
      { path: '/u/alice', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent).toContain('alice');
    expect(document.querySelector('.profile-page.panel')).not.toBeNull();
    // The game id is no longer shown as text; the replay link carries it.
    expect(document.querySelector('.profile-games__id')).toBeNull();
    // All four players are shown, grouped by team, with the profile owner
    // (seat 0 = "alice") emphasized and everyone else present.
    const selfPlayer = document.querySelector('.profile-games__player.is-self');
    expect(selfPlayer?.textContent?.trim()).toBe('alice');
    expect(document.querySelectorAll('.profile-games__player').length).toBe(4);
    expect(document.querySelectorAll('.profile-games__team').length).toBe(2);
    expect(document.querySelector('.profile-games__vs')).not.toBeNull();
    ['Bob', 'Bot', 'Guest'].forEach((n) => expect(document.body.textContent).toContain(n));
    // Match state: a "Won" result with the score, from alice's perspective.
    const result = document.querySelector('.profile-games__result');
    expect(result?.textContent?.trim()).toBe('Won');
    expect(result?.classList.contains('is-won')).toBe(true);
    expect(document.querySelector('.profile-games__score')?.textContent).toBe('312–245');
    // Rating is rounded and carries its deviation, like the leaderboard.
    expect(document.querySelector('.profile-head__rating')?.textContent).toContain('1500');
    expect(document.querySelector('.profile-head__rd')?.textContent).toContain('±100');
    expect(document.querySelector('.profile-head__meta')?.textContent).toContain('Member since');
    const link = document.querySelector<HTMLAnchorElement>('.profile-games a[data-link]');
    expect(link).not.toBeNull();
    expect(link?.getAttribute('href')).toBe('/replay/g1abcdef-0000-0000-0000-000000000000');
    // total === games length, so no "Load more".
    expect(document.querySelector('.profile-games__more')).toBeNull();
    cleanup();
  });

  it('paginates with Load more when total exceeds the first page', async () => {
    const page1 = {
      username: 'alice',
      limit: 1,
      offset: 0,
      total: 2,
      games: [
        {
          game_id: 'g1aaaaaa-0000-0000-0000-000000000000',
          seat_index: 1,
          player_id: 'p1',
          players: [
            { seat_index: 0, name: 'Carol', is_bot: false },
            { seat_index: 1, name: 'alice', is_bot: false },
            { seat_index: 2, name: 'Dave', is_bot: false },
            { seat_index: 3, name: 'Eve', is_bot: false },
          ],
          state: 'in_progress',
          team_score: null,
          opp_score: null,
        },
      ],
    };
    const page2 = {
      username: 'alice',
      limit: 1,
      offset: 1,
      total: 2,
      games: [
        {
          game_id: 'g2bbbbbb-0000-0000-0000-000000000000',
          seat_index: 2,
          player_id: 'p1',
          players: [
            { seat_index: 0, name: 'Frank', is_bot: false },
            { seat_index: 1, name: 'Grace', is_bot: false },
            { seat_index: 2, name: 'alice', is_bot: false },
            { seat_index: 3, name: 'Heidi', is_bot: false },
          ],
          state: 'lost',
          team_score: 180,
          opp_score: 320,
        },
      ],
    };
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (url.endsWith('/users/alice')) {
          return new Response(
            JSON.stringify({
              username: 'alice',
              created_at: '2026-01-01',
              games_played: 2,
              last_seen_at: '2026-06-20',
              rating: 1500,
              rd: 100,
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        if (url.includes('offset=1')) {
          return new Response(JSON.stringify(page2), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          });
        }
        if (url.endsWith('/users/alice/games')) {
          return new Response(JSON.stringify(page1), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          });
        }
        return new Response('not found', { status: 404 });
      }),
    );
    const cleanup = profile.render(
      { username: 'alice' },
      { path: '/u/alice', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));

    const more = document.querySelector<HTMLButtonElement>('.profile-games__more');
    expect(more).not.toBeNull();
    expect(more?.textContent).toContain('1 of 2');
    expect(document.querySelectorAll('.profile-games li').length).toBe(1);
    // First page's live game shows "In progress" with no score.
    expect(document.querySelector('.profile-games__result')?.textContent?.trim()).toBe(
      'In progress',
    );
    expect(document.querySelector('.profile-games__score')).toBeNull();

    more!.click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));

    expect(document.querySelectorAll('.profile-games li').length).toBe(2);
    // The second page's game is appended (its replay link carries the id).
    expect(
      document.querySelector('.profile-games li:last-child a[data-link]')?.getAttribute('href'),
    ).toBe('/replay/g2bbbbbb-0000-0000-0000-000000000000');
    // Second page's finished game shows the lost result + score.
    expect(document.body.textContent).toContain('Lost');
    expect(document.body.textContent).toContain('180–320');
    // Fully loaded — the button disappears.
    expect(document.querySelector('.profile-games__more')).toBeNull();
    cleanup();
  });

  it('shows not-found on 404', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('not found', { status: 404 })),
    );
    const cleanup = profile.render(
      { username: 'ghost' },
      { path: '/u/ghost', search: new URLSearchParams() },
    );
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent?.toLowerCase()).toContain('not found');
    cleanup();
  });
});
