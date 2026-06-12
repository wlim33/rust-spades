import { test, newPlayerContext } from '../fixtures';
import { HomePage } from '../pages/home-page';
import { CreatePage } from '../pages/create-page';
import { LobbyPage } from '../pages/lobby-page';
import { GamePage } from '../pages/game-page';
import { waitForGameUrl } from '../helpers/routing';

test('friends challenge: create, four players join, reach betting', async ({ browser }) => {
  const players = await Promise.all([0, 1, 2, 3].map(() => newPlayerContext(browser)));
  try {
    // Creator opens a challenge, takes the default Team A seat, and lands in the lobby.
    const creator = players[0]!.page;
    await new HomePage(creator).goto();
    await new HomePage(creator).playWithFriends();
    await creator.waitForURL(/\/create$/);
    await new CreatePage(creator).create();
    await waitForGameUrl(creator);
    const shareUrl = creator.url();

    // The other three each claim the first joinable team. Sequential so two
    // players never grab the same slot at the same instant.
    for (let i = 1; i < players.length; i++) {
      const p = players[i]!.page;
      await p.goto(shareUrl);
      await new LobbyPage(p).joinFirstOpenTeam(`Player${i + 1}`);
    }

    // When the fourth slot fills, everyone navigates into the game and bets.
    await Promise.all(players.map((p) => waitForGameUrl(p.page)));
    await Promise.all(players.map((p) => new GamePage(p.page).waitForBetting()));
  } finally {
    await Promise.all(players.map((p) => p.context.close()));
  }
});
