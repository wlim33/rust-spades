import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { GameStateResponse, HandResponse } from '../../src/state/game';
import { request } from '../../src/api/client';
import { loadSession, saveSession } from '../../src/lib/storage';

vi.mock('../../src/api/client', () => ({
  request: vi.fn(),
  ApiError: class ApiError extends Error {
    status = 0;
  },
}));
vi.mock('../../src/lib/storage', () => ({
  saveSession: vi.fn(),
  loadSession: vi.fn(),
  clearSession: vi.fn(),
}));

const requestMock = vi.mocked(request);
const loadSessionMock = vi.mocked(loadSession);

// The server keys turn state by full UUID; the short id is only a URL alias.
const SELF_UUID = '11111111-1111-1111-1111-111111111111';
const SELF_SHORT = 'abc123'; // uuid_to_short_id(SELF_UUID) — a different namespace

const gameWhereItIsSelfsTurn = (): GameStateResponse => ({
  game_id: 'g1',
  state: { Trick: 0 },
  team_a_score: 0,
  team_b_score: 0,
  team_a_bags: 0,
  team_b_bags: 0,
  current_player_id: SELF_UUID, // it's THIS player's turn — full UUID
  player_names: [
    { player_id: SELF_UUID, name: 'me' },
    { player_id: '22222222-2222-2222-2222-222222222222', name: 'p2' },
    { player_id: '33333333-3333-3333-3333-333333333333', name: 'p3' },
    { player_id: '44444444-4444-4444-4444-444444444444', name: 'p4' },
  ],
});

const selfHand: HandResponse = { player_id: SELF_UUID, cards: [] };

describe('bootFromUrl identity', () => {
  beforeEach(() => {
    requestMock.mockReset();
    loadSessionMock.mockReset();
    vi.mocked(saveSession).mockReset();
  });

  it('gives self the same id-space as current_player_id (by-player-url path)', async () => {
    const { bootFromUrl } = await import('../../src/routes/boot');
    loadSessionMock.mockReturnValue(null); // skip the localStorage path
    requestMock.mockResolvedValueOnce({
      game_id: 'g1',
      player_id: SELF_UUID,
      player_short_id: SELF_SHORT,
      game: gameWhereItIsSelfsTurn(),
      hand: selfHand,
    });

    const result = await bootFromUrl('shortlink');
    expect(result.kind).toBe('game');
    if (result.kind !== 'game') return;

    // When it is the player's own turn, isMyTurn (currentPlayerId === playerId)
    // must hold — otherwise their input is never enabled and the game hangs.
    expect(result.store.playerId.value).toBe(result.store.currentPlayerId.value);
    expect(result.store.playerIds.value).toContain(result.store.playerId.value);
  });

  it('heals a stale short-id localStorage session to the canonical UUID', async () => {
    const { bootFromUrl } = await import('../../src/routes/boot');
    // A session saved before the fix: pid is the short id, which never matches
    // the UUID current_player_id.
    loadSessionMock.mockReturnValue({ gid: 'g1', pid: SELF_SHORT });
    requestMock
      .mockResolvedValueOnce(gameWhereItIsSelfsTurn()) // GET /games/g1
      .mockResolvedValueOnce(selfHand) // GET /games/g1/players/abc123/hand -> canonical UUID
      .mockResolvedValueOnce({ players: [] }); // GET /games/g1/presence

    const result = await bootFromUrl('shortlink');
    expect(result.kind).toBe('game');
    if (result.kind !== 'game') return;

    expect(result.store.playerId.value).toBe(result.store.currentPlayerId.value);
  });
});
