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

/// Legacy SQLite rows used per-player fields (player_a, player_b, ...).
/// Our deserializer must still accept that shape.
#[test]
fn test_game_json_deserialize_legacy_player_fields() {
    let id = Uuid::new_v4();
    let pid_a = Uuid::new_v4();
    let pid_b = Uuid::new_v4();
    let pid_c = Uuid::new_v4();
    let pid_d = Uuid::new_v4();
    let blank = serde_json::json!({"suit": "Blank", "rank": "Blank"});
    let legacy = serde_json::json!({
        "id": id,
        "state": "NotStarted",
        "scoring": {
            "config": {"max_points": 500},
            "team_a": {"current_round_tricks_won": [0,0,0,0,0,0,0,0,0,0,0,0,0], "bags": 0, "cumulative_points": 0},
            "team_b": {"current_round_tricks_won": [0,0,0,0,0,0,0,0,0,0,0,0,0], "bags": 0, "cumulative_points": 0},
            "in_betting_stage": true,
            "bets_placed": [[0,0,0,0]],
            "is_over": false,
            "round": 0,
            "trick": 0,
            "nil_check": [false, false, false, false],
            "player_tricks_won": [0,0,0,0]
        },
        "current_player_index": 0,
        "deck": [],
        "hands_played": [[blank, blank, blank, blank]],
        "leading_suit": "Blank",
        "player_a": {"id": pid_a, "hand": []},
        "player_b": {"id": pid_b, "hand": []},
        "player_c": {"id": pid_c, "hand": []},
        "player_d": {"id": pid_d, "hand": []}
    });
    let g: Game = serde_json::from_value(legacy).unwrap();
    assert_eq!(*g.get_id(), id);
    assert_eq!(g.get_player_names()[0].0, pid_a);
    assert_eq!(g.get_player_names()[1].0, pid_b);
    assert_eq!(g.get_player_names()[2].0, pid_c);
    assert_eq!(g.get_player_names()[3].0, pid_d);
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
