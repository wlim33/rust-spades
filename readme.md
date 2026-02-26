
# Spades

[![MIT Licence](https://img.shields.io/github/license/wlim33/rust-spades.svg)](https://github.com/wlim33/rust-spades/blob/master/LICENSE.txt)

Rust implementation of the four-player [trick-taking](https://en.wikipedia.org/wiki/Trick-taking_game) card game Spades. Rules: [pagat.com/auctionwhist/spades.html](https://www.pagat.com/auctionwhist/spades.html).

## Installation

```toml
[dependencies]
spades = "1.1"
```

## Library Usage

```rust
use spades::{Game, GameTransition, State};
use rand::seq::SliceRandom;
use rand::thread_rng;

let mut g = Game::new(
    uuid::Uuid::new_v4(),
    [uuid::Uuid::new_v4(),
     uuid::Uuid::new_v4(),
     uuid::Uuid::new_v4(),
     uuid::Uuid::new_v4()],
    500,
    None, // optional TimerConfig
);

g.play(GameTransition::Start);

while *g.get_state() != State::Completed {
    let mut rng = thread_rng();
    if let State::Trick(_) = *g.get_state() {
        let hand = g.get_current_hand().unwrap().clone();
        let card = hand.as_slice().choose(&mut rng).unwrap();
        g.play(GameTransition::Card(card.clone()));
    } else {
        g.play(GameTransition::Bet(3));
    }
}
```

## Server Mode

Optional HTTP server for hosting concurrent multiplayer games. Includes matchmaking, challenge links, WebSocket game subscriptions, SSE event streams, optional SQLite persistence, and Fischer increment timers.

```bash
cargo run --features server -- --port 3000
cargo run --features server -- --port 3000 --db games.sqlite
```

See [SERVER.md](SERVER.md) for the full API reference.

## Bidding

Nil bids are supported (bet zero for +/-100 point bonus/penalty). Blind bids are not yet supported.

## Documentation

[docs.rs/spades](https://docs.rs/spades/)

## Contributing

Issues and pull requests welcome.
