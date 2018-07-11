
# Spades · [![MIT Licence](	https://img.shields.io/github/license/wlim33/rust-spades.svg)](https://github.com/wlim33/rust-spades/blob/master/LICENSE.txt) · [![Build Status](https://travis-ci.org/wlim33/rust-spades.svg?branch=master)](https://travis-ci.org/wlim33/rust-spades)


Spades is a four person [trick-taking](https://en.wikipedia.org/wiki/Trick-taking_game) card game. For the complete rules of spades, click [here](https://www.pagat.com/auctionwhist/spades.html). 

## Getting Started
Add this line to your `Cargo.toml`:
```
[dependencies]
spades = "1.0.0"
```

## How to use
```rust
extern crate spades;
extern crate uuid;

use spades::{Game, GameTransition};
use spades::result::{TransitionSuccess, TransitionError, GetError};

let mut g = Game::new(uuid::Uuid::new_v4(), 
        [uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4()], 
         500);

g.play(GameTransition::Start);

//Each round starts with a round of betting

assert_eq!(g.play(GameTransition::Bet(3)), TransitionSuccess::Bet);
g.play(GameTransition::Bet(4));
g.play(GameTransition::Bet(4));
g.play(GameTransition::Bet(2));

assert_eq!(g.play(GameTransition::Bet(3)), TransitionFailure::Bet);


let hand = g.get_hand(g.current_player).clone();

let valid_card = g.last().unwrap().clone();
g.play(GameTransition::Card(valid_card));

//...

```

## Documentation
For a complete description of the crate, check the docs.rs [page](https://docs.rs/spades/).

## Contributing
If there is a feature you would like to see added, please feel free to make an open an issue or pull request.