extern crate spades;

extern crate rand;

extern crate uuid;

use std::io;
use spades::{Game, GameTransition};
use rand::{thread_rng, Rng};

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
    while !g.scoring.is_over {
        let mut stdin = io::stdin();
        let input = &mut String::new();

        let mut rng = thread_rng();

        //println!("{}, {}", g.game_state.round, g.game_state.trick);

        if !g.scoring.in_betting_stage {
            let hand = g.get_hand(g.current_player).clone();


            let random_card = rng.choose(hand.as_slice()).unwrap();

            g.play(GameTransition::Card(random_card.clone()));

        } else {
            g.play(GameTransition::Bet(3));
        }

        // println!("g.current_player: {}\n", g.current_player);
        // println!("hand: {:#?}", g.hands_played.last().unwrap());
        //stdin.read_line(input);

    }
    //println!("{:?}", g);
    //println!("rounds: {}, \nhands played: {:#?}", g.scoring.round, g.hands_played.len());
}