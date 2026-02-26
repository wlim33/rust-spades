# Improvements

## Implemented

### Server Mode
REST API server with concurrent game management via GameManager. Axum-based, async, thread-safe.

### WebSocket Support
Real-time game state push via `/games/:game_id/ws`. Sends state on connect, broadcasts on every transition.

### SQLite Persistence
Optional. Games survive server restarts. Enable with `--db <path>`.

### Matchmaking
Seek queue (auto-match 4 players with same max_points) and manual lobbies (create, join, game starts at 4).

### Challenge Links
Seat-specific join URLs. Creator picks a seat, shares links. Configurable expiry. Game starts when all 4 seats filled.

### Fischer Increment Timers
Optional per-game clock configuration. Auto-abort on first-round timeout, auto-bet/auto-play on subsequent timeouts.

### Player Names
Display names with 20-character limit and profanity filter via rustrict.

### Bug Fix: get_hand_by_player_id
Fixed copy-paste bug where all branches checked player_a.id instead of each player's actual ID.

## Not Yet Implemented

### Authentication
No auth. Any client can access any game or player hand. Production use requires JWT or similar.

### Rate Limiting
No rate limiting on any endpoint.

### Game History / Replay
No transition log. No ability to review or replay past games.

### AI Players
No bot support. All 4 seats require human players.

### Spectator Mode
No read-only observer role.

### Tournament Support
No multi-game bracket or aggregate scoring.

### Game Variants
Only standard 4-player partnership Spades. No solo variant, no jokers, no custom house rules.

## Code Quality Notes

- Some long methods (notably `play`) could be broken up
- The deprecated `get_hand` method should be removed in a future major version
- `result.rs` uses deprecated `Error::description` and `Error::cause` — should migrate to `Display` and `Error::source`
