use crate::{Game, GameTransition, State};
use uuid::Uuid;

#[test]
fn test_game_json_roundtrip_not_started() {
    let game = Game::new(
        Uuid::new_v4(),
        [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
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
        [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
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
    let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
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
