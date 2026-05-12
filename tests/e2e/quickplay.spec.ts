import { test } from './setup';

test('four players matched via quickplay', async ({ browser }) => {
  const contexts = await Promise.all([
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
  ]);
  const pages = await Promise.all(contexts.map((c) => c.newPage()));

  try {
    await Promise.all(pages.map((p) => p.goto('/')));
    await Promise.all(
      pages.map((p) => p.getByRole('button', { name: '5+3', exact: true }).click()),
    );

    // All four should reach a /play/:shortId URL within a few seconds.
    await Promise.all(
      pages.map((p) =>
        p.waitForFunction(() => /\/play\/(?!new-ai)[^/]+$/.test(location.pathname), {
          timeout: 15_000,
        }),
      ),
    );

    // And reach BETTING phase — either bet buttons (active turn) or non-empty center text (others waiting).
    await Promise.all(
      pages.map((p) =>
        p.waitForFunction(
          () =>
            document.querySelector('.spades-bets') !== null ||
            (document.querySelector('.spades-center-text')?.textContent?.trim() ?? '') !== '',
          { timeout: 10_000 },
        ),
      ),
    );
  } finally {
    await Promise.all(contexts.map((c) => c.close()));
  }
});
