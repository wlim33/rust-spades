import { test as base } from '@playwright/test';

export const test = base.extend<{ apiUp: void }>({
  apiUp: [
    // eslint-disable-next-line no-empty-pattern
    async ({}, use) => {
      const url = process.env.VITE_API_URL ?? 'http://localhost:3000';
      const res = await fetch(`${url}/games`).catch(() => null);
      if (!res || !res.ok) {
        throw new Error(`rust-spades not reachable at ${url}/games`);
      }
      await use();
    },
    { auto: true },
  ],
});
export { expect } from '@playwright/test';
