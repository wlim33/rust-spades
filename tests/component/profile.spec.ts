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
              display_name: 'Alice',
              created_at: '2026',
              games_played: 7,
              games_won: 4,
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        if (url.endsWith('/users/alice/games')) {
          return new Response(
            JSON.stringify([
              {
                game_id: 'g1',
                started_at: '2026-05-01',
                ended_at: '2026-05-01',
                team: 'A',
                won: true,
                score: 510,
              },
            ]),
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
    expect(document.body.textContent).toContain('g1');
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
