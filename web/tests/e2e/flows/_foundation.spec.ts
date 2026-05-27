import { test, expect } from '../fixtures';
import { HomePage } from '../pages/home-page';

test('authedPage is recognized by GET /auth/me', async ({ authedPage }) => {
  const res = await authedPage.request.get('/auth/me');
  expect(res.ok()).toBe(true);
  const me = (await res.json()) as { username: string };
  expect(me.username).toMatch(/^e2e_/);
});

test('HomePage renders the five-button menu', async ({ page }) => {
  const home = new HomePage(page);
  await home.goto();
  await expect(home.menu()).toBeVisible();
  await expect(home.menu().locator('button')).toHaveCount(5);
});
