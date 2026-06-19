import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { leaderboard } from '../../src/routes/leaderboard';

function entry(rank: number, username: string, rating: number) {
  return { rank, username, rating, rd: 50, games_played: 10, score: rating - 100 };
}

describe('leaderboard route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    vi.unstubAllGlobals();
  });
  afterEach(() => vi.restoreAllMocks());

  it('renders ranked rows for the default all-time board', async () => {
    const fetchMock = vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            period: 'all-time',
            entries: [entry(1, 'alice', 1700), entry(2, 'bob', 1600)],
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );
    vi.stubGlobal('fetch', fetchMock);
    const cleanup = leaderboard.render({}, { path: '/leaderboard', search: new URLSearchParams() });
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining('period=all-time'),
      expect.anything(),
    );
    expect(document.body.textContent).toContain('alice');
    expect(document.body.textContent).toContain('bob');
    expect(document.querySelectorAll('.leaderboard__row').length).toBe(2);
    cleanup();
  });

  it('shows detailed columns: rounded rating with ±RD, games, and conservative score', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            // 1723.4 rating, rd 50, games 10, score 1623.4 — exercises rounding.
            JSON.stringify({ period: 'all-time', entries: [entry(1, 'alice', 1723.4)] }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      ),
    );
    const cleanup = leaderboard.render({}, { path: '/leaderboard', search: new URLSearchParams() });
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));

    const ratingCell = document.querySelector('tbody .leaderboard__rating');
    expect(ratingCell?.textContent).toContain('1723'); // rounded from 1723.4
    expect(ratingCell?.textContent).toContain('±50');
    expect(document.querySelector('tbody .leaderboard__games')?.textContent?.trim()).toBe('10');
    expect(document.querySelector('tbody .leaderboard__score')?.textContent?.trim()).toBe('1623');
    // Top-3 rank gets neutral emphasis.
    expect(document.querySelector('tbody .leaderboard__rank.is-top')).not.toBeNull();
    cleanup();
  });

  it('switches to this-month and refetches', async () => {
    const periods: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        periods.push(url.includes('this-month') ? 'this-month' : 'all-time');
        return new Response(JSON.stringify({ period: 'x', entries: [] }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    const cleanup = leaderboard.render({}, { path: '/leaderboard', search: new URLSearchParams() });
    await new Promise((r) => setTimeout(r, 0));
    (document.querySelector('[data-testid="tab-this-month"]') as HTMLButtonElement).click();
    // two flushes: one for the refetch promise, one for the resulting re-render
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(periods).toContain('this-month');
    cleanup();
  });

  it('shows an empty state when no players are ranked', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ period: 'all-time', entries: [] }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
      ),
    );
    const cleanup = leaderboard.render({}, { path: '/leaderboard', search: new URLSearchParams() });
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.textContent?.toLowerCase()).toContain('no ranked players');
    cleanup();
  });
});
