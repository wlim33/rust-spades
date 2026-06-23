use rand::seq::IndexedRandom;
use spades::{Game, GameTransition, State};

#[test]
fn drive_a_game_to_completion_with_random_legal_play() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        ],
        100,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    let mut rng = rand::rng();

    while *g.get_state() != State::Completed {
        match *g.get_state() {
            State::Trick(_) => {
                let legal = g.get_legal_cards().expect("legal cards in Trick state");
                let card = *legal.choose(&mut rng).expect("at least one legal card");
                g.play(GameTransition::Card(card))
                    .expect("legal card should play");
            }
            State::Bidding(_) => {
                g.play(GameTransition::Bet(3))
                    .expect("Bet always valid in Betting");
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(*g.get_state(), State::Completed);
}
