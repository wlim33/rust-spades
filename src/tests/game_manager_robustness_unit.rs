#[cfg(feature = "server")]
use super::super::game_manager::{GameManager, GameManagerError};
#[cfg(feature = "server")]
use super::super::GameTransition;

#[cfg(feature = "server")]
#[test]
fn test_game_not_found_errors() {
    let manager = GameManager::new();
    let invalid_id = uuid::Uuid::new_v4();

    // Test all operations with non-existent game ID
    assert!(matches!(
        manager.get_game_state(invalid_id),
        Err(GameManagerError::GameNotFound)
    ));

    assert!(matches!(
        manager.get_hand(invalid_id, uuid::Uuid::new_v4()),
        Err(GameManagerError::GameNotFound)
    ));

    assert!(matches!(
        manager.make_transition(invalid_id, GameTransition::Start),
        Err(GameManagerError::GameNotFound)
    ));

    assert!(matches!(
        manager.remove_game(invalid_id),
        Err(GameManagerError::GameNotFound)
    ));
}

#[cfg(feature = "server")]
#[test]
fn test_invalid_player_id() {
    let manager = GameManager::new();
    let response = manager.create_game(500).unwrap();

    manager
        .make_transition(response.game_id, GameTransition::Start)
        .unwrap();

    // Try to get hand with invalid player ID
    let invalid_player_id = uuid::Uuid::new_v4();
    let result = manager.get_hand(response.game_id, invalid_player_id);
    
    assert!(matches!(result, Err(GameManagerError::GameError(_))));
}

#[cfg(feature = "server")]
#[test]
fn test_concurrent_game_creation() {
    use std::sync::Arc;
    use std::thread;

    let manager = Arc::new(GameManager::new());
    let mut handles = vec![];

    // Create multiple games concurrently
    for _ in 0..10 {
        let manager_clone = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            manager_clone.create_game(500)
        });
        handles.push(handle);
    }

    let mut game_ids = vec![];
    for handle in handles {
        let response = handle.join().unwrap().unwrap();
        game_ids.push(response.game_id);
    }

    // Verify all games were created with unique IDs
    assert_eq!(game_ids.len(), 10);
    let unique_count: std::collections::HashSet<_> = game_ids.iter().collect();
    assert_eq!(unique_count.len(), 10);

    // Verify all games are in the list
    let all_games = manager.list_games().unwrap();
    assert_eq!(all_games.len(), 10);
}

#[cfg(feature = "server")]
#[test]
fn test_concurrent_transitions() {
    use std::sync::Arc;
    use std::thread;

    let manager = Arc::new(GameManager::new());
    let response = manager.create_game(500).unwrap();
    let game_id = response.game_id;

    // Start the game
    manager.make_transition(game_id, GameTransition::Start).unwrap();

    let mut handles = vec![];

    // Try concurrent transitions (should be serialized by RwLock)
    for i in 0..4 {
        let manager_clone = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            manager_clone.make_transition(game_id, GameTransition::Bet(3 + i))
        });
        handles.push(handle);
    }

    let mut results = vec![];
    for handle in handles {
        let result = handle.join().unwrap();
        results.push(result);
    }

    // All 4 bets should succeed (though order may vary due to concurrent access)
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(success_count, 4);
}

#[cfg(feature = "server")]
#[test]
fn test_remove_and_recreate_game() {
    let manager = GameManager::new();
    let response = manager.create_game(500).unwrap();
    let game_id = response.game_id;

    // Verify game exists
    assert!(manager.get_game_state(game_id).is_ok());

    // Remove game
    manager.remove_game(game_id).unwrap();

    // Verify game is gone
    assert!(matches!(
        manager.get_game_state(game_id),
        Err(GameManagerError::GameNotFound)
    ));

    // Try to remove again (should fail)
    assert!(matches!(
        manager.remove_game(game_id),
        Err(GameManagerError::GameNotFound)
    ));

    // Create new game (should work fine)
    let new_response = manager.create_game(500).unwrap();
    assert_ne!(new_response.game_id, game_id);
}

#[cfg(feature = "server")]
#[test]
fn test_invalid_transition_sequence() {
    let manager = GameManager::new();
    let response = manager.create_game(500).unwrap();
    let game_id = response.game_id;

    // Try to bet before starting
    let result = manager.make_transition(game_id, GameTransition::Bet(3));
    assert!(matches!(result, Err(GameManagerError::GameError(_))));

    // Try to play card before starting
    let hand = manager.get_hand(game_id, response.player_ids[0]).unwrap();
    if !hand.cards.is_empty() {
        let card = hand.cards[0].clone();
        let result = manager.make_transition(game_id, GameTransition::Card(card));
        assert!(matches!(result, Err(GameManagerError::GameError(_))));
    }
}

#[cfg(feature = "server")]
#[test]
fn test_game_state_consistency() {
    let manager = GameManager::new();
    let response = manager.create_game(500).unwrap();
    let game_id = response.game_id;

    // Check initial state
    let state = manager.get_game_state(game_id).unwrap();
    assert_eq!(state.game_id, game_id);
    assert!(state.current_player_id.is_none());

    // Start game
    manager.make_transition(game_id, GameTransition::Start).unwrap();

    // Check state after start
    let state = manager.get_game_state(game_id).unwrap();
    assert!(state.current_player_id.is_some());
    assert!(state.team_a_score.is_some());
    assert!(state.team_b_score.is_some());
}

#[cfg(feature = "server")]
#[test]
fn test_list_multiple_games() {
    let manager = GameManager::new();

    // Create multiple games with different max_points
    let game1 = manager.create_game(250).unwrap();
    let game2 = manager.create_game(500).unwrap();
    let game3 = manager.create_game(750).unwrap();

    let games = manager.list_games().unwrap();
    assert_eq!(games.len(), 3);
    assert!(games.contains(&game1.game_id));
    assert!(games.contains(&game2.game_id));
    assert!(games.contains(&game3.game_id));

    // Remove one game
    manager.remove_game(game2.game_id).unwrap();

    let games = manager.list_games().unwrap();
    assert_eq!(games.len(), 2);
    assert!(games.contains(&game1.game_id));
    assert!(!games.contains(&game2.game_id));
    assert!(games.contains(&game3.game_id));
}
