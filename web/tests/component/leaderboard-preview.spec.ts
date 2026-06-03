import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { home, quickplay } from '../../src/routes/home';

type Entry = {
  rank: number;
  username: string;
  rating: number;
  rd: number;
  games_played: number;
  score: number;
};

function entry(rank: number, username: string, rating: number): Entry {
  return { rank, username, rating, rd: 50, games_played: 10, score: rating - 100 };
}

// Leaderboard JSON for /leaderboard; empty array for the queue poll
// (refreshQueueSizes ignores non-arrays, but [] keeps the stub explicit).
function stubLeaderboardFetch(entries: Entry[], period = 'all-time'): ReturnType<typeof vi.fn> {
  return vi.fn(async (url: string) => {
    if (typeof url === 'string' && url.includes('/leaderboard')) {
      return new Response(JSON.stringify({ period, entries }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }
    return new Response(JSON.stringify([]), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
}

async function flush(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

function renderHome(): () => void {
  return home.render({}, { path: '/', search: new URLSearchParams() });
}

describe('home leaderboard preview', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    quickplay.value = null;
    vi.unstubAllGlobals();
  });
  afterEach(() => {
    quickplay.value = null;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('shows at most five rows even when the API returns ten', async () => {
    const tens = Array.from({ length: 10 }, (_, i) =>
      entry(i + 1, `player${i + 1}`, 1900 - i * 10),
    );
    vi.stubGlobal('fetch', stubLeaderboardFetch(tens));
    const cleanup = renderHome();
    await flush();
    expect(document.querySelector('[data-testid="home-leaderboard"]')).not.toBeNull();
    expect(
      document.querySelectorAll('[data-testid="home-leaderboard"] .leaderboard__row').length,
    ).toBe(5);
    expect(document.body.textContent).toContain('player1');
    expect(document.body.textContent).not.toContain('player6');
    cleanup();
  });

  it('links rows to profiles and the header to the full board', async () => {
    vi.stubGlobal('fetch', stubLeaderboardFetch([entry(1, 'alice', 1800)]));
    const cleanup = renderHome();
    await flush();
    const nameLink = document.querySelector(
      '[data-testid="home-leaderboard"] .leaderboard__name',
    ) as HTMLAnchorElement;
    expect(nameLink.getAttribute('href')).toBe('/u/alice');
    const moreLink = document.querySelector('.home-leaderboard__more') as HTMLAnchorElement;
    expect(moreLink.getAttribute('href')).toBe('/leaderboard');
    cleanup();
  });

  it('renders below the menu without removing the join buttons', async () => {
    vi.stubGlobal('fetch', stubLeaderboardFetch([entry(1, 'alice', 1800)]));
    const cleanup = renderHome();
    await flush();
    const menu = document.querySelector('[data-testid="home-menu"]');
    const preview = document.querySelector('[data-testid="home-leaderboard"]');
    expect(menu).not.toBeNull();
    expect(document.querySelector('[data-testid="play-friends"]')).not.toBeNull();
    expect(preview).not.toBeNull();
    // Placement guarantee: the preview comes AFTER the menu in the DOM, so the
    // join buttons stay first/primary.
    expect(menu!.compareDocumentPosition(preview!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    cleanup();
  });

  it('switches to this-month and refetches with that period', async () => {
    const periods: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (typeof url === 'string' && url.includes('/leaderboard')) {
          periods.push(url.includes('this-month') ? 'this-month' : 'all-time');
          return new Response(JSON.stringify({ period: 'x', entries: [] }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          });
        }
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    const cleanup = renderHome();
    await flush();
    const tab = document.querySelector('[data-testid="home-tab-this-month"]') as HTMLButtonElement;
    expect(tab).not.toBeNull();
    tab.click();
    await flush();
    expect(periods).toContain('this-month');
    expect(tab.getAttribute('aria-pressed')).toBe('true');
    cleanup();
  });

  it('shows a quiet unavailable message on failure and keeps the join menu', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new Error('network down');
      }),
    );
    const cleanup = renderHome();
    await flush();
    expect(document.body.textContent).toContain('Leaderboard unavailable.');
    // Not the loud red field-error treatment the full page uses.
    expect(document.querySelector('[data-testid="home-leaderboard"] .field-error')).toBeNull();
    // The core guarantee: a leaderboard failure never removes the join buttons.
    expect(document.querySelector('[data-testid="home-menu"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="play-friends"]')).not.toBeNull();
    cleanup();
  });

  it('shows an empty state when no players are ranked', async () => {
    vi.stubGlobal('fetch', stubLeaderboardFetch([]));
    const cleanup = renderHome();
    await flush();
    expect(document.body.textContent?.toLowerCase()).toContain('no ranked players');
    cleanup();
  });
});
