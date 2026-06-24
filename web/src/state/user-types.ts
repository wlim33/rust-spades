// User types for /auth/* and /users/* endpoints.
// Hand-written until the server's oasgen coverage includes these routes.

export type User = {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
};

export type PublicProfile = {
  username: string;
  created_at: string;
  games_played: number;
  last_seen_at: string | null;
  rating: number;
  rd: number;
};

export type SeatPlayer = {
  seat_index: number;
  name: string;
  is_bot: boolean;
};

// Match outcome from the profile owner's perspective. `in_progress` = a live
// game; `unknown` = finished before result tracking / pruned (no state to show).
export type ProfileGameState = 'won' | 'lost' | 'tied' | 'aborted' | 'in_progress' | 'unknown';

export type ProfileGameEntry = {
  game_id: string;
  // The profile owner's own seat — emphasized in the game row.
  seat_index: number;
  player_id: string;
  // All four seats, ordered by seat index. Seats 0 & 2 are one partnership;
  // seats 1 & 3 are the other.
  players: SeatPlayer[];
  // Outcome for the profile owner, plus their team's score vs the opponents'
  // (null until the game finishes).
  state: ProfileGameState;
  team_score: number | null;
  opp_score: number | null;
};

export type ProfileGames = {
  username: string;
  limit: number;
  offset: number;
  total: number;
  games: ProfileGameEntry[];
};

export type LeaderboardPeriod = 'all-time' | 'this-month';

export type LeaderboardEntry = {
  rank: number;
  username: string;
  rating: number;
  rd: number;
  games_played: number;
  score: number;
};

export type Leaderboard = {
  period: string;
  entries: LeaderboardEntry[];
};
