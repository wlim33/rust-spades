use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use tokio::sync::broadcast;
use crate::{Game, GameTransition, State, Card};
use crate::result::TransitionSuccess;
use serde::{Serialize, Deserialize};

/// Event broadcast to WebSocket subscribers when game state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GameEvent {
    StateChanged(GameStateResponse),
}

/// Manages multiple concurrent spades games
#[derive(Clone)]
pub struct GameManager {
    games: Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>,
    broadcasters: Arc<RwLock<HashMap<Uuid, broadcast::Sender<GameEvent>>>>,
}

/// Response for creating a new game
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGameResponse {
    pub game_id: Uuid,
    pub player_ids: [Uuid; 4],
}

/// Response for getting game state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateResponse {
    pub game_id: Uuid,
    pub state: State,
    pub team_a_score: Option<i32>,
    pub team_b_score: Option<i32>,
    pub team_a_bags: Option<i32>,
    pub team_b_bags: Option<i32>,
    pub current_player_id: Option<Uuid>,
}

/// Response for getting a player's hand
#[derive(Debug, Serialize, Deserialize)]
pub struct HandResponse {
    pub player_id: Uuid,
    pub cards: Vec<Card>,
}

/// Request to make a game transition
#[derive(Debug, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub game_id: Uuid,
    pub player_id: Uuid,
    #[serde(flatten)]
    pub transition: TransitionRequestType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransitionRequestType {
    Start,
    Bet { amount: i32 },
    Card { card: Card },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GameManagerError {
    GameNotFound,
    GameError(String),
    LockError,
}

impl GameManager {
    /// Create a new game manager
    pub fn new() -> Self {
        GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new game with 4 players
    pub fn create_game(&self, max_points: i32) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];

        let game = Game::new(game_id, player_ids, max_points);

        let mut games = self.games.write().map_err(|_| GameManagerError::LockError)?;
        games.insert(game_id, Arc::new(RwLock::new(game)));
        drop(games);

        let (tx, _) = broadcast::channel(64);
        let mut broadcasters = self.broadcasters.write().map_err(|_| GameManagerError::LockError)?;
        broadcasters.insert(game_id, tx);

        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    /// Create a new game with pre-assigned player IDs
    pub fn create_game_with_players(&self, player_ids: [Uuid; 4], max_points: i32) -> Result<CreateGameResponse, GameManagerError> {
        let game_id = Uuid::new_v4();
        let game = Game::new(game_id, player_ids, max_points);

        let mut games = self.games.write().map_err(|_| GameManagerError::LockError)?;
        games.insert(game_id, Arc::new(RwLock::new(game)));
        drop(games);

        let (tx, _) = broadcast::channel(64);
        let mut broadcasters = self.broadcasters.write().map_err(|_| GameManagerError::LockError)?;
        broadcasters.insert(game_id, tx);

        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    /// Get the state of a game
    pub fn get_game_state(&self, game_id: Uuid) -> Result<GameStateResponse, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;
        
        Ok(GameStateResponse {
            game_id,
            state: game.get_state().clone(),
            team_a_score: game.get_team_a_score().ok().copied(),
            team_b_score: game.get_team_b_score().ok().copied(),
            team_a_bags: game.get_team_a_bags().ok().copied(),
            team_b_bags: game.get_team_b_bags().ok().copied(),
            current_player_id: game.get_current_player_id().ok().copied(),
        })
    }

    /// Get a player's hand
    pub fn get_hand(&self, game_id: Uuid, player_id: Uuid) -> Result<HandResponse, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let game = game_lock.read().map_err(|_| GameManagerError::LockError)?;
        
        let cards = game.get_hand_by_player_id(player_id)
            .map_err(|e| GameManagerError::GameError(format!("{:?}", e)))?
            .clone();
        
        Ok(HandResponse {
            player_id,
            cards,
        })
    }

    /// Make a game transition (start, bet, play card)
    pub fn make_transition(&self, game_id: Uuid, transition: GameTransition)
        -> Result<TransitionSuccess, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        let game_lock = games.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        let mut game = game_lock.write().map_err(|_| GameManagerError::LockError)?;

        let result = game.play(transition)
            .map_err(|e| GameManagerError::GameError(format!("{:?}", e)))?;

        let state_response = GameStateResponse {
            game_id,
            state: game.get_state().clone(),
            team_a_score: game.get_team_a_score().ok().copied(),
            team_b_score: game.get_team_b_score().ok().copied(),
            team_a_bags: game.get_team_a_bags().ok().copied(),
            team_b_bags: game.get_team_b_bags().ok().copied(),
            current_player_id: game.get_current_player_id().ok().copied(),
        };

        drop(game);
        drop(games);

        if let Ok(broadcasters) = self.broadcasters.read() {
            if let Some(tx) = broadcasters.get(&game_id) {
                let _ = tx.send(GameEvent::StateChanged(state_response));
            }
        }

        Ok(result)
    }

    /// List all active games
    pub fn list_games(&self) -> Result<Vec<Uuid>, GameManagerError> {
        let games = self.games.read().map_err(|_| GameManagerError::LockError)?;
        Ok(games.keys().copied().collect())
    }

    /// Remove a completed game
    pub fn remove_game(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        let mut games = self.games.write().map_err(|_| GameManagerError::LockError)?;
        games.remove(&game_id).ok_or(GameManagerError::GameNotFound)?;
        drop(games);

        if let Ok(mut broadcasters) = self.broadcasters.write() {
            broadcasters.remove(&game_id);
        }

        Ok(())
    }

    /// Subscribe to game state change events
    pub fn subscribe(&self, game_id: Uuid) -> Result<broadcast::Receiver<GameEvent>, GameManagerError> {
        let broadcasters = self.broadcasters.read().map_err(|_| GameManagerError::LockError)?;
        let tx = broadcasters.get(&game_id).ok_or(GameManagerError::GameNotFound)?;
        Ok(tx.subscribe())
    }
}

impl Default for GameManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_game() {
        let manager = GameManager::new();
        let response = manager.create_game(500).unwrap();
        
        assert_ne!(response.game_id, Uuid::nil());
        assert_eq!(response.player_ids.len(), 4);
    }

    #[test]
    fn test_get_game_state() {
        let manager = GameManager::new();
        let response = manager.create_game(500).unwrap();
        
        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.state, State::NotStarted);
        assert_eq!(state.game_id, response.game_id);
    }

    #[test]
    fn test_make_transition() {
        let manager = GameManager::new();
        let response = manager.create_game(500).unwrap();
        
        // Start the game
        let result = manager.make_transition(response.game_id, GameTransition::Start).unwrap();
        assert_eq!(result, TransitionSuccess::Start);
        
        // Verify game is now in betting state
        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.state, State::Betting(0));
    }

    #[test]
    fn test_list_games() {
        let manager = GameManager::new();
        let game1 = manager.create_game(500).unwrap();
        let game2 = manager.create_game(500).unwrap();
        
        let games = manager.list_games().unwrap();
        assert_eq!(games.len(), 2);
        assert!(games.contains(&game1.game_id));
        assert!(games.contains(&game2.game_id));
    }

    #[test]
    fn test_remove_game() {
        let manager = GameManager::new();
        let response = manager.create_game(500).unwrap();

        manager.remove_game(response.game_id).unwrap();

        let games = manager.list_games().unwrap();
        assert_eq!(games.len(), 0);
    }

    #[test]
    fn test_create_game_with_players() {
        let manager = GameManager::new();
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let response = manager.create_game_with_players(player_ids, 500).unwrap();

        assert_eq!(response.player_ids, player_ids);
        assert_ne!(response.game_id, Uuid::nil());

        let state = manager.get_game_state(response.game_id).unwrap();
        assert_eq!(state.state, State::NotStarted);
    }
}
