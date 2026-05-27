import { test, expect } from '../fixtures';
import { GamePage } from '../pages/game-page';
import { HomePage } from '../pages/home-page';
import { waitForGameUrl } from '../helpers/routing';

test('Play with computers: bet, then reload preserves the 13-card hand', async ({ page }) => {
  await new HomePage(page).goto();
  await new HomePage(page).playWithComputers();
  await waitForGameUrl(page);

  const game = new GamePage(page);
  await game.bet(3);
  await game.waitForPlayable();

  await page.reload();
  await expect(game.hand()).toHaveCount(13, { timeout: 10_000 });
});
