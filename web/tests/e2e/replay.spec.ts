/**
 * e2e tests for the /replay/:id viewer.
 *
 * Terminal-game strategy: create a 1-human + 3-bot AI game at low max_points
 * (100) and drive the human player's turns via direct API calls. Bots handle
 * their turns automatically (synchronous AI cascade inside the server). Once
 * the server state reaches Completed, the replay endpoint returns 200.
 *
 * In-progress strategy: create a normal 4-human game (not auto-started).
 * The replay endpoint returns 403; the viewer shows "still in progress".
 */

import { test, expect } from './fixtures';
import type { APIRequestContext } from '@playwright/test';

// ---------------------------------------------------------------------------
// API helpers

type CreateResponse = {
  game_id: string;
  player_ids: string[];
};

type HandResponse = {
  cards: Array<{ suit: string; rank: string }>;
};

/**
 * Create a 1-human + 3-bot AI game. Returns game_id and the human player_id.
 */
async function createAiGame(
  request: APIRequestContext,
  maxPoints: number,
): Promise<{ gameId: string; playerId: string }> {
  const res = await request.post('/games', {
    data: { max_points: maxPoints, num_humans: 1 },
  });
  if (!res.ok()) throw new Error(`create AI game failed: ${res.status()} ${await res.text()}`);
  const { game_id, player_ids } = (await res.json()) as CreateResponse;
  return { gameId: game_id, playerId: player_ids[0]! };
}

/**
 * Drive a 1-human AI game to completion by making the human's moves via the
 * API. Bots handle their own turns via synchronous cascade inside the server,
 * so there is no delay waiting for bot moves — the state is already up-to-date
 * after each human move. Returns once the game reaches Completed or Aborted.
 *
 * To minimise HTTP round-trips the state response's `table_cards` field is
 * used to determine the lead suit; the hand is sorted so matching-suit cards
 * are tried first (follow-suit obligation), avoiding illegal-play retries.
 */
async function driveGameToCompletion(
  request: APIRequestContext,
  gameId: string,
  playerId: string,
  maxMoves = 300,
): Promise<void> {
  for (let move = 0; move < maxMoves; move++) {
    const stateRes = await request.get(`/games/${gameId}`);
    if (!stateRes.ok()) throw new Error(`get state failed: ${stateRes.status()}`);
    const state = (await stateRes.json()) as {
      state: Record<string, unknown> | string;
      current_player_id?: string | null;
      table_cards?: Array<{ suit: string; rank: string } | null> | null;
    };

    const stateKey =
      typeof state.state === 'object' ? Object.keys(state.state)[0] : String(state.state);

    if (stateKey === 'Completed' || stateKey === 'Aborted') return;

    // Only act on our turn.
    if (state.current_player_id !== playerId) continue;

    if (stateKey === 'Bidding') {
      const betRes = await request.post(`/games/${gameId}/transition`, {
        data: { type: 'bet', amount: 3 },
      });
      if (!betRes.ok()) throw new Error(`bet failed: ${betRes.status()} ${await betRes.text()}`);
    } else if (stateKey === 'Trick') {
      // Fetch the hand and sort by play priority to minimise illegal-play retries.
      const handRes = await request.get(`/games/${gameId}/players/${playerId}/hand`);
      if (!handRes.ok()) throw new Error(`get hand failed: ${handRes.status()}`);
      const hand = (await handRes.json()) as HandResponse;

      // Determine lead suit from table_cards (first non-null entry).
      const tableCells = state.table_cards ?? [];
      const leadCard = tableCells.find((c) => c !== null && c !== undefined) ?? null;
      const leadSuit = leadCard?.suit ?? null;

      // Sort: if following suit, put matching-suit cards first.
      // If leading (leadSuit === null), put non-spade cards first (avoid early spade lead).
      const sorted = [...hand.cards].sort((a, b) => {
        if (leadSuit !== null) {
          // Follow-suit: matching lead suit goes first
          const aMatch = a.suit === leadSuit ? 0 : 1;
          const bMatch = b.suit === leadSuit ? 0 : 1;
          return aMatch - bMatch;
        } else {
          // Leading: prefer non-spades to avoid breaking spades prematurely
          const aSpade = a.suit === 'Spade' ? 1 : 0;
          const bSpade = b.suit === 'Spade' ? 1 : 0;
          return aSpade - bSpade;
        }
      });

      let played = false;
      for (const card of sorted) {
        const playRes = await request.post(`/games/${gameId}/transition`, {
          data: { type: 'card', card },
        });
        if (playRes.ok()) {
          played = true;
          break;
        }
      }
      if (!played) throw new Error('no legal card found in hand');
    }
    // For other states (NextHand etc.) just loop again to re-check.
  }
  throw new Error(`game did not reach terminal state within ${maxMoves} moves`);
}

// ---------------------------------------------------------------------------

test('replay page renders four hands and controls for a terminal game', async ({ page }) => {
  test.setTimeout(90_000);
  const { gameId, playerId } = await createAiGame(page.request, 100);
  await driveGameToCompletion(page.request, gameId, playerId);

  await page.goto(`/replay/${gameId}`);

  // Controls toolbar must be visible.
  const toolbar = page.locator('[role="toolbar"][aria-label="Replay controls"]');
  await expect(toolbar).toBeVisible({ timeout: 10_000 });

  // All four seat containers should have rendered face-up hand cards.
  const handCards = page.locator('.replay-hand-card');
  await expect(handCards.first()).toBeVisible({ timeout: 10_000 });
  // 4 × 13 = 52 cards dealt across all four hands.
  await expect(handCards).toHaveCount(52, { timeout: 10_000 });

  // Round indicator is shown in the controls bar.
  const roundIndicator = toolbar.locator('text=/Round \\d+\\/\\d+/');
  await expect(roundIndicator).toBeVisible();
});

test('stepping forward with the next control advances the replay', async ({ page }) => {
  test.setTimeout(90_000);
  const { gameId, playerId } = await createAiGame(page.request, 100);
  await driveGameToCompletion(page.request, gameId, playerId);

  await page.goto(`/replay/${gameId}`);

  const toolbar = page.locator('[role="toolbar"][aria-label="Replay controls"]');
  await expect(toolbar).toBeVisible({ timeout: 10_000 });

  // Wait for hands to load.
  await expect(page.locator('.replay-hand-card').first()).toBeVisible({ timeout: 10_000 });

  // Read the step indicator before clicking Next.
  const progressSpan = toolbar.locator('.replay-progress');
  const progressBefore = await progressSpan.textContent();

  // Click "Next step" (">") — should be enabled at cursor = -1.
  const nextBtn = toolbar.getByRole('button', { name: 'Next step' });
  await expect(nextBtn).toBeEnabled();
  await nextBtn.click();

  // Step indicator must have changed.
  await expect(progressSpan).not.toHaveText(progressBefore ?? '', { timeout: 5_000 });
});

test('navigating to /replay/:id for an in-progress game shows "still in progress"', async ({
  page,
}) => {
  // 4-human game (no bots, not auto-started) stays in NotStarted → 403 on replay.
  const res = await page.request.post('/games', { data: { max_points: 500 } });
  if (!res.ok()) throw new Error(`create game failed: ${res.status()}`);
  const { game_id } = (await res.json()) as { game_id: string };

  await page.goto(`/replay/${game_id}`);

  await expect(page.getByText('This game is still in progress.')).toBeVisible({
    timeout: 10_000,
  });
});
