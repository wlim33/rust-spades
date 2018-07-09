
# Spades [![MIT Licence](https://badges.frapsoft.com/os/mit/mit.svg?v=103)](https://github.com/wlim33/rust-spades/blob/master/LICENSE.txt) [![Build Status](https://travis-ci.org/wlim33/rust-spades.svg?branch=master)](https://travis-ci.org/wlim33/rust-spades)

To learn the rules of spades, click [here](http://www.hoylegaming.com/rules/showrule.aspx?RuleID=227).


## How to use
```rust
extern crate spades;

use spades::{Game, GameTransition};

let mut g = Game::new(uuid::Uuid::new_v4(), 
        [uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4()], 
         500);

g.play(GameTransition::Start);

g.play(GameTransition::Bet(3));
g.play(GameTransition::Bet(3));
g.play(GameTransition::Bet(3));
g.play(GameTransition::Bet(3));

let hand = g.get_hand(g.current_player).clone();

let valid_card = g.last().unwrap().clone();
g.play(GameTransition::Card(valid_card));
```

## Documentation
Further documentation is forthcoming.

