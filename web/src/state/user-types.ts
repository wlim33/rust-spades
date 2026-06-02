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

export type ProfileGameEntry = {
  game_id: string;
  seat_index: number;
  player_id: string;
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
