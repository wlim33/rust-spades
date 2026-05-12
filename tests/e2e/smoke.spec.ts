import { test, expect } from '@playwright/test';

test('home renders the menu', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle('Spades');
  await expect(page.getByRole('heading', { name: 'Spades' })).toBeVisible();
  await expect(page.locator('[data-testid="home-menu"] button')).toHaveCount(5);
});

test('clicking a quickplay button logs to console', async ({ page }) => {
  const logs: string[] = [];
  page.on('console', (msg) => logs.push(msg.text()));
  await page.goto('/');
  await page.getByRole('button', { name: '5+3' }).click();
  // Console log fires synchronously after click handler.
  await expect.poll(() => logs.some((l) => l.includes('seek quickplay'))).toBe(true);
});

test('unknown route renders 404', async ({ page }) => {
  await page.goto('/no-such-path');
  await expect(page.getByRole('heading', { name: 'Not found' })).toBeVisible();
});
