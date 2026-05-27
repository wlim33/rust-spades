import type { APIRequestContext, Page } from '@playwright/test';

export type AiGame = { gameId: string; playerId: string; shortId: string };

/**
 * Creates a 1-human + 3-bot game (auto-started) via the API. Pass a context-bound
 * request (page.request / context.request): the human seat is owned by the
 * requester's anonymous identity, and the page must share that cookie to make moves.
 */
export async function createAiGame(request: APIRequestContext): Promise<AiGame> {
  const created = await request.post('/games', { data: { max_points: 500, num_humans: 1 } });
  if (!created.ok()) {
    throw new Error(`create AI game failed: ${created.status()} ${await created.text()}`);
  }
  const { game_id, player_ids } = (await created.json()) as {
    game_id: string;
    player_ids: string[];
  };
  const stateRes = await request.get(`/games/${game_id}`);
  if (!stateRes.ok()) {
    throw new Error(`fetch game state failed: ${stateRes.status()} ${await stateRes.text()}`);
  }
  const state = (await stateRes.json()) as { short_id?: string | null };
  const shortId = state.short_id ?? game_id;
  return { gameId: game_id, playerId: player_ids[0]!, shortId };
}

/**
 * Seeds the localStorage session the SPA reads on boot, so navigating directly to
 * /play/{shortId} resolves the player. Must run before page.goto.
 */
export async function seedAiSession(page: Page, game: AiGame): Promise<void> {
  await page.addInitScript(
    ([shortId, gid, pid]) => {
      localStorage.setItem(`spades_game_${shortId}`, JSON.stringify({ gid, pid }));
    },
    [game.shortId, game.gameId, game.playerId] as const,
  );
}
