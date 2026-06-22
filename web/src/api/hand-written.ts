// Reserved for endpoint types that aren't covered by /openapi.json yet.
// Currently the server's oasgen coverage is partial — only GET routes for
// games / matchmaking / challenges are typed. POST /games, DELETE routes,
// PUT player name, all SSE/WS endpoints, /auth/*, and /users/* are not yet
// in the schema. As each Plan 2/3 task touches an un-typed endpoint, the
// implementer either:
//   (a) hand-writes a `RequestBody`/`Response` type pair here, or
//   (b) uses the low-level `request<T>()` helper from api/client.ts with
//       an inline interface.
//
// This file should shrink to empty once rust-spades' oasgen coverage is
// complete.

import { api, ApiError } from './client';
import type { ReplayResponse } from '../replay/types';

/** Fetch a finished game's replay model. Throws ApiError(403) for in-progress games, ApiError(404) for unknown. */
export async function fetchReplay(id: string): Promise<ReplayResponse> {
  const { data, error, response } = await api.GET('/games/{game_id}/replay.json', {
    params: { path: { game_id: id } },
  });
  if (!data) {
    const msg = typeof error === 'object' && error !== null && 'message' in error
      ? String((error as { message: unknown }).message)
      : 'replay fetch failed';
    throw new ApiError(response?.status ?? 0, msg);
  }
  return data;
}
