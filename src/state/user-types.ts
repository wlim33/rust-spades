// User types for /auth/* and /users/* endpoints.
// Hand-written until the server's oasgen coverage includes these routes.

export type User = {
  id: string;
  username: string;
  email: string;
  display_name?: string | null;
  email_verified: boolean;
  created_at: string;
};

export type ProfileResponse = {
  username: string;
  display_name?: string | null;
  created_at: string;
  games_played: number;
  games_won: number;
};

export type GameHistoryItem = {
  game_id: string;
  started_at: string;
  ended_at: string | null;
  team: 'A' | 'B';
  won: boolean;
  score: number;
};
