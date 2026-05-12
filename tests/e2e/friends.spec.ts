import { test } from './setup';

test('create + join via friends challenge', async ({ browser }) => {
  const ctxs = await Promise.all([
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
    browser.newContext(),
  ]);
  const pages = await Promise.all(ctxs.map((c) => c.newPage()));
  const creator = pages[0]!;
  const joiners = [pages[1]!, pages[2]!, pages[3]!];

  try {
    // Creator: navigate to /create and submit without pre-selecting a seat
    // (no creator_seat means all 4 seats are open; creator joins via lobby like everyone else).
    await creator.goto('/');
    await creator.getByRole('button', { name: 'Play with Friends' }).click();
    await creator.waitForURL(/\/create$/);

    // Submit without picking a seat — the challenge opens with 4 empty seats.
    await creator.getByRole('button', { name: 'Create', exact: true }).click();

    // Wait for redirect to the lobby.
    await creator.waitForFunction(() => /\/play\/[^/]+$/.test(location.pathname), {
      timeout: 15_000,
    });
    const shareUrl = creator.url();

    // All 4 players (creator + 3 joiners) each pick the first open seat and join.
    // Process them in sequence so no two try to grab the same seat simultaneously.
    const allPlayers = [creator, ...joiners];
    for (let i = 0; i < allPlayers.length; i++) {
      const player = allPlayers[i]!;
      // Creator is already on the lobby page; joiners navigate to shareUrl.
      if (i > 0) {
        await player.goto(shareUrl);
      }
      // Click the first open seat button.
      await player.locator('button.seat-open').first().click({ timeout: 10_000 });
      // Fill in a name and click Join.
      await player.locator('.join-modal input').fill(`Player${i + 1}`);
      await player.getByRole('button', { name: 'Join', exact: true }).click();
      // Wait for the modal to close (joined event received) before the next player goes.
      await player.waitForFunction(() => document.querySelector('.join-modal') === null, {
        timeout: 10_000,
      });
    }

    // Everyone should navigate to the game and reach BETTING phase.
    await Promise.all(
      pages.map((p) =>
        p.waitForFunction(() => /\/play\/(?!new-ai)[^/]+$/.test(location.pathname), {
          timeout: 15_000,
        }),
      ),
    );

    // BETTING phase: either bet buttons (active turn) or non-empty center text (others waiting).
    await Promise.all(
      pages.map((p) =>
        p.waitForFunction(
          () =>
            document.querySelector('.spades-bets') !== null ||
            (document.querySelector('.spades-center-text')?.textContent?.trim() ?? '') !== '',
          { timeout: 15_000 },
        ),
      ),
    );
  } finally {
    await Promise.all(ctxs.map((c) => c.close()));
  }
});
