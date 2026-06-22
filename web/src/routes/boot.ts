import { ApiError, request } from '../api/client';
import {
  createGameStore,
  type GameStore,
  type GameStateResponse,
  type HandResponse,
  type PresencePlayer,
} from '../state/game';
import { saveSession, loadSession, clearSession } from '../lib/storage';

export type ChallengeSeat = {
  seat: 'A' | 'B' | 'C' | 'D';
  player_id: string;
  name: string | null;
} | null;

export type ChallengeStatus = {
  challenge_id: string;
  max_points: number;
  seats: ChallengeSeat[];
  status: 'open' | 'started' | 'cancelled' | 'expired';
  expires_at_epoch_secs: number;
};

export type BootResult =
  | { kind: 'game'; store: GameStore; gameId: string; playerId: string }
  | { kind: 'lobby'; challengeId: string; shortId: string; status: ChallengeStatus }
  | { kind: 'error'; message: string };

export async function startAIGame(): Promise<{
  gameId: string;
  playerId: string;
  shortId: string;
}> {
  const created = await request<{ game_id: string; player_ids: string[] }>('/games', {
    method: 'POST',
    body: JSON.stringify({ max_points: 500, num_humans: 1 }),
  });
  const state = await request<{ short_id?: string | null }>(`/games/${created.game_id}`, {
    method: 'GET',
  });
  const shortId = state.short_id ?? created.game_id;
  return { gameId: created.game_id, playerId: created.player_ids[0]!, shortId };
}

export async function bootFromUrl(shortId: string): Promise<BootResult> {
  // 1. localStorage
  const saved = loadSession(shortId);
  if (saved) {
    try {
      const state = await request<GameStateResponse>(`/games/${saved.gid}`, { method: 'GET' });
      const hand = await request<HandResponse>(`/games/${saved.gid}/players/${saved.pid}/hand`, {
        method: 'GET',
      });
      // Identify self by the canonical UUID the server resolved (hand.player_id),
      // not saved.pid — a session saved before this fix holds a short id, which
      // never equals the UUID current_player_id, so the player's turn (and thus
      // their input) would never activate. saved.pid still works as the hand-fetch
      // URL token because that endpoint accepts either id form.
      const store = createGameStore(hand.player_id);
      store.applyState(state, hand);
      try {
        const presence = await request<{ players: PresencePlayer[] }>(
          `/games/${saved.gid}/presence`,
          { method: 'GET' },
        );
        store.applyPresence(presence.players);
      } catch {
        // optional
      }
      return { kind: 'game', store, gameId: saved.gid, playerId: hand.player_id };
    } catch {
      clearSession(shortId);
    }
  }

  // 2. by-player-url
  try {
    const resp = await request<{
      game_id: string;
      player_short_id?: string;
      player_id: string;
      game: GameStateResponse;
      hand: HandResponse;
    }>(`/games/by-player-url/${shortId}`, { method: 'GET' });
    // Self must be the canonical UUID: every turn comparison (isMyTurn, active
    // seat) tests it against current_player_id / player_names[].player_id, which
    // the server emits as UUIDs. player_short_id is a URL alias in a different
    // namespace — using it here means self never matches the active player and
    // the player's input is never enabled. Endpoints (ws/hand/play) accept the
    // UUID directly, so the short id is not needed past the by-player-url lookup.
    const playerId = resp.player_id;
    const store = createGameStore(playerId);
    store.applyState(resp.game, resp.hand);
    saveSession(shortId, resp.game_id, playerId);
    return { kind: 'game', store, gameId: resp.game_id, playerId };
  } catch {
    // fall through
  }

  // 3. by-short-id (challenge)
  try {
    const status = await request<ChallengeStatus>(`/challenges/by-short-id/${shortId}`, {
      method: 'GET',
    });
    if (status.status === 'open') {
      return { kind: 'lobby', challengeId: status.challenge_id, shortId, status };
    }
    if (status.status === 'started')
      return { kind: 'error', message: 'This game has already started.' };
    return { kind: 'error', message: 'This challenge is no longer available.' };
  } catch (e) {
    // Distinguish a real 404 from a server/network failure: a transient outage
    // on a valid link shouldn't read as "this game doesn't exist".
    const notFound = e instanceof ApiError && e.status >= 400 && e.status < 500;
    return {
      kind: 'error',
      message: notFound
        ? 'Game or challenge not found.'
        : 'Couldn’t reach the server. Please try again.',
    };
  }
}
