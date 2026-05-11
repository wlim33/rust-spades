use rusqlite::Connection;
use std::sync::Mutex;
use uuid::Uuid;
use spades::Game;

/// SQLite-backed persistence for games.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at the given path.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute("PRAGMA foreign_keys = ON", []).map_err(|e| e.to_string())?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(())).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS users (
                id              TEXT PRIMARY KEY,
                username        TEXT NOT NULL,
                username_canon  TEXT NOT NULL UNIQUE,
                email           TEXT NOT NULL UNIQUE,
                email_verified  INTEGER NOT NULL DEFAULT 0,
                password_hash   TEXT,
                token_version   INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                last_login_at   TEXT
            );
            CREATE INDEX IF NOT EXISTS users_username_canon ON users(username_canon);
            CREATE TABLE IF NOT EXISTS oauth_accounts (
                provider        TEXT NOT NULL,
                provider_uid    TEXT NOT NULL,
                user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                email           TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (provider, provider_uid)
            );
            CREATE INDEX IF NOT EXISTS oauth_accounts_user_id ON oauth_accounts(user_id);
            CREATE TABLE IF NOT EXISTS auth_tokens (
                token_hash      TEXT PRIMARY KEY,
                user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                purpose         TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at      TEXT NOT NULL,
                used_at         TEXT
            );
            CREATE INDEX IF NOT EXISTS auth_tokens_user_id ON auth_tokens(user_id);
            CREATE INDEX IF NOT EXISTS auth_tokens_expires_at ON auth_tokens(expires_at);
            CREATE TABLE IF NOT EXISTS login_failures (
                user_id         TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
                failure_count   INTEGER NOT NULL DEFAULT 0,
                locked_until    TEXT
            );
            CREATE TABLE IF NOT EXISTS game_seats (
                game_id         TEXT NOT NULL,
                seat_index      INTEGER NOT NULL,
                player_id       TEXT NOT NULL,
                user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
                anon_user_id    TEXT,
                is_bot          INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (game_id, seat_index)
            );
            CREATE INDEX IF NOT EXISTS game_seats_user_id ON game_seats(user_id);
            CREATE INDEX IF NOT EXISTS game_seats_anon_user_id ON game_seats(anon_user_id);"
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

        game.play(spades::GameTransition::Start).unwrap();
        store.update_game(&game).unwrap();

        let loaded = store.load_all_games().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(*loaded[0].get_state(), spades::State::Betting(0));
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

    #[test]
    fn auth_tables_created() {
        let store = SqliteStore::open(":memory:").unwrap();
        let conn = store.conn.lock().unwrap();
        for table in ["users", "oauth_accounts", "auth_tokens", "login_failures", "game_seats"] {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(exists, 1, "table {} not created", table);
        }
    }

    #[test]
    fn users_username_canon_is_unique() {
        let store = SqliteStore::open(":memory:").unwrap();
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, username_canon, email, password_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["u1", "Alice", "alice", "a@x.com", None::<String>],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO users (id, username, username_canon, email, password_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["u2", "ALICE", "alice", "a2@x.com", None::<String>],
        );
        assert!(err.is_err(), "username_canon should be UNIQUE");
    }
}
