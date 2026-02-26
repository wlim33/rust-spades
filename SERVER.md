# Spades Server

HTTP server for hosting concurrent multiplayer Spades games.

## Building and Running

```bash
cargo build --features server
cargo run --features server -- --port 3000
cargo run --features server -- --port 3000 --db games.sqlite
```

Default port is 3000. Override with `--port` or `PORT` env var.
SQLite persistence is optional. Enable with `--db <path>` or `DATABASE_URL` env var. Without it, games are in-memory only.

## API Reference

### Game Endpoints

#### POST /games

Create a new game with random player IDs.

```json
// Request
{"max_points": 500}

// Response 200
{"game_id": "<uuid>", "player_ids": ["<uuid>", "<uuid>", "<uuid>", "<uuid>"]}
```

#### GET /games

List all active game IDs.

```json
// Response 200
["<uuid>", "<uuid>"]
```

#### GET /games/:game_id

Get current game state.

```json
// Response 200
{
  "game_id": "<uuid>",
  "state": "Betting(0)",
  "team_a_score": 0,
  "team_b_score": 0,
  "team_a_bags": 0,
  "team_b_bags": 0,
  "current_player_id": "<uuid>",
  "player_names": [
    {"player_id": "<uuid>", "name": "Alice"},
    {"player_id": "<uuid>", "name": null},
    {"player_id": "<uuid>", "name": null},
    {"player_id": "<uuid>", "name": null}
  ],
  "timer_config": null,
  "player_clocks_ms": null,
  "active_player_clock_ms": null
}
```

#### DELETE /games/:game_id

Remove a game. Returns 204 on success, 404 if not found.

#### POST /games/:game_id/transition

Make a move.

```json
// Start
{"type": "start"}

// Bet
{"type": "bet", "amount": 3}

// Play card
{"type": "card", "card": {"suit": "Spade", "rank": "Ace"}}

// Response 200
{"success": true, "result": "Start"}
```

Suit values: `Club`, `Diamond`, `Heart`, `Spade`.
Rank values: `Two` through `Ten`, `Jack`, `Queen`, `King`, `Ace`.

#### GET /games/:game_id/players/:player_id/hand

Get a player's current hand.

```json
// Response 200
{"player_id": "<uuid>", "cards": [{"suit": "Spade", "rank": "Ace"}, ...]}
```

#### PUT /games/:game_id/players/:player_id/name

Set or clear a player's display name. Names must be 1-20 characters and pass profanity filter.

```json
// Request
{"name": "Alice"}

// Clear name
{"name": null}
```

Returns 204 on success.

#### GET /games/:game_id/ws

WebSocket subscription for real-time game state updates.

Query params: `player_id` (optional).

Sends current state on connect, then pushes `GameEvent` on every transition:

```json
{"StateChanged": { ... GameStateResponse ... }}
```

Or on abort:

```json
{"GameAborted": {"game_id": "<uuid>", "reason": "..."}}
```

### Matchmaking: Seek Queue

Auto-matches 4 players with identical `max_points`.

#### POST /matchmaking/seek

SSE endpoint. Adds player to seek queue and streams events until matched.

```json
// Request body
{"max_points": 500, "name": "Alice"}
```

Events:
- `queue_status` — position in queue
- `game_start` — matched, contains `MatchResult` with `game_id`, `player_id`, `player_ids`, `player_names`

Disconnecting removes the player from the queue.

#### GET /matchmaking/seeks

List seek queue summaries grouped by `max_points`.

```json
// Response 200
[{"max_points": 500, "waiting": 2}]
```

### Matchmaking: Lobbies

Manual join. Creator starts a lobby, others join. Game starts when 4 players are present.

#### POST /lobbies

SSE endpoint. Creates a lobby. Creator is the first player.

```json
// Request body
{"max_points": 500, "name": "Alice"}
```

Events:
- `lobby_update` — lobby state (players count, names)
- `game_start` — 4th player joined, contains `MatchResult`

#### GET /lobbies

List open lobbies.

```json
// Response 200
[{"lobby_id": "<uuid>", "max_points": 500, "players": 2, "player_names": ["Alice", null]}]
```

#### POST /lobbies/:lobby_id/join

SSE endpoint. Join an existing lobby.

```json
// Request body
{"name": "Bob"}
```

Same events as lobby creation.

#### DELETE /lobbies/:lobby_id

Delete a lobby. Only the creator can delete.

```json
// Request body
{"creator_id": "<uuid>"}
```

Returns 204 on success.

### Challenges

Seat-specific invitations. Creator picks a seat, shares join links for remaining seats. Game starts when all 4 seats are filled.

#### POST /challenges

SSE endpoint. Creates a challenge.

Request body is a `ChallengeConfig`:

```json
{
  "max_points": 500,
  "timer_config": {"initial_time_secs": 300, "increment_secs": 5},
  "creator_seat": "A",
  "creator_name": "Alice",
  "expiry_secs": 86400
}
```

All fields except `max_points` are optional. `creator_seat` defaults to no seat (observer-created challenge). `expiry_secs` defaults to 86400 (1 day).

Events:
- `challenge_created` — includes `challenge_id`, `seats`, `join_urls`, `expires_at_epoch_secs`
- `seat_update` — someone joined or left
- `game_start` — all seats filled, contains `MatchResult`
- `cancelled` — expired or creator cancelled

#### GET /challenges

List open challenges.

```json
// Response 200
[{"challenge_id": "<uuid>", "max_points": 500, "seats_filled": 2, "seats": [...]}]
```

#### GET /challenges/:challenge_id

Get challenge status.

```json
// Response 200
{
  "challenge_id": "<uuid>",
  "max_points": 500,
  "timer_config": null,
  "seats": [{"seat": "A", "player_id": "<uuid>", "name": "Alice"}, null, null, null],
  "status": "Open",
  "expires_at_epoch_secs": 1234567890
}
```

Status values: `Open`, `Started` (with `game_id`), `Cancelled`, `Expired`.

#### POST /challenges/:challenge_id/join/:seat

SSE endpoint. Join a specific seat (A, B, C, or D).

```json
// Request body (optional)
{"name": "Bob"}
```

Events: `seat_update`, `game_start`, `cancelled`.

Disconnecting vacates the seat.

#### DELETE /challenges/:challenge_id

Cancel a challenge. Only the creator can cancel.

```json
// Request body
{"creator_id": "<uuid>"}
```

Returns 204 on success, 403 if not creator, 404 if not found.

## Timers

Games can be created with Fischer increment timers via `timer_config` on challenge creation.

- `initial_time_secs` — starting clock per player
- `increment_secs` — seconds added after each move

Behavior on timeout:
- First round of betting: game aborts
- Later betting rounds: auto-bet 1
- Trick play: auto-play a random legal card

Clock state is included in `GameStateResponse` as `player_clocks_ms` and `active_player_clock_ms`.

## SQLite Persistence

When started with `--db`, the server:
- Creates a `games` table if it does not exist
- Loads all persisted games on startup
- Saves game state on every transition
- Removes game data on deletion

Schema:
```sql
CREATE TABLE games (
    id TEXT PRIMARY KEY,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
)
```

## Using GameManager in Rust Code

```rust
use spades::game_manager::GameManager;
use spades::GameTransition;

let manager = GameManager::new(); // in-memory
// or: let manager = GameManager::with_db("games.sqlite").unwrap();

let response = manager.create_game(500, None).unwrap();
let game_id = response.game_id;

manager.make_transition(game_id, GameTransition::Start).unwrap();

let state = manager.get_game_state(game_id).unwrap();
let hand = manager.get_hand(game_id, response.player_ids[0]).unwrap();
```

## Architecture

- **GameManager** — thread-safe concurrent game storage with broadcast channels for state events
- **Matchmaker** — seek queue (auto-match) and lobby (manual join) systems
- **ChallengeManager** — seat-based invitations with expiry
- **SqliteStore** — optional persistence layer
- **REST/SSE/WebSocket** — Axum-based HTTP server with CORS support
- **Drop Guards** — SSE handlers clean up seeks, lobbies, and challenge seats on client disconnect

## Dependencies (server feature only)

- `tokio` — async runtime
- `axum` — web framework (with WebSocket support)
- `tower` / `tower-http` — middleware, CORS
- `tokio-stream` / `futures-util` / `async-stream` — SSE streaming
- `rusqlite` — SQLite (bundled)
- `rustrict` — profanity filter for player names
- `serde` / `serde_json` — serialization (always included, not feature-gated)
