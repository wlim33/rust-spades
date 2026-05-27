import { test, expect } from '@playwright/test';

test('home renders the menu', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle('Spades');
  await expect(page.locator('.site-title')).toBeVisible();
  await expect(page.locator('[data-testid="home-menu"] button')).toHaveCount(5);
});

test('clicking a quickplay button shows the waiting view', async ({ page }) => {
  // Intercept the matchmaking SSE call and return a never-ending stream so
  // the UI stays in the "Finding players" waiting state long enough to assert.
  await page.route('**/matchmaking/seek', async (route) => {
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
      body: '',
    });
  });
  await page.goto('/');
  await page.getByRole('button', { name: '5+3' }).click();
  // After clicking, the UI transitions to the waiting state.
  await expect(page.getByText('Finding players')).toBeVisible();
  // Cancel returns to the menu.
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.locator('[data-testid="home-menu"]')).toBeVisible();
});

test('unknown route renders 404', async ({ page }) => {
  await page.goto('/no-such-path');
  await expect(page.getByRole('heading', { name: 'Not found' })).toBeVisible();
});
