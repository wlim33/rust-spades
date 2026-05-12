import { test, expect } from './setup';

test('anonymous AI game — bet + reload preserves hand', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Play with Computers' }).click();

  // Wait for SPA navigation away from /play/new-ai to the real game short ID.
  // pushState-based routing doesn't fire a load event, so poll location.pathname.
  await page.waitForFunction(() => /\/play\/(?!new-ai)[^/]+$/.test(location.pathname), {
    timeout: 15_000,
  });

  // BETTING phase should show bet buttons.
  await page.waitForSelector('.spades-bets button', { state: 'visible', timeout: 10_000 });
  await page.locator('.spades-bets').getByRole('button', { name: '3', exact: true }).click();

  // Wait for PLAYING phase (cards become clickable).
  await page.waitForSelector('.cm-clickable', { state: 'visible', timeout: 10_000 });

  // Reload should preserve game state — the hand-container should still hold cards.
  await page.reload();
  await expect(page.locator('.hand-container .card')).toHaveCount(13, { timeout: 5_000 });
});
