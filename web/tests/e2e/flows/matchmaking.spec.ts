import { test, newPlayerContext } from '../fixtures';
import { HomePage } from '../pages/home-page';
import { GamePage } from '../pages/game-page';
import { waitForGameUrl } from '../helpers/routing';

test('four players matched via quickplay reach the betting phase', async ({ browser }) => {
  const players = await Promise.all([0, 1, 2, 3].map(() => newPlayerContext(browser)));
  try {
    await Promise.all(players.map((p) => new HomePage(p.page).goto()));
    await Promise.all(players.map((p) => new HomePage(p.page).quickplay('5+3')));

    // All four land in a real game and reach BETTING, regardless of arrival order.
    await Promise.all(players.map((p) => waitForGameUrl(p.page)));
    await Promise.all(players.map((p) => new GamePage(p.page).waitForBetting()));
  } finally {
    await Promise.all(players.map((p) => p.context.close()));
  }
});
