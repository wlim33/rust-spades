extern crate spades;

extern crate rand;
extern crate uuid;

use std::{io};
use spades::{Game, GameTransition, State, Card, Suit};
use rand::{thread_rng};

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
        //println!("{:#?},", g.scoring.team_a.bets);
        let mut stdin = io::stdin();
        let input = &mut String::new();

        let mut rng = thread_rng();

        //println!("{}, {}", g.game_state.round, g.game_state.trick);

        if let State::Trick(_playerindex) = *g.get_state() {
            assert!(g.get_current_hand().is_ok());
            let mut hand = g.get_current_hand().ok().unwrap().clone();
            
            // io::stdin()
            //     .read_line(input)
            //     .expect("failed to read from stdin");

            // //random_card = rng.choose(hand.as_slice()).unwrap();
            // let trimmed = input.trim();
            // let card : usize = match trimmed.parse::<usize>() {
            //     Ok(i) => i,
            //     Err(..) => panic!("Bad input."),
            // };

            let x = get_valid_card_index(*g.get_leading_suit().unwrap(), &hand);

            g.play(GameTransition::Card(hand[x].clone()));
           
        } else {
            g.play(GameTransition::Bet(3));
        }

        // println!("g.current_player: {}\n", g.current_player);
        // println!("hand: {:#?}", g.hands_played.last().unwrap());
        //stdin.read_line(input);
 
    }
    assert_eq!(*g.get_state(), State::Completed);
    //println!("{:#?}", g);
}

pub fn get_valid_card_index(leading_suit: Suit, hand: &Vec<Card>) -> usize {
    if hand.iter().any(|ref x| x.suit == leading_suit) && leading_suit != Suit::Blank {
        return hand.iter().position(|ref x| x.suit == leading_suit).unwrap();
    } else {
        return 0;
    }
    
}
