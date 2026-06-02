use crate::{Game, GameTransition, State};
use uuid::Uuid;

fn four_players() -> [Uuid; 4] {
    [
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    ]
}

#[test]
fn test_game_json_roundtrip_not_started() {
    let game = Game::new(
        Uuid::new_v4(),
        [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ],
        500,
        None,
    );
    let json = serde_json::to_string(&game).unwrap();
    let deserialized: Game = serde_json::from_str(&json).unwrap();
    assert_eq!(*deserialized.get_id(), *game.get_id());
    assert_eq!(*deserialized.get_state(), State::NotStarted);
}

#[test]
fn test_game_json_roundtrip_after_start() {
    let mut game = Game::new(
        Uuid::new_v4(),
        [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ],
        500,
        None,
    );
    game.play(GameTransition::Start).unwrap();

    let json = serde_json::to_string(&game).unwrap();
    let deserialized: Game = serde_json::from_str(&json).unwrap();
    assert_eq!(*deserialized.get_id(), *game.get_id());
    assert_eq!(*deserialized.get_state(), State::Betting(0));
    assert_eq!(
        deserialized.get_team_a_score().unwrap(),
        game.get_team_a_score().unwrap()
    );
}

#[test]
fn test_game_json_roundtrip_after_bets() {
    let player_ids = [
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    ];
    let mut game = Game::new(Uuid::new_v4(), player_ids, 500, None);
    game.play(GameTransition::Start).unwrap();
    game.play(GameTransition::Bet(3)).unwrap();
    game.play(GameTransition::Bet(4)).unwrap();
    game.play(GameTransition::Bet(3)).unwrap();
    game.play(GameTransition::Bet(3)).unwrap();

    assert_eq!(*game.get_state(), State::Trick(0));

    let json = serde_json::to_string(&game).unwrap();
    let deserialized: Game = serde_json::from_str(&json).unwrap();
    assert_eq!(*deserialized.get_state(), State::Trick(0));
    assert_eq!(*deserialized.get_id(), *game.get_id());

    // Verify player hands survive serialization
    for &pid in &player_ids {
        let original_hand = game.get_hand_by_player_id(pid).unwrap();
        let restored_hand = deserialized.get_hand_by_player_id(pid).unwrap();
        assert_eq!(original_hand, restored_hand);
    }
}

#[test]
fn test_game_json_roundtrip_mid_trick() {
    // The server persists games to SQLite mid-trick, so the on-table cards,
    // leading suit, and spades_broken flag must all survive a roundtrip.
    let mut game = Game::new(Uuid::new_v4(), four_players(), 500, None);
    game.play(GameTransition::Start).unwrap();
    for _ in 0..4 {
        game.play(GameTransition::Bet(3)).unwrap();
    }
    // Complete one full trick (breaks spades if a spade is discarded), then play
    // two cards into the next trick so the table is partially filled.
    for _ in 0..4 + 2 {
        let legal = game.get_legal_cards().unwrap();
        game.play(GameTransition::Card(legal[0])).unwrap();
    }
    assert!(matches!(*game.get_state(), State::Trick(2)));
    assert!(game.get_leading_suit().unwrap().is_some());

    let json = serde_json::to_string(&game).unwrap();
    let restored: Game = serde_json::from_str(&json).unwrap();

    // Whole-game structural equality via canonical JSON (covers private fields
    // like spades_broken and the deck without exposing them).
    assert_eq!(
        serde_json::to_value(&game).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    // Spot-check the trick-in-progress fields explicitly.
    assert_eq!(*restored.get_state(), *game.get_state());
    assert_eq!(
        restored.get_leading_suit().unwrap(),
        game.get_leading_suit().unwrap()
    );
    assert_eq!(
        restored.get_current_trick_cards().unwrap(),
        game.get_current_trick_cards().unwrap()
    );
    assert_eq!(
        restored.get_current_player_index_num(),
        game.get_current_player_index_num()
    );
}

#[test]
fn test_game_json_roundtrip_after_completed_trick() {
    // `last_completed_trick` / `last_trick_winner` are set when a trick completes
    // and cleared on the next play() call, so the realistic persistence point is
    // immediately after the trick. They must survive a roundtrip there.
    let mut game = Game::new(Uuid::new_v4(), four_players(), 500, None);
    game.play(GameTransition::Start).unwrap();
    for _ in 0..4 {
        game.play(GameTransition::Bet(3)).unwrap();
    }
    for _ in 0..4 {
        let legal = game.get_legal_cards().unwrap();
        game.play(GameTransition::Card(legal[0])).unwrap();
    }
    assert!(game.get_last_completed_trick().is_some());
    assert!(game.get_last_trick_winner_id().is_some());

    let json = serde_json::to_string(&game).unwrap();
    let restored: Game = serde_json::from_str(&json).unwrap();

    assert_eq!(
        serde_json::to_value(&game).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    assert_eq!(
        restored.get_last_completed_trick(),
        game.get_last_completed_trick()
    );
    assert_eq!(
        restored.get_last_trick_winner_id(),
        game.get_last_trick_winner_id()
    );
}

#[test]
fn test_game_json_roundtrip_completed_game() {
    // A finished game (terminal state + final scores + winners) must roundtrip,
    // since completed games are read back from storage for history/leaderboards.
    let player_ids = four_players();
    let mut game = Game::new(Uuid::new_v4(), player_ids, 100, None);
    game.play(GameTransition::Start).unwrap();
    while *game.get_state() != State::Completed {
        match *game.get_state() {
            State::Betting(_) => {
                game.play(GameTransition::Bet(3)).unwrap();
            }
            State::Trick(_) => {
                let legal = game.get_legal_cards().unwrap();
                game.play(GameTransition::Card(legal[0])).unwrap();
            }
            _ => unreachable!(),
        }
    }

    let json = serde_json::to_string(&game).unwrap();
    let restored: Game = serde_json::from_str(&json).unwrap();

    assert_eq!(
        serde_json::to_value(&game).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    assert_eq!(*restored.get_state(), State::Completed);
    assert_eq!(restored.get_winner_ids(), game.get_winner_ids());
    assert_eq!(restored.get_team_a_score(), game.get_team_a_score());
    assert_eq!(restored.get_team_b_score(), game.get_team_b_score());
}
