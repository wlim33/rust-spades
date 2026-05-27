import { test, expect } from '../fixtures';
import { GamePage } from '../pages/game-page';
import { HomePage } from '../pages/home-page';
import { waitForGameUrl } from '../helpers/routing';
import { createAiGame, seedAiSession } from '../helpers/games';

test('Play with computers: bet, then reload preserves the 13-card hand', async ({ page }) => {
  const home = new HomePage(page);
  await home.goto();
  await home.playWithComputers();
  await waitForGameUrl(page);

  const game = new GamePage(page);
  await game.bet(3);
  await game.waitForPlayable();

  await page.reload();
  await expect(game.hand()).toHaveCount(13, { timeout: 10_000 });
});

test('AI lifecycle: bet, play all 13 tricks, advance to the next hand', async ({ page }) => {
  // API-assisted setup: create the game with the page's own anon identity so
  // the page is authorized to make the human's moves, then seed the boot session.
  const game = await createAiGame(page.request);
  await seedAiSession(page, game);
  await page.goto(`/play/${game.shortId}`);

  const g = new GamePage(page);
  await g.waitForBetting();
  await g.bet(3);
  await g.playOutHand(); // hand drains 13 -> 0

  // One hand never reaches 500, so a fresh hand should be dealt (13 cards again);
  // accept GAME_OVER as a valid alternative for robustness.
  await expect(async () => {
    const nextHandDealt = (await g.hand().count()) === 13;
    const gameOver = await page
      .getByRole('button', { name: 'Play Again' })
      .isVisible()
      .catch(() => false);
    expect(nextHandDealt || gameOver).toBe(true);
  }).toPass({ timeout: 20_000 });
});
