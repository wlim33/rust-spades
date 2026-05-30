# Improvements

## Breaking changes (2.0.0)

- **`Suit::Blank` and `Rank::Blank` removed.** Trick slots are now `Option<Card>` (`None` for unplayed) and `leading_suit` is now `Option<Suit>` (`None` between tricks/rounds). Public API: `Game::get_current_trick_cards` now returns `Result<&[Option<Card>; 4], GetError>`. `Game::get_leading_suit` now returns `Result<Option<Suit>, GetError>`. `GameStateResponse.table_cards` is `Option<[Option<Card>; 4]>`.
- **`TeamState::current_round_tricks_won`** changed from `[i32; 13]` to `i32`. Public field — direct accessors must be updated. Backward-compat: arrays are still accepted by deserialization (summed) so existing SQLite rows load cleanly.
- **`new_pot()` removed.** No longer needed.
- **Internal field rename:** `Game.player_a/b/c/d` → `Game.players: [Player; 4]`. The `Game::get_hand` deprecated method now returns `Err(GetError::InvalidUuid)` for out-of-range indices (was: silently returned player D's hand). The custom `Deserialize` on `Game` still accepts pre-2.0 SQLite rows with the four-sibling shape.
- **`bin/server.rs` split into `src/bin/server/` directory** with `dto.rs`, `presence.rs`, `ws.rs`, and `handlers/{games,matchmaking,challenges,players}.rs`. Public API of the binary is unchanged.
- **CORS off by default.** Pass `--cors-allow-origin <origin>` (repeatable) or `CORS_ALLOW_ORIGIN=<comma-list>` to enable. Use `--cors-allow-origin '*'` for permissive (dev only).
- **HTTP error response bodies** now use `Display` formatting instead of `Debug`. The error string is the human-readable message from each error type's `Display` impl, not the variant `Debug` form.
- **`GameManagerError`** has new typed variants `Transition(TransitionError)` and `Get(GetError)`. The old string-wrapped `GameError(String)` variant is removed.


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

### Authentication
Email/password registration and login with Argon2id hashing, session-based identity via tower-sessions, email verification, password reset, and OAuth login via Google and GitHub. Anonymous session IDs are preserved across login; game seats created while anonymous are claimed on registration.

### Rate Limiting
Per-IP and per-email rate limiting on auth endpoints (register, login, password reset). Global rate limiting is still TODO.

### Game History
Per-user game listing via `/users/:username/games` (paginated). Full replay viewer is still TODO.

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

All Phase 3/4 cleanup items are now done: `bin/server.rs` split into a module directory, HTTP error bodies use `Display`, `GameManagerError` carries typed `Transition`/`Get` variants, `Suit::Blank`/`Rank::Blank` removed, `current_round_tricks_won` collapsed to a single counter, and the workspace split (`spades-core` + `spades-server`) is in place.

Remaining nits: none outstanding. (`GetError` now implements `std::error::Error`, matching `TransitionError`.)
