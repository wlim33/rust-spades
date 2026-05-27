import type { Page } from '@playwright/test';

/** Matches an in-game URL like /play/abc123 but not the /play/new-ai bootstrap. */
export const GAME_URL_RE = /\/play\/(?!new-ai)[^/]+$/;

/** Waits for SPA pushState navigation into a real game URL. */
export async function waitForGameUrl(page: Page, timeout = 15_000): Promise<void> {
  await page.waitForFunction(() => /\/play\/(?!new-ai)[^/]+$/.test(location.pathname), {
    timeout,
  });
}
