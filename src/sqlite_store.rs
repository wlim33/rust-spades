use rusqlite::Connection;
use std::sync::Mutex;
use uuid::Uuid;
use crate::Game;

/// SQLite-backed persistence for games.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at the given path.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL
            );"
        ).map_err(|e| e.to_string())?;
        Ok(SqliteStore { conn: Mutex::new(conn) })
    }

    /// Load all persisted games.
    pub fn load_all_games(&self) -> Result<Vec<Game>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT data FROM games").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        }).map_err(|e| e.to_string())?;

        let mut games = Vec::new();
        for row in rows {
            let json = row.map_err(|e| e.to_string())?;
            let game: Game = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to deserialize game: {}", e))?;
            games.push(game);
        }
        Ok(games)
    }

    /// Insert a new game.
    pub fn insert_game(&self, game: &Game) -> Result<(), String> {
        let json = serde_json::to_string(game).map_err(|e| e.to_string())?;
        let id = game.get_id().to_string();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO games (id, data) VALUES (?1, ?2)",
            rusqlite::params![id, json],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Update an existing game.
    pub fn update_game(&self, game: &Game) -> Result<(), String> {
        let json = serde_json::to_string(game).map_err(|e| e.to_string())?;
        let id = game.get_id().to_string();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE games SET data = ?2 WHERE id = ?1",
            rusqlite::params![id, json],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete a game by ID.
    pub fn delete_game(&self, game_id: Uuid) -> Result<(), String> {
        let id = game_id.to_string();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM games WHERE id = ?1",
            rusqlite::params![id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_game() -> Game {
        Game::new(
            Uuid::new_v4(),
            [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
            500,
            None,
        )
    }

    #[test]
    fn test_open_creates_table() {
        let store = SqliteStore::open(":memory:").unwrap();
        let games = store.load_all_games().unwrap();
        assert!(games.is_empty());
    }

    #[test]
    fn test_insert_and_load() {
        let store = SqliteStore::open(":memory:").unwrap();
        let game = make_game();
        let game_id = *game.get_id();

        store.insert_game(&game).unwrap();

        let loaded = store.load_all_games().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(*loaded[0].get_id(), game_id);
    }

    #[test]
    fn test_update_game() {
        let store = SqliteStore::open(":memory:").unwrap();
        let mut game = make_game();
        store.insert_game(&game).unwrap();

        game.play(crate::GameTransition::Start).unwrap();
        store.update_game(&game).unwrap();

        let loaded = store.load_all_games().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(*loaded[0].get_state(), crate::game_state::State::Betting(0));
    }

    #[test]
    fn test_delete_game() {
        let store = SqliteStore::open(":memory:").unwrap();
        let game = make_game();
        let game_id = *game.get_id();

        store.insert_game(&game).unwrap();
        assert_eq!(store.load_all_games().unwrap().len(), 1);

        store.delete_game(game_id).unwrap();
        assert!(store.load_all_games().unwrap().is_empty());
    }

    #[test]
    fn test_insert_multiple_games() {
        let store = SqliteStore::open(":memory:").unwrap();
        for _ in 0..3 {
            store.insert_game(&make_game()).unwrap();
        }
        assert_eq!(store.load_all_games().unwrap().len(), 3);
    }

    #[test]
    fn test_insert_or_replace_same_id() {
        let store = SqliteStore::open(":memory:").unwrap();
        let game = make_game();
        store.insert_game(&game).unwrap();
        // INSERT OR REPLACE with same ID should not duplicate
        store.insert_game(&game).unwrap();
        assert_eq!(store.load_all_games().unwrap().len(), 1);
    }
}
