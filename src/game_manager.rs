use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use uuid::Uuid;
use crate::{Game, GameTransition, State, Card};
use crate::result::TransitionSuccess;
use crate::storage::{GameStorage, StorageError};
use serde::{Serialize, Deserialize};

/// Manages multiple concurrent spades games
#[derive(Clone)]
pub struct GameManager {
    games: Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>,
    storage: Option<Arc<Mutex<GameStorage>>>,
}

/// Response for creating a new game
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGameResponse {
    pub game_id: Uuid,
    pub player_ids: [Uuid; 4],
}

/// Response for getting game state
#[derive(Debug, Serialize, Deserialize)]
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
    StorageError(String),
}

impl From<StorageError> for GameManagerError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::GameNotFound => GameManagerError::GameNotFound,
            StorageError::DatabaseError(msg) => GameManagerError::StorageError(msg),
            StorageError::SerializationError(msg) => GameManagerError::StorageError(msg),
        }
    }
}

impl GameManager {
    /// Create a new game manager without persistence
    pub fn new() -> Self {
        log::debug!("Creating GameManager without persistence");
        GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            storage: None,
        }
    }

    /// Create a new game manager with SQLite persistence
    pub fn with_storage(db_path: &str) -> Result<Self, GameManagerError> {
        log::info!("Creating GameManager with SQLite storage at: {}", db_path);
        let storage = GameStorage::new(db_path)
            .map_err(|e| GameManagerError::StorageError(format!("{:?}", e)))?;
        
        let manager = GameManager {
            games: Arc::new(RwLock::new(HashMap::new())),
            storage: Some(Arc::new(Mutex::new(storage))),
        };
        
        // Load existing games from storage
        manager.load_all_games()?;
        
        Ok(manager)
    }

    /// Load all games from storage into memory
    fn load_all_games(&self) -> Result<(), GameManagerError> {
        if let Some(storage_mutex) = &self.storage {
            log::info!("Loading games from storage");
            let storage = storage_mutex.lock().map_err(|_| GameManagerError::LockError)?;
            let game_ids = storage.list_games()?;
            
            let mut games = self.games.write().map_err(|e| {
                log::error!("Failed to acquire write lock for loading games: {:?}", e);
                GameManagerError::LockError
            })?;
            
            for game_id in game_ids {
                match storage.load_game(game_id) {
                    Ok(game) => {
                        games.insert(game_id, Arc::new(RwLock::new(game)));
                        log::debug!("Loaded game {} from storage", game_id);
                    }
                    Err(e) => {
                        log::warn!("Failed to load game {}: {:?}", game_id, e);
                    }
                }
            }
            
            log::info!("Loaded {} games from storage", games.len());
        }
        Ok(())
    }

    /// Save a game to storage if persistence is enabled
    fn save_to_storage(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        if let Some(storage_mutex) = &self.storage {
            let games = self.games.read().map_err(|e| {
                log::error!("Failed to acquire read lock for saving: {:?}", e);
                GameManagerError::LockError
            })?;
            
            if let Some(game_lock) = games.get(&game_id) {
                let game = game_lock.read().map_err(|e| {
                    log::error!("Failed to acquire read lock for game {}: {:?}", game_id, e);
                    GameManagerError::LockError
                })?;
                
                let storage = storage_mutex.lock().map_err(|_| GameManagerError::LockError)?;
                storage.save_game(&*game)?;
                log::debug!("Saved game {} to storage", game_id);
            }
        }
        Ok(())
    }

    /// Delete a game from storage if persistence is enabled
    fn delete_from_storage(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        if let Some(storage_mutex) = &self.storage {
            let storage = storage_mutex.lock().map_err(|_| GameManagerError::LockError)?;
            storage.delete_game(game_id)?;
            log::debug!("Deleted game {} from storage", game_id);
        }
        Ok(())
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
        
        log::info!("Creating new game {} with max_points: {}", game_id, max_points);
        let game = Game::new(game_id, player_ids, max_points);
        
        let mut games = self.games.write().map_err(|e| {
            log::error!("Failed to acquire write lock for creating game: {:?}", e);
            GameManagerError::LockError
        })?;
        games.insert(game_id, Arc::new(RwLock::new(game)));
        drop(games); // Release lock before saving
        
        // Save to storage if persistence is enabled
        self.save_to_storage(game_id)?;
        
        log::debug!("Game {} created successfully with players: {:?}", game_id, player_ids);
        
        Ok(CreateGameResponse {
            game_id,
            player_ids,
        })
    }

    /// Get the state of a game
    pub fn get_game_state(&self, game_id: Uuid) -> Result<GameStateResponse, GameManagerError> {
        let games = self.games.read().map_err(|e| {
            log::error!("Failed to acquire read lock for game state: {:?}", e);
            GameManagerError::LockError
        })?;
        let game_lock = games.get(&game_id).ok_or_else(|| {
            log::warn!("Attempted to get state for non-existent game: {}", game_id);
            GameManagerError::GameNotFound
        })?;
        let game = game_lock.read().map_err(|e| {
            log::error!("Failed to acquire read lock for game {}: {:?}", game_id, e);
            GameManagerError::LockError
        })?;
        
        log::debug!("Retrieved state for game {}", game_id);
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
        let games = self.games.read().map_err(|e| {
            log::error!("Failed to acquire read lock for getting hand: {:?}", e);
            GameManagerError::LockError
        })?;
        let game_lock = games.get(&game_id).ok_or_else(|| {
            log::warn!("Attempted to get hand for non-existent game: {}", game_id);
            GameManagerError::GameNotFound
        })?;
        let game = game_lock.read().map_err(|e| {
            log::error!("Failed to acquire read lock for game {}: {:?}", game_id, e);
            GameManagerError::LockError
        })?;
        
        let cards = game.get_hand_by_player_id(player_id)
            .map_err(|e| {
                log::warn!("Failed to get hand for player {} in game {}: {:?}", player_id, game_id, e);
                GameManagerError::GameError(format!("{:?}", e))
            })?
            .clone();
        
        log::debug!("Retrieved hand for player {} in game {}", player_id, game_id);
        Ok(HandResponse {
            player_id,
            cards,
        })
    }

    /// Make a game transition (start, bet, play card)
    pub fn make_transition(&self, game_id: Uuid, transition: GameTransition) 
        -> Result<TransitionSuccess, GameManagerError> {
        let games = self.games.read().map_err(|e| {
            log::error!("Failed to acquire read lock for transition: {:?}", e);
            GameManagerError::LockError
        })?;
        let game_lock = games.get(&game_id).ok_or_else(|| {
            log::warn!("Attempted to make transition for non-existent game: {}", game_id);
            GameManagerError::GameNotFound
        })?;
        let mut game = game_lock.write().map_err(|e| {
            log::error!("Failed to acquire write lock for game {}: {:?}", game_id, e);
            GameManagerError::LockError
        })?;
        
        log::debug!("Making transition for game {}: {:?}", game_id, 
            match &transition {
                GameTransition::Start => "Start",
                GameTransition::Bet(_) => "Bet",
                GameTransition::Card(_) => "Card",
            });
        
        let result = game.play(transition)
            .map_err(|e| {
                log::warn!("Transition failed for game {}: {:?}", game_id, e);
                GameManagerError::GameError(format!("{:?}", e))
            })?;
        
        drop(game); // Release lock before saving
        drop(games);
        
        // Save to storage if persistence is enabled
        self.save_to_storage(game_id)?;
        
        Ok(result)
    }

    /// List all active games
    pub fn list_games(&self) -> Result<Vec<Uuid>, GameManagerError> {
        let games = self.games.read().map_err(|e| {
            log::error!("Failed to acquire read lock for listing games: {:?}", e);
            GameManagerError::LockError
        })?;
        let game_ids: Vec<Uuid> = games.keys().copied().collect();
        log::debug!("Listed {} active games", game_ids.len());
        Ok(game_ids)
    }

    /// Remove a completed game
    pub fn remove_game(&self, game_id: Uuid) -> Result<(), GameManagerError> {
        let mut games = self.games.write().map_err(|e| {
            log::error!("Failed to acquire write lock for removing game: {:?}", e);
            GameManagerError::LockError
        })?;
        games.remove(&game_id).ok_or_else(|| {
            log::warn!("Attempted to remove non-existent game: {}", game_id);
            GameManagerError::GameNotFound
        })?;
        drop(games); // Release lock before deleting from storage
        
        // Delete from storage if persistence is enabled (ignore errors if game not in storage)
        let _ = self.delete_from_storage(game_id);
        
        log::info!("Removed game {}", game_id);
        Ok(())
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
    fn test_game_manager_with_storage() {
        use crate::storage::GameStorage;
        
        // Create a temporary file for the database
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_game_manager.db");
        let db_path_str = db_path.to_str().unwrap();
        
        // Clean up any existing database
        let _ = std::fs::remove_file(&db_path);
        
        // Create manager with storage
        let manager = GameManager::with_storage(db_path_str).unwrap();
        
        // Create and save a game
        let response = manager.create_game(500).unwrap();
        let game_id = response.game_id;
        
        // Start the game
        manager.make_transition(game_id, GameTransition::Start).unwrap();
        
        // Create a new manager instance to test loading from storage
        let manager2 = GameManager::with_storage(db_path_str).unwrap();
        
        // Verify the game was loaded
        let games = manager2.list_games().unwrap();
        assert_eq!(games.len(), 1);
        assert!(games.contains(&game_id));
        
        // Verify game state was preserved
        let state = manager2.get_game_state(game_id).unwrap();
        assert_eq!(state.state, State::Betting(0));
        
        // Clean up
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_game_manager_persistence() {
        use crate::storage::GameStorage;
        
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_persistence.db");
        let db_path_str = db_path.to_str().unwrap();
        
        // Clean up any existing database
        let _ = std::fs::remove_file(&db_path);
        
        // Create manager and add some games
        {
            let manager = GameManager::with_storage(db_path_str).unwrap();
            let game1 = manager.create_game(500).unwrap();
            let game2 = manager.create_game(750).unwrap();
            
            manager.make_transition(game1.game_id, GameTransition::Start).unwrap();
            manager.make_transition(game2.game_id, GameTransition::Start).unwrap();
        } // Manager goes out of scope
        
        // Create new manager and verify games were persisted
        {
            let manager = GameManager::with_storage(db_path_str).unwrap();
            let games = manager.list_games().unwrap();
            assert_eq!(games.len(), 2);
        }
        
        // Clean up
        let _ = std::fs::remove_file(&db_path);
    }
}
