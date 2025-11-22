#[cfg(feature = "server")]
use rusqlite::{Connection, params};
#[cfg(feature = "server")]
use serde_json;
#[cfg(feature = "server")]
use std::path::Path;
#[cfg(feature = "server")]
use uuid::Uuid;
#[cfg(feature = "server")]
use crate::Game;

#[cfg(feature = "server")]
#[derive(Debug)]
pub enum StorageError {
    DatabaseError(String),
    SerializationError(String),
    GameNotFound,
}

#[cfg(feature = "server")]
impl From<rusqlite::Error> for StorageError {
    fn from(err: rusqlite::Error) -> Self {
        StorageError::DatabaseError(err.to_string())
    }
}

#[cfg(feature = "server")]
impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::SerializationError(err.to_string())
    }
}

#[cfg(feature = "server")]
pub struct GameStorage {
    conn: Connection,
}

#[cfg(feature = "server")]
impl GameStorage {
    /// Create a new GameStorage with the specified database path
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, StorageError> {
        log::info!("Initializing game storage at: {:?}", db_path.as_ref());
        let conn = Connection::open(db_path)?;
        
        // Create the games table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                game_data TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        
        log::info!("Game storage initialized successfully");
        Ok(GameStorage { conn })
    }

    /// Create an in-memory database (useful for testing)
    pub fn new_in_memory() -> Result<Self, StorageError> {
        log::debug!("Creating in-memory game storage");
        let conn = Connection::open_in_memory()?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                game_data TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        
        Ok(GameStorage { conn })
    }

    /// Save a game to the database
    pub fn save_game(&self, game: &Game) -> Result<(), StorageError> {
        let game_id = game.get_id().to_string();
        log::debug!("Saving game {} to storage", game_id);
        
        // Serialize the game to JSON
        let game_json = serde_json::to_string(game)?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        // Try to update first, if that fails, insert
        let updated = self.conn.execute(
            "UPDATE games SET game_data = ?1, updated_at = ?2 WHERE id = ?3",
            params![game_json, timestamp, game_id],
        )?;
        
        if updated == 0 {
            // Game doesn't exist, insert it
            self.conn.execute(
                "INSERT INTO games (id, game_data, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![game_id, game_json, timestamp, timestamp],
            )?;
            log::info!("Game {} saved to storage (new)", game_id);
        } else {
            log::debug!("Game {} updated in storage", game_id);
        }
        
        Ok(())
    }

    /// Load a game from the database
    pub fn load_game(&self, game_id: Uuid) -> Result<Game, StorageError> {
        let game_id_str = game_id.to_string();
        log::debug!("Loading game {} from storage", game_id_str);
        
        let game_json: String = self.conn.query_row(
            "SELECT game_data FROM games WHERE id = ?1",
            params![game_id_str],
            |row| row.get(0),
        ).map_err(|e| {
            if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                log::warn!("Game {} not found in storage", game_id_str);
                StorageError::GameNotFound
            } else {
                StorageError::from(e)
            }
        })?;
        
        let game: Game = serde_json::from_str(&game_json)?;
        log::debug!("Game {} loaded from storage", game_id_str);
        Ok(game)
    }

    /// Delete a game from the database
    pub fn delete_game(&self, game_id: Uuid) -> Result<(), StorageError> {
        let game_id_str = game_id.to_string();
        log::debug!("Deleting game {} from storage", game_id_str);
        
        let deleted = self.conn.execute(
            "DELETE FROM games WHERE id = ?1",
            params![game_id_str],
        )?;
        
        if deleted == 0 {
            log::warn!("Attempted to delete non-existent game {} from storage", game_id_str);
            return Err(StorageError::GameNotFound);
        }
        
        log::info!("Game {} deleted from storage", game_id_str);
        Ok(())
    }

    /// List all game IDs in the database
    pub fn list_games(&self) -> Result<Vec<Uuid>, StorageError> {
        log::debug!("Listing all games from storage");
        
        let mut stmt = self.conn.prepare("SELECT id FROM games")?;
        let game_ids: Result<Vec<Uuid>, rusqlite::Error> = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            match Uuid::parse_str(&id_str) {
                Ok(uuid) => Ok(uuid),
                Err(e) => {
                    log::error!("Failed to parse UUID from database: {} - {}", id_str, e);
                    Err(rusqlite::Error::InvalidQuery)
                }
            }
        })?
        .collect();
        
        let game_ids = game_ids?;
        log::debug!("Found {} games in storage", game_ids.len());
        Ok(game_ids)
    }

    /// Get the count of games in the database
    pub fn count_games(&self) -> Result<usize, StorageError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM games",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Delete all games from the database (useful for testing)
    pub fn clear_all(&self) -> Result<(), StorageError> {
        log::warn!("Clearing all games from storage");
        self.conn.execute("DELETE FROM games", [])?;
        log::info!("All games cleared from storage");
        Ok(())
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn test_create_in_memory_storage() {
        let storage = GameStorage::new_in_memory().unwrap();
        assert_eq!(storage.count_games().unwrap(), 0);
    }

    #[test]
    fn test_save_and_load_game() {
        let storage = GameStorage::new_in_memory().unwrap();
        
        let game_id = Uuid::new_v4();
        let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let game = Game::new(game_id, player_ids, 500);
        
        // Save the game
        storage.save_game(&game).unwrap();
        
        // Load the game
        let loaded_game = storage.load_game(game_id).unwrap();
        assert_eq!(loaded_game.get_id(), game.get_id());
    }

    #[test]
    fn test_update_game() {
        let storage = GameStorage::new_in_memory().unwrap();
        
        let game_id = Uuid::new_v4();
        let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let mut game = Game::new(game_id, player_ids, 500);
        
        // Save the game
        storage.save_game(&game).unwrap();
        
        // Start the game
        game.play(crate::GameTransition::Start).unwrap();
        
        // Update the game
        storage.save_game(&game).unwrap();
        
        // Load and verify
        let loaded_game = storage.load_game(game_id).unwrap();
        assert_eq!(*loaded_game.get_state(), crate::State::Betting(0));
    }

    #[test]
    fn test_delete_game() {
        let storage = GameStorage::new_in_memory().unwrap();
        
        let game_id = Uuid::new_v4();
        let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let game = Game::new(game_id, player_ids, 500);
        
        storage.save_game(&game).unwrap();
        assert_eq!(storage.count_games().unwrap(), 1);
        
        storage.delete_game(game_id).unwrap();
        assert_eq!(storage.count_games().unwrap(), 0);
    }

    #[test]
    fn test_delete_nonexistent_game() {
        let storage = GameStorage::new_in_memory().unwrap();
        let game_id = Uuid::new_v4();
        
        let result = storage.delete_game(game_id);
        assert!(matches!(result, Err(StorageError::GameNotFound)));
    }

    #[test]
    fn test_load_nonexistent_game() {
        let storage = GameStorage::new_in_memory().unwrap();
        let game_id = Uuid::new_v4();
        
        let result = storage.load_game(game_id);
        assert!(matches!(result, Err(StorageError::GameNotFound)));
    }

    #[test]
    fn test_list_games() {
        let storage = GameStorage::new_in_memory().unwrap();
        
        let game1_id = Uuid::new_v4();
        let game2_id = Uuid::new_v4();
        let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        
        let game1 = Game::new(game1_id, player_ids, 500);
        let game2 = Game::new(game2_id, player_ids, 500);
        
        storage.save_game(&game1).unwrap();
        storage.save_game(&game2).unwrap();
        
        let games = storage.list_games().unwrap();
        assert_eq!(games.len(), 2);
        assert!(games.contains(&game1_id));
        assert!(games.contains(&game2_id));
    }

    #[test]
    fn test_clear_all() {
        let storage = GameStorage::new_in_memory().unwrap();
        
        let player_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        
        for _ in 0..5 {
            let game = Game::new(Uuid::new_v4(), player_ids, 500);
            storage.save_game(&game).unwrap();
        }
        
        assert_eq!(storage.count_games().unwrap(), 5);
        
        storage.clear_all().unwrap();
        assert_eq!(storage.count_games().unwrap(), 0);
    }
}
