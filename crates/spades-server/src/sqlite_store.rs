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

    pub fn insert_user(&self, new: &crate::auth::users::NewUser) -> Result<uuid::Uuid, String> {
        use crate::auth::users::canonicalize_username;
        let id = uuid::Uuid::new_v4();
        let canon = canonicalize_username(&new.username);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO users (id, username, username_canon, email, email_verified, password_hash, token_version) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            rusqlite::params![
                id.to_string(),
                &new.username,
                canon,
                &new.email,
                new.email_verified as i32,
                new.password_hash.as_deref(),
            ],
        ).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed: users.username_canon") {
                "username_taken".to_string()
            } else if msg.contains("UNIQUE constraint failed: users.email") {
                "email_taken".to_string()
            } else {
                msg
            }
        })?;
        Ok(id)
    }

    pub fn find_user_by_id(&self, id: uuid::Uuid) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username, username_canon, email, email_verified, password_hash, token_version, created_at, last_login_at \
             FROM users WHERE id = ?1",
            rusqlite::params![id.to_string()],
            row_to_user,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn find_user_by_email(&self, email: &str) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username, username_canon, email, email_verified, password_hash, token_version, created_at, last_login_at \
             FROM users WHERE email = ?1",
            rusqlite::params![email],
            row_to_user,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn find_user_by_username(&self, username: &str) -> Result<Option<crate::auth::users::User>, String> {
        use crate::auth::users::canonicalize_username;
        let canon = canonicalize_username(username);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username, username_canon, email, email_verified, password_hash, token_version, created_at, last_login_at \
             FROM users WHERE username_canon = ?1",
            rusqlite::params![canon],
            row_to_user,
        ).map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn update_user_password(&self, user_id: uuid::Uuid, new_hash: &str) -> Result<i32, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let new_version: i32 = conn.query_row(
            "UPDATE users SET password_hash = ?1, token_version = token_version + 1 \
             WHERE id = ?2 RETURNING token_version",
            rusqlite::params![new_hash, user_id.to_string()],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        Ok(new_version)
    }

    pub fn set_user_email_verified(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET email_verified = 1 WHERE id = ?1",
            rusqlite::params![user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn touch_user_login(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET last_login_at = datetime('now') WHERE id = ?1",
            rusqlite::params![user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_user_email(&self, user_id: uuid::Uuid, new_email: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET email = ?1, email_verified = 0 WHERE id = ?2",
            rusqlite::params![new_email, user_id.to_string()],
        ).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed: users.email") {
                "email_taken".to_string()
            } else {
                msg
            }
        })?;
        Ok(())
    }

    pub fn claim_anon_game_seats(&self, anon_id: uuid::Uuid, user_id: uuid::Uuid) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let n = conn.execute(
            "UPDATE game_seats SET user_id = ?1 WHERE anon_user_id = ?2 AND user_id IS NULL",
            rusqlite::params![user_id.to_string(), anon_id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn insert_auth_token(
        &self,
        token_hash: &str,
        user_id: uuid::Uuid,
        purpose: &str,
        ttl_secs: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at) \
             VALUES (?1, ?2, ?3, datetime('now', ?4))",
            rusqlite::params![token_hash, user_id.to_string(), purpose, format!("+{ttl_secs} seconds")],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn row_to_user(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::auth::users::User> {
    let id_s: String = r.get(0)?;
    let id = uuid::Uuid::parse_str(&id_s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(
        0, rusqlite::types::Type::Text, Box::new(e),
    ))?;
    Ok(crate::auth::users::User {
        id,
        username: r.get(1)?,
        username_canon: r.get(2)?,
        email: r.get(3)?,
        email_verified: r.get::<_, i32>(4)? != 0,
        password_hash: r.get(5)?,
        token_version: r.get(6)?,
        created_at: r.get(7)?,
        last_login_at: r.get(8)?,
    })
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

    use crate::auth::users::NewUser;

    fn new_user(name: &str, email: &str) -> NewUser {
        NewUser {
            username: name.into(),
            email: email.into(),
            password_hash: Some("$argon2id$dummy".into()),
            email_verified: false,
        }
    }

    #[test]
    fn insert_and_find_user() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        let u = store.find_user_by_id(id).unwrap().unwrap();
        assert_eq!(u.username, "Alice");
        assert_eq!(u.username_canon, "alice");
        assert_eq!(u.email_verified, false);
        assert_eq!(u.token_version, 0);
    }

    #[test]
    fn find_by_email_and_username_works() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        let by_email = store.find_user_by_email("alice@x.com").unwrap().unwrap();
        let by_username = store.find_user_by_username("ALICE").unwrap().unwrap();
        assert_eq!(by_email.id, by_username.id);
    }

    #[test]
    fn duplicate_username_rejected() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "a1@x.com")).unwrap();
        let err = store.insert_user(&new_user("alice", "a2@x.com")).unwrap_err();
        assert_eq!(err, "username_taken");
    }

    #[test]
    fn duplicate_email_rejected() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "a@x.com")).unwrap();
        let err = store.insert_user(&new_user("Bob", "a@x.com")).unwrap_err();
        assert_eq!(err, "email_taken");
    }

    #[test]
    fn password_update_bumps_token_version() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        let v1 = store.update_user_password(id, "$argon2id$new").unwrap();
        let v2 = store.update_user_password(id, "$argon2id$newer").unwrap();
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
    }

    #[test]
    fn email_verify_and_touch_login() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store.insert_user(&new_user("Alice", "alice@x.com")).unwrap();
        store.set_user_email_verified(id).unwrap();
        store.touch_user_login(id).unwrap();
        let u = store.find_user_by_id(id).unwrap().unwrap();
        assert!(u.email_verified);
        assert!(u.last_login_at.is_some());
    }
}
