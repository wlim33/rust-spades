import { test, expect } from './fixtures';

test('signup, view /me, logout, login again', async ({ page }) => {
  const stamp = Date.now();
  const email = `e2e-${stamp}@example.com`;
  const username = `e2e_${stamp}`.slice(0, 20);

  // Signup
  await page.goto('/signup');
  await page.locator('#email').fill(email);
  await page.locator('#username').fill(username);
  await page.locator('#password').fill('correcthorse');
  await page.locator('[data-testid=submit]').click();

  // After signup, navigated to /. Header should show the avatar menu.
  await page.waitForFunction(() => location.pathname === '/');
  await expect(page.locator('[data-testid=avatar-menu] summary')).toHaveText(username);

  // Visit /me
  await page.goto('/me');
  await expect(page.locator('#email')).toBeVisible();
  await expect(page.locator('body')).toContainText(email);

  // Sign out
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await page.waitForFunction(() => location.pathname === '/');
  await expect(page.locator('[data-testid=sign-in]')).toBeVisible();

  // Log back in
  await page.goto('/login');
  await page.locator('#email').fill(email);
  await page.locator('#password').fill('correcthorse');
  await page.locator('[data-testid=submit]').click();

  await page.waitForFunction(() => location.pathname === '/');
  await expect(page.locator('[data-testid=avatar-menu] summary')).toHaveText(username);
});
