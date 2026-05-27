import { test, expect } from '../fixtures';

test('authedPage is recognized by GET /auth/me', async ({ authedPage }) => {
  const res = await authedPage.request.get('/auth/me');
  expect(res.ok()).toBe(true);
  const me = (await res.json()) as { username: string };
  expect(me.username).toMatch(/^e2e_/);
});
