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
Display names with 32-character limit and profanity filter via rustrict.

### AI Players (random strategy)
`spades::ai::AiStrategy` trait + `RandomStrategy`. Server can create 1- or 2-human games with the rest filled by random-policy bots.

### Spades-broken rule
`TransitionError::SpadesNotBroken` rejects leading a spade on the first trick of a round (unless a player's hand is all spades). Once any spade is played, `spades_broken` flips to true; it resets at the start of each new round. `get_legal_cards` reflects the rule.

### Bug fix: bag penalty per 10 bags
The score-update used `if self.bags >= 10` and could miss a second penalty when a round dumped >= 20 bags into a team. Now `while`.

### Bug fix: `get_current_trick_cards` in Betting state
Was returning `GetError::GameCompleted` from the Betting arm. Now returns the dedicated `Unknown` variant for a stage mismatch.

### `Card: Copy`
`Card` is a 2-byte value type; the prior `Clone`-only derive forced unnecessary clones at call sites.

### `Sqids` cached in `OnceLock`
`uuid_to_short_id`, `short_id_to_uuid`, `encode_player_url`, `decode_player_url` no longer rebuild the `Sqids` instance per call.

### Tie comment in `get_winner_ids`
Clarified that the tie-branch is unreachable because scoring keeps `is_over = false` on a max-points tie.

### Single deck shuffle on creation
Removed redundant shuffles in `new_deck()` and `deal_four_players()`; one shuffle in `Game::deal_cards` is enough.

### `players: [Player; 4]`
Internal struct field for the four players is now an array. Eliminates the four-arm dispatching match in `get_current_player_id`, `get_current_hand`, `get_hand_by_player_id`, `set_player_name`, `get_player_names`, `get_last_trick_winner_id`, `deal_cards`, and the card-removal step in `play`. Also fixes the silent index-aliasing bug in the deprecated `get_hand(usize)` — out-of-range now returns `GetError::InvalidUuid` instead of player D's hand.

### Backward-compat deserialization for existing SQLite rows
`Game`'s custom `Deserialize` accepts both the new `{ "players": [...] }` shape and the legacy `{ "player_a": ..., "player_b": ..., ... }` shape. Re-serialization always uses the new shape. Existing databases load cleanly; new writes use the new format.

## Not Yet Implemented

### Authentication
No auth. Any client can access any game or player hand. Production use requires JWT or similar.

### Rate Limiting
No rate limiting on any endpoint.

### Game History / Replay
No transition log. No ability to review or replay past games.

### Spectator Mode
No read-only observer role.

### Tournament Support
No multi-game bracket or aggregate scoring.

### Game Variants
Only standard 4-player partnership Spades. No solo variant, no jokers, no custom house rules.

### Stronger AI
Only `RandomStrategy` ships today. No heuristic, MCTS, or learned policy.

### Blind bids
Standard bids and nil bids are supported. Blind nil (commit to nil before seeing cards) is not.

## Code Quality Notes

- `GetError` does not yet implement `std::error::Error` (Display is implemented). Planned for Phase 3.
- `bin/server.rs` is 2150 lines in one file. Planned for Phase 3 module split.
- `format!("{:?}", e)` is used pervasively for HTTP error bodies. Will move to `Display` in Phase 3.
- `GameManagerError::GameError(String)` flattens typed errors into strings. Planned typed-error pass in Phase 3.
- `Suit::Blank` and `Rank::Blank` are sentinel values mixed into the same enums as real cards. Planned removal in Phase 4 (2.0.0 break) in favor of `Option<Card>` for trick slots.
- `TeamState::current_round_tricks_won: [i32; 13]` is only ever summed. Planned collapse to a single counter in Phase 4.
- Single-crate layout: server feature pulls in ~12 optional deps for library consumers. Planned workspace split (`spades-core`, `spades-server`) in Phase 4.
