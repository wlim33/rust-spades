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
                { game_id: 'g1abcdef-0000-0000-0000-000000000000', seat_index: 0, player_id: 'p1' },
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
    expect(document.body.textContent).toContain('g1abcdef');
    expect(document.querySelector('.profile-page.panel')).not.toBeNull();
    expect(document.querySelector('.profile-games code')?.textContent).toBe('g1abcdef');
    expect(document.querySelector('.profile-games__seat')?.textContent).toContain('0');
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
