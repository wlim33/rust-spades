
# Spades

[![MIT Licence](https://img.shields.io/github/license/wlim33/rust-spades.svg)](https://github.com/wlim33/rust-spades/blob/master/LICENSE.txt)

Rust implementation of the four-player [trick-taking](https://en.wikipedia.org/wiki/Trick-taking_game) card game Spades. Rules: [pagat.com/auctionwhist/spades.html](https://www.pagat.com/auctionwhist/spades.html).

## Installation

```toml
[dependencies]
spades = "2.0"
```

## Library Usage

```rust
use spades::{Game, GameTransition, State};
use rand::seq::SliceRandom;
use rand::thread_rng;

let mut g = Game::new(
    uuid::Uuid::new_v4(),
    [uuid::Uuid::new_v4(); 4],
    500,
    None, // optional TimerConfig
);
g.play(GameTransition::Start).unwrap();

let mut rng = thread_rng();
while *g.get_state() != State::Completed {
    if let State::Trick(_) = *g.get_state() {
        let legal = g.get_legal_cards().unwrap();
        let card = *legal.choose(&mut rng).unwrap();
        g.play(GameTransition::Card(card)).unwrap();
    } else {
        g.play(GameTransition::Bet(3)).unwrap();
    }
}
```

## Server Mode

Optional HTTP server for hosting concurrent multiplayer games. Includes matchmaking, challenge links, WebSocket game subscriptions, SSE event streams, optional SQLite persistence, and Fischer increment timers. Ships as a separate crate (`spades-server`) so library consumers don't pull in axum/tokio/sqlite.

```bash
cargo run -p spades-server -- --port 3000
cargo run -p spades-server -- --port 3000 --db games.sqlite
```

See [SERVER.md](SERVER.md) for the full API reference.

## Local development

Run the full stack (Rust server + web UI) with one command. One-time setup:

```bash
pnpm -C web install
pnpm -C web exec playwright install chromium   # for e2e tests
```

Then, from the repo root:

```bash
make dev     # backend on :3000 + Vite UI on :5173 (Ctrl-C stops both)
make test    # cargo + web unit/component tests
make e2e     # web end-to-end tests (auto-starts the backend)
make         # list all targets
```

The dev server writes to a local `dev.sqlite` (git-ignored). `make clean` removes it.

## Bidding

Nil bids are supported (bet zero for +/-100 point bonus/penalty). Blind bids are not yet supported.

## Documentation

[docs.rs/spades](https://docs.rs/spades/)

## Contributing

Issues and pull requests welcome. The repo ships an opt-in pre-push hook that runs clippy, the full test suite, and a per-crate coverage regression check via `cargo llvm-cov`. See [docs/coverage.md](docs/coverage.md) for how to enable it and how the coverage baseline ratchets.
