import { test, newPlayerContext } from '../fixtures';
import { HomePage } from '../pages/home-page';
import { CreatePage } from '../pages/create-page';
import { LobbyPage } from '../pages/lobby-page';
import { GamePage } from '../pages/game-page';
import { waitForGameUrl } from '../helpers/routing';

test('friends challenge: create, four players join, reach betting', async ({ browser }) => {
  const players = await Promise.all([0, 1, 2, 3].map(() => newPlayerContext(browser)));
  try {
    // Creator opens a challenge with four empty seats and lands in the lobby.
    const creator = players[0]!.page;
    await new HomePage(creator).goto();
    await new HomePage(creator).playWithFriends();
    await creator.waitForURL(/\/create$/);
    await new CreatePage(creator).create();
    await creator.waitForFunction(() => /\/play\/[^/]+$/.test(location.pathname), {
      timeout: 15_000,
    });
    const shareUrl = creator.url();

    // Each player claims the first open seat. Sequential so two players never
    // grab the same seat at the same instant.
    for (let i = 0; i < players.length; i++) {
      const p = players[i]!.page;
      if (i > 0) await p.goto(shareUrl);
      await new LobbyPage(p).joinFirstOpenSeat(`Player${i + 1}`);
    }

    // When the fourth seat fills, everyone navigates into the game and bets.
    await Promise.all(players.map((p) => waitForGameUrl(p.page)));
    await Promise.all(players.map((p) => new GamePage(p.page).waitForBetting()));
  } finally {
    await Promise.all(players.map((p) => p.context.close()));
  }
});
