import { test, expect } from '../fixtures';
import { GamePage } from '../pages/game-page';
import { createAiGame, seedAiSession } from '../helpers/games';

/**
 * Connection-chaos regression gate. Boots an AI game through the dev-only chaos
 * harness (src/lib/chaos.ts) with injected HTTP latency + jitter, plays several
 * tricks, and asserts the event pipeline isn't gated on redundant /hand fetches.
 *
 * Root cause of the choppiness (proven by this harness): the WS drain
 * (api/ws.ts) awaits a /hand fetch before applying each event, and the client
 * used to re-fetch the hand on EVERY event — including the ~3 opponent plays per
 * trick that can't change the south hand. On a slow link each of those redundant
 * round-trips stalled the next animation, so latency/jitter read as choppy,
 * irregular pacing. Baseline before the fix under this profile: ~4.4 hand
 * fetches/trick, inter-play gap p50≈136ms (≈ the injected latency), p95≈511ms.
 *
 * The fix (game-sync.ts handMayHaveChanged) skips the fetch when the held hand
 * provably can't have changed, so a trick now costs ~1 hand fetch (south's own
 * play) instead of ~4 — and opponent plays apply at server speed, off the
 * latency path. This gate locks that in: hand fetches must stay near one per
 * trick, never the per-play count.
 *
 * Chaos only loads under import.meta.env.DEV; Playwright drives the Vite dev
 * server, so `?chaos` activates it. reducedMotion stays at the project default:
 * we measure Layer-A (event apply), not animation duration.
 */

// Injected network profile: a mediocre-but-not-broken connection.
const LAT = 120;
const JIT = 80;
const TRICKS = 8;

// A healthy hand needs ~1 fetch per trick (south's own play); the redundant-
// fetch bug made it ~4 (one per play). Ceiling sits between the two with margin
// for the occasional extra fetch (a leg where south plays mid-trick can add one)
// — pre-fix this trips at ~35, post-fix it sits near TRICKS.
const MAX_HAND_FETCHES = TRICKS * 2;

type ChaosReport = {
  fetches: number;
  handFetches: number;
  wsFramesIn: number;
  handGapMs: { count: number; mean: number; p50: number; p95: number; max: number };
};

test('chaos: opponent plays do not trigger redundant hand fetches under latency', async ({
  page,
}) => {
  test.setTimeout(90_000);

  const game = await createAiGame(page.request);
  await seedAiSession(page, game);
  await page.goto(`/play/${game.shortId}?chaos=1&lat=${LAT}&jit=${JIT}`);

  const g = new GamePage(page);
  await g.waitForBetting();
  await g.bet(3);
  await expect(g.hand()).toHaveCount(13, { timeout: 20_000 });

  // Discard the boot/betting fetches so the sample is steady-state play only.
  await page.evaluate(() => (window as unknown as { chaos: { reset(): void } }).chaos.reset());

  // Play several tricks; each pulls a burst of opponent plays through the event
  // pipeline — exactly the events that must NOT each cost a hand fetch.
  for (let remaining = 13; remaining > 13 - TRICKS; remaining--) {
    await expect(g.hand()).toHaveCount(remaining, { timeout: 20_000 });
    await g.playFirstLegalCard();
    await expect(g.hand()).toHaveCount(remaining - 1, { timeout: 20_000 });
  }

  const report = await page.evaluate(
    () => (window as unknown as { chaos: { report(): ChaosReport } }).chaos.report() as ChaosReport,
  );
  console.log('[chaos report]', JSON.stringify(report));

  // We played TRICKS cards, so the hand changed (and must be refetched) ~TRICKS
  // times. Anything approaching 4×TRICKS means opponent plays are re-fetching.
  expect(report.handFetches).toBeGreaterThanOrEqual(TRICKS - 1);
  expect(report.handFetches).toBeLessThanOrEqual(MAX_HAND_FETCHES);
});
