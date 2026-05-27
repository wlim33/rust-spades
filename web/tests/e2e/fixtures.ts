import {
  test as base,
  expect,
  type Page,
  type Browser,
  type BrowserContext,
} from '@playwright/test';
import { registerUser } from './helpers/auth';

const BACKEND_URL = process.env.VITE_API_URL ?? 'http://localhost:3000';
// Mirrors `use.baseURL` in playwright.config.ts. browser.newContext() does not
// inherit config `use` options, so multi-context helpers set it explicitly.
const APP_URL = 'http://localhost:5173';

type Fixtures = {
  apiUp: void;
  authedPage: Page;
};

export const test = base.extend<Fixtures>({
  apiUp: [
    // eslint-disable-next-line no-empty-pattern
    async ({}, use) => {
      const res = await fetch(`${BACKEND_URL}/health`).catch(() => null);
      if (!res || !res.ok) {
        throw new Error(`rust-spades not reachable at ${BACKEND_URL}/health`);
      }
      await use();
    },
    { auto: true },
  ],

  // A Page whose context is already authenticated. registerUser uses
  // page.request, which shares the cookie jar with this page's context.
  authedPage: async ({ page }, use) => {
    await registerUser(page.request);
    await use(page);
  },
});

export { expect };

/**
 * Creates an independent authenticated browser context + page, for multi-player
 * flows that need several simultaneous clients. Caller must close the context.
 */
export async function newPlayerContext(
  browser: Browser,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ baseURL: APP_URL });
  await registerUser(context.request);
  const page = await context.newPage();
  return { context, page };
}
