extern crate spades;

extern crate rand;

extern crate uuid;

use std::{io};
use spades::{Game, GameTransition, State};
use rand::{thread_rng, Rng};

#[test]
#[allow(unused)]
fn main() {
    let mut g = Game::new(uuid::Uuid::new_v4(), 
        [uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4(), 
         uuid::Uuid::new_v4()], 
         500);

    g.play(GameTransition::Start);
    println!("{:#?}", g);
    while *g.get_state() != State::Completed {
        let mut stdin = io::stdin();
        let input = &mut String::new();

        let mut rng = thread_rng();

        //println!("{}, {}", g.game_state.round, g.game_state.trick);

        if let State::Trick(_playerindex) = *g.get_state() {
            assert!(g.get_current_hand().is_ok());
            let hand = g.get_current_hand().ok().unwrap().clone();

            let random_card = rng.choose(hand.as_slice()).unwrap();

            g.play(GameTransition::Card(random_card.clone()));
        } else {
            g.play(GameTransition::Bet(3));
        }

        // println!("g.current_player: {}\n", g.current_player);
        // println!("hand: {:#?}", g.hands_played.last().unwrap());
        //stdin.read_line(input);
 
    }
    assert_eq!(*g.get_state(), State::Completed);
    println!("{:?}", g);
} 