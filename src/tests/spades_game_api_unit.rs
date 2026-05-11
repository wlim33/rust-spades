use super::super::cards::{Card, Suit, Rank};
use super::super::result::{TransitionSuccess, TransitionError, GetError};
use super::super::{Game, GameTransition};
use super::super::game_state::State;

fn betting_done(g: &mut Game) {
    for _ in 0..4 {
        g.play(GameTransition::Bet(3)).unwrap();
    }
}

#[test]
fn cannot_lead_spade_before_broken_when_non_spades_available() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
        500,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    g.players[0].hand = vec![
        Card { suit: Suit::Club, rank: Rank::Five },
        Card { suit: Suit::Spade, rank: Rank::Ace },
    ];
    g.players[1].hand = vec![Card { suit: Suit::Club, rank: Rank::Two }];
    g.players[2].hand = vec![Card { suit: Suit::Club, rank: Rank::Three }];
    g.players[3].hand = vec![Card { suit: Suit::Club, rank: Rank::Four }];
    betting_done(&mut g);
    assert!(matches!(g.get_state(), State::Trick(0)));
    assert_eq!(
        g.play(GameTransition::Card(Card { suit: Suit::Spade, rank: Rank::Ace })),
        Err(TransitionError::SpadesNotBroken)
    );
}

#[test]
fn can_lead_spade_when_hand_is_only_spades() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
        500,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    g.players[0].hand = vec![Card { suit: Suit::Spade, rank: Rank::Ace }];
    g.players[1].hand = vec![Card { suit: Suit::Club, rank: Rank::Two }];
    g.players[2].hand = vec![Card { suit: Suit::Club, rank: Rank::Three }];
    g.players[3].hand = vec![Card { suit: Suit::Club, rank: Rank::Four }];
    betting_done(&mut g);
    assert_eq!(
        g.play(GameTransition::Card(Card { suit: Suit::Spade, rank: Rank::Ace })),
        Ok(TransitionSuccess::PlayCard)
    );
}

#[test]
fn lead_spade_allowed_after_spades_broken() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
        500,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    // Player B has no clubs and will trump with a spade, breaking spades.
    g.players[0].hand = vec![
        Card { suit: Suit::Club, rank: Rank::Five },
        Card { suit: Suit::Club, rank: Rank::Six },
    ];
    g.players[1].hand = vec![
        Card { suit: Suit::Spade, rank: Rank::Two },
        Card { suit: Suit::Spade, rank: Rank::Three },
    ];
    g.players[2].hand = vec![
        Card { suit: Suit::Club, rank: Rank::Three },
        Card { suit: Suit::Heart, rank: Rank::Two },
    ];
    g.players[3].hand = vec![
        Card { suit: Suit::Club, rank: Rank::Four },
        Card { suit: Suit::Heart, rank: Rank::Three },
    ];
    betting_done(&mut g);
    g.play(GameTransition::Card(Card { suit: Suit::Club, rank: Rank::Five })).unwrap();
    g.play(GameTransition::Card(Card { suit: Suit::Spade, rank: Rank::Two })).unwrap();
    g.play(GameTransition::Card(Card { suit: Suit::Club, rank: Rank::Three })).unwrap();
    g.play(GameTransition::Card(Card { suit: Suit::Club, rank: Rank::Four })).unwrap();
    // Player B (only spade played) wins and leads the next trick.
    assert!(matches!(g.get_state(), State::Trick(0)));
    assert_eq!(g.get_current_player_id().unwrap(), &g.players[1].id);
    // Spades broken — leading with spade now allowed.
    assert_eq!(
        g.play(GameTransition::Card(Card { suit: Suit::Spade, rank: Rank::Three })),
        Ok(TransitionSuccess::PlayCard)
    );
}

#[test]
fn get_legal_cards_excludes_spades_on_lead_before_broken() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
        500,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    g.players[0].hand = vec![
        Card { suit: Suit::Club, rank: Rank::Five },
        Card { suit: Suit::Spade, rank: Rank::Ace },
    ];
    g.players[1].hand = vec![Card { suit: Suit::Club, rank: Rank::Two }];
    g.players[2].hand = vec![Card { suit: Suit::Club, rank: Rank::Three }];
    g.players[3].hand = vec![Card { suit: Suit::Club, rank: Rank::Four }];
    betting_done(&mut g);
    let legal = g.get_legal_cards().unwrap();
    assert_eq!(legal, vec![Card { suit: Suit::Club, rank: Rank::Five }]);
}

#[test]
fn get_current_trick_cards_in_betting_returns_unknown_not_completed() {
    let mut g = Game::new(
        uuid::Uuid::new_v4(),
        [uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
        500,
        None,
    );
    g.play(GameTransition::Start).unwrap();
    assert!(matches!(g.get_state(), State::Betting(_)));
    assert_eq!(g.get_current_trick_cards().err(), Some(GetError::Unknown));
}

#[allow(unused)]
#[test]
pub fn api_main_unit() {
    let mut g = Game::new(uuid::Uuid::new_v4(),
        [uuid::Uuid::new_v4(),
         uuid::Uuid::new_v4(),
         uuid::Uuid::new_v4(),
         uuid::Uuid::new_v4()],
         500, None);


    assert_eq!(g.play(GameTransition::Card(Card { suit: Suit::Heart, rank: Rank::Five })), Err(TransitionError::NotStarted));
    assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::NotStarted));

    assert_eq!(g.play(GameTransition::Start), Ok(TransitionSuccess::Start));
    assert_eq!(g.play(GameTransition::Start), Err(TransitionError::AlreadyStarted));

    let hand_a = vec![
        Card { suit: Suit::Club, rank: Rank::Five },
        Card { suit: Suit::Club, rank: Rank::Ace },
        Card { suit: Suit::Diamond, rank: Rank::Two },
        Card { suit: Suit::Diamond, rank: Rank::Three },
        Card { suit: Suit::Diamond, rank: Rank::King },
        Card { suit: Suit::Diamond, rank: Rank::Ace },
        Card { suit: Suit::Heart, rank: Rank::Five },
        Card { suit: Suit::Heart, rank: Rank::Nine },
        Card { suit: Suit::Heart, rank: Rank::Jack },
        Card { suit: Suit::Heart, rank: Rank::King },
        Card { suit: Suit::Heart, rank: Rank::Six },
        Card { suit: Suit::Spade, rank: Rank::Six },
        Card { suit: Suit::Spade, rank: Rank::Ace },
    ];
    let hand_b = vec![
        Card {suit: Suit::Club, rank: Rank::Four},
        Card {suit: Suit::Club, rank: Rank::Six},
        Card {suit: Suit::Club, rank: Rank::Nine},
        Card {suit: Suit::Club, rank: Rank::Jack},
        Card {suit: Suit::Diamond, rank: Rank::Seven},
        Card {suit: Suit::Heart, rank: Rank::Four},
        Card {suit: Suit::Heart, rank: Rank::Eight},
        Card {suit: Suit::Heart,rank: Rank::Queen},
        Card {suit: Suit::Spade, rank: Rank::Two},
        Card {suit: Suit::Spade, rank: Rank::Five},
        Card {suit: Suit::Spade, rank: Rank::Eight},
        Card {suit: Suit::Spade, rank: Rank::Ten},
        Card {suit: Suit::Spade, rank: Rank::King},
    ];
    let hand_c = vec![
        Card {suit: Suit::Club, rank: Rank::Two},
        Card {suit: Suit::Club, rank: Rank::Seven},
        Card {suit: Suit::Club, rank: Rank::Ten},
        Card {suit: Suit::Diamond, rank: Rank::Five},
        Card {suit: Suit::Diamond, rank: Rank::Eight},
        Card {suit: Suit::Diamond, rank: Rank::Nine},
        Card {suit: Suit::Diamond, rank: Rank::Jack},
        Card {suit: Suit::Diamond, rank: Rank::Queen},
        Card {suit: Suit::Heart, rank: Rank::Three},
        Card {suit: Suit::Heart, rank: Rank::Seven},
        Card {suit: Suit::Spade, rank: Rank::Nine},
        Card {suit: Suit::Spade, rank: Rank::Jack},
        Card {suit: Suit::Spade, rank: Rank::Queen},
    ];
    let hand_d = vec![
        Card {suit: Suit::Club, rank: Rank::Three },
        Card {suit: Suit::Club, rank: Rank::Eight },
        Card {suit: Suit::Club, rank: Rank::Queen },
        Card {suit: Suit::Club, rank: Rank::King },
        Card {suit: Suit::Diamond, rank: Rank::Four },
        Card {suit: Suit::Diamond, rank: Rank::Six },
        Card {suit: Suit::Diamond, rank: Rank::Ten },
        Card {suit: Suit::Heart, rank: Rank::Two },
        Card {suit: Suit::Heart, rank: Rank::Ten },
        Card {suit: Suit::Heart, rank: Rank::Ace },
        Card {suit: Suit::Spade, rank: Rank::Three },
        Card {suit: Suit::Spade, rank: Rank::Four },
        Card {suit: Suit::Spade, rank: Rank::Seven },
    ];

    g.players[0].hand = hand_a;
    g.players[1].hand = hand_b;
    g.players[2].hand = hand_c;
    g.players[3].hand = hand_d;
    assert_eq!(g.state, State::Betting(0));

    assert_eq!(g.play(GameTransition::Card(Card { suit: Suit::Heart, rank: Rank::Five })), Err(TransitionError::CardInBettingStage));
    assert_eq!(g.play(GameTransition::Start), Err(TransitionError::AlreadyStarted));
    assert_eq!(g.play(GameTransition::Bet(3)), Ok(TransitionSuccess::Bet));

    assert_eq!(g.play(GameTransition::Card(Card { suit: Suit::Heart, rank: Rank::Five })), Err(TransitionError::CardInBettingStage));
    assert_eq!(g.play(GameTransition::Bet(3)), Ok(TransitionSuccess::Bet));

    assert_eq!(g.play(GameTransition::Card(Card { suit: Suit::Heart, rank: Rank::Five })), Err(TransitionError::CardInBettingStage));
    assert_eq!(g.play(GameTransition::Bet(3)), Ok(TransitionSuccess::Bet));

    assert_eq!(g.play(GameTransition::Card(Card { suit: Suit::Heart, rank: Rank::Five })), Err(TransitionError::CardInBettingStage));
    assert_eq!(g.play(GameTransition::Bet(3)), Ok(TransitionSuccess::BetComplete));

    let mut trick_test_closure = |trick_number: usize, played_cards: &[Card; 4], team_a_won : usize| {
        assert_eq!(g.play(GameTransition::Card(played_cards[0])), Ok(TransitionSuccess::PlayCard));
        assert_eq!(g.play(GameTransition::Start), Err(TransitionError::AlreadyStarted));
        assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::BetInTrickStage));

        assert_eq!(g.play(GameTransition::Card(played_cards[1])), Ok(TransitionSuccess::PlayCard));
        assert_eq!(g.play(GameTransition::Start), Err(TransitionError::AlreadyStarted));
        assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::BetInTrickStage));

        assert_eq!(g.play(GameTransition::Card(played_cards[2])), Ok(TransitionSuccess::PlayCard));
        assert_eq!(g.play(GameTransition::Start), Err(TransitionError::AlreadyStarted));
        assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::BetInTrickStage));

        assert_eq!(g.play(GameTransition::Card(played_cards[3])), Ok(TransitionSuccess::Trick));
        assert_eq!(g.play(GameTransition::Start), Err(TransitionError::AlreadyStarted));
        assert_eq!(g.play(GameTransition::Bet(3)), Err(TransitionError::BetInTrickStage));

        assert_eq!(g.scoring.team_b.current_round_tricks_won[trick_number], if team_a_won == 1 { 1 } else {0});
        assert_eq!(g.scoring.team_a.current_round_tricks_won[trick_number], if team_a_won == 0 { 1 } else {0});
    };
    
    let pots = [
        [
            Card { suit: Suit::Club, rank: Rank::Ace },
            Card { suit: Suit::Club, rank: Rank::Six },
            Card { suit: Suit::Club, rank: Rank::Ten },
            Card { suit: Suit::Club, rank: Rank::King }
        ], 
        [
            Card { suit: Suit::Club, rank: Rank::Five },
            Card { suit: Suit::Club, rank: Rank::Four },
            Card { suit: Suit::Club, rank: Rank::Seven },
            Card { suit: Suit::Club, rank: Rank::Queen }
        ],
        [
            Card { suit: Suit::Club, rank: Rank::Eight },
            Card { suit: Suit::Spade, rank: Rank::Six },
            Card {suit: Suit::Club, rank: Rank::Nine },
            Card { suit: Suit::Club, rank: Rank::Two },
        ],

        [
            Card { suit: Suit::Club, rank: Rank::Ace },
            Card { suit: Suit::Club, rank: Rank::Six },
            Card { suit: Suit::Club, rank: Rank::Ten },
            Card { suit: Suit::Club, rank: Rank::King }
        ],
        [
            Card { suit: Suit::Club, rank: Rank::Five },
            Card { suit: Suit::Club, rank: Rank::Four },
            Card { suit: Suit::Club, rank: Rank::Seven },
            Card { suit: Suit::Club, rank: Rank::Queen }
        ],
        [
            Card { suit: Suit::Club, rank: Rank::Eight },
            Card { suit: Suit::Spade, rank: Rank::Six },
            Card {suit: Suit::Club, rank: Rank::Nine },
            Card { suit: Suit::Club, rank: Rank::Two },
        ],
    ];
    let trick_winners = [
        0,
        1,
        0

    ];
    // for t_n in 0..3 {
    //     trick_test_closure(t_n, &pots[t_n], trick_winners[t_n]);
    // }
    
}