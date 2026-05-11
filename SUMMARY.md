# Implementation Summary

## Current State (v2.0.0)

Cargo workspace with a core library crate (`spades`) and an HTTP server crate (`spades-server`).

### Core Library (`crates/spades-core`, published as `spades`)
- Full game logic: dealing, betting (including nil), trick-taking, scoring with bags
- State machine: `NotStarted → Betting(0-3) → Trick(0-3) → Completed/Aborted`
- Teams: players A+C vs B+D (`Game.players: [Player; 4]`)
- Trick slots are `Option<Card>`; `Suit::Blank`/`Rank::Blank` sentinels removed
- "Spades not broken" lead rule enforced
- Fischer increment timer config type
- JSON serialization; custom `Deserialize` accepts pre-2.0 rows for SQLite compat
- Optional `openapi` feature derives `oasgen::OaSchema` on public types
- AI: `ai::AiStrategy` trait + `RandomStrategy`

### Server (`crates/spades-server`, binary `spades-server`)
- Axum REST + WebSocket + SSE
- Matchmaking: seek queue (auto-match by max_points) + manual lobbies
- Challenge links: seat-specific join URLs with configurable expiry
- Optional SQLite persistence (`--db <path>` or `DATABASE_URL`)
- CORS off by default (`--cors-allow-origin` / `CORS_ALLOW_ORIGIN`)
- Player name validation with profanity filter (rustrict)
- Drop guards: SSE handlers clean up seeks/lobbies/seats on disconnect
- OpenAPI/Swagger UI via `oasgen`
- HTTP error bodies use `Display`, not `Debug`

### Modules
| Crate | File | Description |
|-------|------|-------------|
| spades-core | `lib.rs` | `Game`, `Player`, transitions, `TimerConfig` |
| spades-core | `cards.rs` | `Card`, `Suit`, `Rank`, deck, trick resolution |
| spades-core | `game_state.rs` | `State` enum |
| spades-core | `scoring.rs` | Round scoring, bags, nil bonus |
| spades-core | `result.rs` | `TransitionSuccess`, `TransitionError`, `GetError` |
| spades-core | `ai.rs` | `AiStrategy` trait, `RandomStrategy` |
| spades-server | `game_manager.rs` | Thread-safe game storage, broadcast channels, SQLite glue |
| spades-server | `matchmaking.rs` | Seek queue + lobby manager |
| spades-server | `challenges.rs` | Seat-based invitations with expiry |
| spades-server | `validation.rs` | Player name rules |
| spades-server | `sqlite_store.rs` | Persistence layer |
| spades-server | `oasgen_impls.rs` | OpenAPI schema impls for non-core types |
| spades-server | `bin/server/main.rs` | Route wiring, CLI/env config |
| spades-server | `bin/server/dto.rs` | Request/response DTOs |
| spades-server | `bin/server/presence.rs` | `PresenceTracker` |
| spades-server | `bin/server/ws.rs` | WebSocket handler |
| spades-server | `bin/server/handlers/{games,matchmaking,challenges,players}.rs` | Per-resource HTTP handlers |

### Tests

238 total (core: 104 unit + 83 integration + 1 doc; server: 49 unit + 1 integration).

```
cargo test --workspace
```

### Build / Run

```
cargo build --workspace
cargo run -p spades-server -- --port 3000
cargo run -p spades-server -- --port 3000 --db games.sqlite
```

### Deployment

- `deploy/setup.sh` — one-time provisioning script for a Linux box (creates `deploy` user, installs rustup, sets up systemd)
- `deploy/spades-server.service` — systemd unit; runs release binary as `deploy` user with sandboxing
- `hooks/pre-push` — opt-in pre-push gate (`cargo clippy -D warnings` + `cargo test`); enable with `git config core.hooksPath hooks`
- Local SSH deploy script lives under `/bin/deploy` and `/deploy/` — gitignored, never checked in
