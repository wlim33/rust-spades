use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use spades::Game;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::warn;
use uuid::Uuid;

/// Connection pool over one SQLite database. Replaces the previous single
/// `Mutex<Connection>`: that serialized every read behind one lock, which
/// negated WAL's reader/writer concurrency. With a pool, independent reads
/// run on independent connections in parallel (WAL lets them proceed while a
/// writer commits), and a blocking acquire no longer parks a tokio worker on
/// a shared mutex for the duration of someone else's query.
type SqlitePool = r2d2::Pool<SqliteConnectionManager>;

/// A row from `api_tokens` minus the hash. Returned by `list_api_tokens`
/// for UI display; the plaintext token is shown to the user exactly once,
/// at issue time.
#[derive(Debug, Clone)]
pub struct ApiTokenRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// A user's seat in one game, joined with the stamped outcome and a peek at
/// the (possibly absent) live game state. Returned by `profile_games_for_user`.
#[derive(Debug, Clone)]
pub struct ProfileGameRow {
    pub game_id: Uuid,
    pub seat_index: i32,
    pub player_id: Uuid,
    /// `won` / `lost` / `tied` / `aborted`, or `None` until the game finishes
    /// (or for games that ended before result tracking existed).
    pub result: Option<String>,
    pub team_score: Option<i32>,
    pub opp_score: Option<i32>,
    /// `json_extract(games.data, '$.state')` — `Some` only while the game row
    /// survives. Distinguishes a live in-progress game from a pruned old one.
    pub live_state: Option<String>,
}

/// SQLite-backed persistence for games.
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at the given path.
    pub fn open(path: &str) -> Result<Self, String> {
        let manager = if path == ":memory:" {
            // A bare `:memory:` path gives every pooled connection its OWN
            // private database, which would make the pool incoherent. Route
            // them to one shared-cache in-memory DB instead; a per-open
            // sequence keeps independent stores (e.g. each test) isolated.
            static MEM_SEQ: AtomicU64 = AtomicU64::new(0);
            let n = MEM_SEQ.fetch_add(1, Ordering::Relaxed);
            SqliteConnectionManager::file(format!("file:spades_mem_{n}?mode=memory&cache=shared"))
        } else {
            SqliteConnectionManager::file(path)
        }
        .with_init(|c| {
            // Per-connection PRAGMAs — every pooled connection needs them.
            // journal_mode=WAL is a persistent DB-level switch on a file DB
            // (idempotent to re-run) and a no-op on the shared-cache memory
            // DB. synchronous=NORMAL is safe under WAL and drops an fsync per
            // commit; busy_timeout gives the writer lock a 5s budget instead
            // of failing fast with SQLITE_BUSY.
            c.execute_batch(
                "PRAGMA foreign_keys = ON; \
                 PRAGMA busy_timeout = 5000; \
                 PRAGMA journal_mode = WAL; \
                 PRAGMA synchronous = NORMAL;",
            )
        });

        let pool = r2d2::Pool::builder()
            .max_size(8)
            // Keep a connection idle so a shared-cache `:memory:` DB isn't
            // dropped (and recreated empty) when the pool goes fully idle.
            .min_idle(Some(1))
            .build(manager)
            .map_err(|e| e.to_string())?;

        // Schema creation + idempotent migration run once, on one connection.
        let conn = pool.get().map_err(|e| e.to_string())?;

        // Idempotent migration: a `users` table from before the Glicko-2
        // ratings work doesn't have these columns. `CREATE TABLE IF NOT
        // EXISTS` doesn't backfill; do it explicitly.
        let table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='users')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if table_exists {
            for (col, ddl) in [
                (
                    "rating",
                    "ALTER TABLE users ADD COLUMN rating REAL NOT NULL DEFAULT 1500.0",
                ),
                (
                    "rd",
                    "ALTER TABLE users ADD COLUMN rd REAL NOT NULL DEFAULT 350.0",
                ),
                (
                    "volatility",
                    "ALTER TABLE users ADD COLUMN volatility REAL NOT NULL DEFAULT 0.06",
                ),
            ] {
                let present: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('users') WHERE name = ?1)",
                        rusqlite::params![col],
                        |r| r.get(0),
                    )
                    .unwrap_or(false);
                if !present {
                    conn.execute(ddl, []).map_err(|e| e.to_string())?;
                }
            }
        }
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
                last_login_at   TEXT,
                rating          REAL NOT NULL DEFAULT 1500.0,
                rd              REAL NOT NULL DEFAULT 350.0,
                volatility      REAL NOT NULL DEFAULT 0.06
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
            CREATE TABLE IF NOT EXISTS api_tokens (
                id              TEXT PRIMARY KEY,
                user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                token_hash      TEXT NOT NULL UNIQUE,
                name            TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                last_used_at    TEXT
            );
            CREATE INDEX IF NOT EXISTS api_tokens_user_id ON api_tokens(user_id);
            CREATE TABLE IF NOT EXISTS game_seats (
                game_id         TEXT NOT NULL,
                seat_index      INTEGER NOT NULL,
                player_id       TEXT NOT NULL,
                user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
                anon_user_id    TEXT,
                is_bot          INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                -- Terminal-game outcome from this seat's perspective, stamped
                -- when the game completes/aborts. NULL until then (or for games
                -- that finished before this column existed). team/opp_score are
                -- the seat's own team score vs the opponents'.
                result          TEXT,
                team_score      INTEGER,
                opp_score       INTEGER,
                PRIMARY KEY (game_id, seat_index)
            );
            CREATE INDEX IF NOT EXISTS game_seats_user_id ON game_seats(user_id);
            CREATE INDEX IF NOT EXISTS game_seats_anon_user_id ON game_seats(anon_user_id);",
        )
        .map_err(|e| e.to_string())?;

        // Idempotent migration: a `game_seats` table from before per-game
        // result tracking lacks these columns. CREATE TABLE IF NOT EXISTS above
        // won't backfill an existing table, so add them explicitly.
        for (col, ddl) in [
            ("result", "ALTER TABLE game_seats ADD COLUMN result TEXT"),
            (
                "team_score",
                "ALTER TABLE game_seats ADD COLUMN team_score INTEGER",
            ),
            (
                "opp_score",
                "ALTER TABLE game_seats ADD COLUMN opp_score INTEGER",
            ),
        ] {
            let present: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('game_seats') WHERE name = ?1)",
                    rusqlite::params![col],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if !present {
                conn.execute(ddl, []).map_err(|e| e.to_string())?;
            }
        }
        drop(conn);
        Ok(SqliteStore { pool })
    }

    /// Load a single persisted game by id. Returns `Ok(None)` if the row
    /// is absent — distinct from a deserialization error.
    pub fn load_game_by_id(&self, id: Uuid) -> Result<Option<Game>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let row: Result<String, rusqlite::Error> = conn.query_row(
            "SELECT data FROM games WHERE id = ?1",
            rusqlite::params![id.to_string()],
            |row| row.get::<_, String>(0),
        );
        match row {
            Ok(json) => {
                let game: Game = serde_json::from_str(&json).map_err(|e| e.to_string())?;
                Ok(Some(game))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Load all persisted games.
    ///
    /// A row whose blob no longer deserializes to a full `Game` (a partial or
    /// legacy record) is skipped and logged rather than aborting the load — a
    /// single corrupt blob must never take down startup. The readable games are
    /// always returned.
    pub fn load_all_games(&self) -> Result<Vec<Game>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, data FROM games")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((id, json))
            })
            .map_err(|e| e.to_string())?;

        let mut games = Vec::new();
        let mut skipped = 0usize;
        for row in rows {
            let (id, json) = row.map_err(|e| e.to_string())?;
            match serde_json::from_str::<Game>(&json) {
                Ok(game) => games.push(game),
                Err(e) => {
                    skipped += 1;
                    warn!(game_id = %id, error = %e, "skipping unreadable game row at startup");
                }
            }
        }
        if skipped > 0 {
            warn!(
                skipped,
                loaded = games.len(),
                "some game rows could not be deserialized and were skipped"
            );
        }
        Ok(games)
    }

    /// Insert a new game.
    pub fn insert_game(&self, game: &Game) -> Result<(), String> {
        let json = serde_json::to_string(game).map_err(|e| e.to_string())?;
        let id = game.get_id().to_string();
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO games (id, data) VALUES (?1, ?2)",
            rusqlite::params![id, json],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Update an existing game.
    pub fn update_game(&self, game: &Game) -> Result<(), String> {
        let json = serde_json::to_string(game).map_err(|e| e.to_string())?;
        let id = *game.get_id();
        self.update_game_serialized(id, json)
    }

    /// Like `update_game` but takes a pre-serialized JSON blob. Callers
    /// inside an async/actor context serialize on their own thread (fast)
    /// and hand the `String` to `spawn_blocking`, so the blocking SQL
    /// write doesn't stall a tokio worker.
    pub fn update_game_serialized(&self, id: Uuid, json: String) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE games SET data = ?2 WHERE id = ?1",
            rusqlite::params![id.to_string(), json],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete a game by ID.
    pub fn delete_game(&self, game_id: Uuid) -> Result<(), String> {
        let id = game_id.to_string();
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM games WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete games that should never be loaded into memory at startup:
    /// terminal games (Completed / Aborted — disposable leftovers from a
    /// crash before the TTL sweep) and runaway games whose move history has
    /// grown past `max_hands` (the never-terminating bot games that caused
    /// the VPS OOM). Filtering in SQL means these rows are dropped without
    /// ever being deserialized, so peak startup memory tracks only live,
    /// in-bounds games. Returns the number of rows deleted.
    ///
    /// `state` is plain serde: unit variants serialize as the strings
    /// `"Completed"` / `"Aborted"`, so `json_extract` matches them exactly
    /// while leaving `{"Betting":n}` / `{"Trick":n}` / `"NotStarted"` alone.
    pub fn prune_stale_games(&self, max_hands: i64) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM games \
             WHERE json_extract(data, '$.state') IN ('Completed', 'Aborted') \
                OR json_array_length(data, '$.hands_played') > ?1",
            rusqlite::params![max_hands],
        )
        .map_err(|e| e.to_string())
    }

    pub fn insert_user(&self, new: &crate::auth::users::NewUser) -> Result<uuid::Uuid, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        insert_user_in(&conn, new)
    }

    pub fn find_user_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        find_user_by_id_in(&conn, id)
    }

    pub fn find_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
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

    pub fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<crate::auth::users::User>, String> {
        use crate::auth::users::canonicalize_username;
        let canon = canonicalize_username(username);
        let conn = self.pool.get().map_err(|e| e.to_string())?;
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
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        update_user_password_in(&conn, user_id, new_hash)
    }

    pub fn set_user_email_verified(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        set_user_email_verified_in(&conn, user_id)
    }

    pub fn touch_user_login(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        touch_user_login_in(&conn, user_id)
    }

    /// Read a user's current Glicko-2 rating. Returns `Ok(None)` if the
    /// user row is absent — the actor's rating-update path uses this to
    /// skip anon / bot seats that aren't claimed yet.
    pub fn get_user_rating(
        &self,
        user_id: uuid::Uuid,
    ) -> Result<Option<crate::ratings::Rating>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let row: Result<(f64, f64, f64), _> = conn.query_row(
            "SELECT rating, rd, volatility FROM users WHERE id = ?1",
            rusqlite::params![user_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        );
        match row {
            Ok((rating, rd, volatility)) => Ok(Some(crate::ratings::Rating {
                rating,
                rd,
                volatility,
            })),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Write a user's new Glicko-2 rating. Caller is responsible for
    /// having computed the update from the pre-game rating slate.
    pub fn set_user_rating(
        &self,
        user_id: uuid::Uuid,
        rating: &crate::ratings::Rating,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE users SET rating = ?2, rd = ?3, volatility = ?4 WHERE id = ?1",
            rusqlite::params![
                user_id.to_string(),
                rating.rating,
                rating.rd,
                rating.volatility,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Issue a new API token. Caller passes the SHA-256-hashed token —
    /// we never persist the plaintext. Returns the new row's `id` so
    /// the caller can hand both `id` and the user-visible plaintext
    /// back in the response.
    pub fn create_api_token(
        &self,
        user_id: uuid::Uuid,
        token_hash: &str,
        name: &str,
    ) -> Result<uuid::Uuid, String> {
        let id = uuid::Uuid::new_v4();
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO api_tokens (id, user_id, token_hash, name) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id.to_string(), user_id.to_string(), token_hash, name],
        )
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn list_api_tokens(&self, user_id: uuid::Uuid) -> Result<Vec<ApiTokenRow>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, created_at, last_used_at FROM api_tokens WHERE user_id = ?1 \
             ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![user_id.to_string()], |r| {
                Ok(ApiTokenRow {
                    id: uuid_col(0, &r.get::<_, String>(0)?)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    last_used_at: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn revoke_api_token(
        &self,
        token_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> Result<bool, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "DELETE FROM api_tokens WHERE id = ?1 AND user_id = ?2",
                rusqlite::params![token_id.to_string(), user_id.to_string()],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// Resolve a Bearer token to its owning user. Updates last_used_at
    /// fire-and-forget so it doesn't slow the auth path.
    pub fn find_user_by_api_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<crate::auth::users::User>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let user_id: Option<String> = conn
            .query_row(
                "SELECT user_id FROM api_tokens WHERE token_hash = ?1",
                rusqlite::params![token_hash],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.to_string()),
            })?;
        let Some(user_id_s) = user_id else {
            return Ok(None);
        };
        // Bump last_used_at. Failures here are non-fatal.
        let _ = conn.execute(
            "UPDATE api_tokens SET last_used_at = datetime('now') WHERE token_hash = ?1",
            rusqlite::params![token_hash],
        );
        let user_id = uuid::Uuid::parse_str(&user_id_s).map_err(|e| e.to_string())?;
        drop(conn);
        self.find_user_by_id(user_id)
    }

    pub fn update_user_email(&self, user_id: uuid::Uuid, new_email: &str) -> Result<i32, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let new_version: i32 = conn.query_row(
            "UPDATE users SET email = ?1, email_verified = 0, token_version = token_version + 1 \
             WHERE id = ?2 RETURNING token_version",
            rusqlite::params![new_email, user_id.to_string()],
            |r| r.get(0),
        ).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed: users.email") {
                "email_taken".to_string()
            } else {
                msg
            }
        })?;
        Ok(new_version)
    }

    pub fn find_oauth_account(
        &self,
        provider: &str,
        provider_uid: &str,
    ) -> Result<Option<uuid::Uuid>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT user_id FROM oauth_accounts WHERE provider = ?1 AND provider_uid = ?2",
            rusqlite::params![provider, provider_uid],
            |r| {
                let s: String = r.get(0)?;
                uuid_col(0, &s)
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn insert_oauth_account(
        &self,
        provider: &str,
        provider_uid: &str,
        user_id: uuid::Uuid,
        email: &str,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        insert_oauth_account_in(&conn, provider, provider_uid, user_id, email)
    }

    pub fn claim_anon_game_seats(
        &self,
        anon_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        claim_anon_game_seats_in(&conn, anon_id, user_id)
    }

    pub fn insert_auth_token(
        &self,
        token_hash: &str,
        user_id: uuid::Uuid,
        purpose: &str,
        ttl_secs: i64,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        insert_auth_token_in(&conn, token_hash, user_id, purpose, ttl_secs)
    }

    pub fn get_lockout(&self, user_id: uuid::Uuid) -> Result<Option<String>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT locked_until FROM login_failures WHERE user_id = ?1",
            rusqlite::params![user_id.to_string()],
            |r| r.get::<_, Option<String>>(0),
        )
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    /// Increment failure_count, returning the new value.
    /// Resets to 1 if a previous lockout window has already expired.
    pub fn bump_login_failure(&self, user_id: uuid::Uuid) -> Result<i32, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        // If locked_until is non-null and has passed, treat as a fresh window.
        conn.execute(
            "INSERT INTO login_failures (user_id, failure_count) VALUES (?1, 1) \
             ON CONFLICT(user_id) DO UPDATE SET \
                failure_count = CASE \
                    WHEN locked_until IS NOT NULL AND locked_until < datetime('now') THEN 1 \
                    ELSE failure_count + 1 \
                END, \
                locked_until = CASE \
                    WHEN locked_until IS NOT NULL AND locked_until < datetime('now') THEN NULL \
                    ELSE locked_until \
                END",
            rusqlite::params![user_id.to_string()],
        )
        .map_err(|e| e.to_string())?;
        let n: i32 = conn
            .query_row(
                "SELECT failure_count FROM login_failures WHERE user_id = ?1",
                rusqlite::params![user_id.to_string()],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn set_lockout(&self, user_id: uuid::Uuid, secs: i64) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE login_failures SET locked_until = datetime('now', ?2) WHERE user_id = ?1",
            rusqlite::params![user_id.to_string(), format!("+{secs} seconds")],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_login_failures(&self, user_id: uuid::Uuid) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        clear_login_failures_in(&conn, user_id)
    }

    pub fn consume_auth_token(
        &self,
        token_hash: &str,
        expected_purpose: &str,
    ) -> Result<crate::auth::tokens::ConsumedToken, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        consume_auth_token_in(&conn, token_hash, expected_purpose)
    }

    pub fn cleanup_expired_tokens(&self) -> Result<usize, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "DELETE FROM auth_tokens WHERE expires_at < datetime('now') OR used_at IS NOT NULL",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    pub fn insert_game_seat(
        &self,
        game_id: uuid::Uuid,
        seat_index: i32,
        player_id: uuid::Uuid,
        owner: crate::auth::game_seats::SeatOwner,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO game_seats (game_id, seat_index, player_id, user_id, anon_user_id, is_bot) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                game_id.to_string(),
                seat_index,
                player_id.to_string(),
                owner.user_id.map(|u| u.to_string()),
                owner.anon_user_id.map(|u| u.to_string()),
                owner.is_bot as i32,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_game_seat_owner(
        &self,
        game_id: uuid::Uuid,
        seat_index: i32,
        owner: crate::auth::game_seats::SeatOwner,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE game_seats SET user_id = ?3, anon_user_id = ?4, is_bot = ?5 \
             WHERE game_id = ?1 AND seat_index = ?2",
            rusqlite::params![
                game_id.to_string(),
                seat_index,
                owner.user_id.map(|u| u.to_string()),
                owner.anon_user_id.map(|u| u.to_string()),
                owner.is_bot as i32,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Lookup a seat by `(game_id, player_id)`. The seat's owner fields
    /// (user_id / anon_user_id) authorise access to that seat's hand —
    /// without this lookup the WS layer would have to round-trip through
    /// the actor for the player → seat-index mapping and a second lookup
    /// for the seat owner.
    pub fn game_seat_by_player_id(
        &self,
        game_id: uuid::Uuid,
        player_id: uuid::Uuid,
    ) -> Result<Option<crate::auth::game_seats::SeatRow>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT game_id, seat_index, player_id, user_id, anon_user_id, is_bot \
             FROM game_seats WHERE game_id = ?1 AND player_id = ?2",
            rusqlite::params![game_id.to_string(), player_id.to_string()],
            seat_row,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    pub fn game_seat(
        &self,
        game_id: uuid::Uuid,
        seat_index: i32,
    ) -> Result<Option<crate::auth::game_seats::SeatRow>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT game_id, seat_index, player_id, user_id, anon_user_id, is_bot \
             FROM game_seats WHERE game_id = ?1 AND seat_index = ?2",
            rusqlite::params![game_id.to_string(), seat_index],
            seat_row,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    }

    /// All seats for a game in a single query, ordered by seat index. The
    /// transition / delete / WS authz paths need every seat to check
    /// ownership and the turn; fetching them one-by-one cost up to five
    /// serialized round-trips per move. A game has at most four rows, so
    /// this is one PK-prefix scan.
    pub fn game_seats_for_game(
        &self,
        game_id: uuid::Uuid,
    ) -> Result<Vec<crate::auth::game_seats::SeatRow>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT game_id, seat_index, player_id, user_id, anon_user_id, is_bot \
                 FROM game_seats WHERE game_id = ?1 ORDER BY seat_index",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![game_id.to_string()], seat_row)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// All four seats of a game with their display identity, ordered by seat.
    /// Returns `(seat_index, username, is_bot)`; `username` is `None` for bot
    /// and guest seats, so callers pick a fallback label.
    pub fn game_players_for_game(
        &self,
        game_id: uuid::Uuid,
    ) -> Result<Vec<(i32, Option<String>, bool)>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT gs.seat_index, u.username, gs.is_bot \
                 FROM game_seats gs \
                 LEFT JOIN users u ON u.id = gs.user_id \
                 WHERE gs.game_id = ?1 ORDER BY gs.seat_index",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![game_id.to_string()], |r| {
                Ok((
                    r.get::<_, i32>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn game_seats_for_user(
        &self,
        user_id: uuid::Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::auth::game_seats::SeatRow>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT game_id, seat_index, player_id, user_id, anon_user_id, is_bot \
             FROM game_seats WHERE user_id = ?1 \
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                rusqlite::params![user_id.to_string(), limit, offset],
                seat_row,
            )
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Profile games for a user: their own seat in each game plus the stamped
    /// outcome and a peek at the live game state. `live_state` is
    /// `json_extract(games.data, '$.state')` — present only while the game row
    /// survives (it's pruned once terminal), so the caller can tell an
    /// in-progress game (live row, no stamped `result`) from an old finished
    /// one (no row, no result). Pure SQL: the game blob is never deserialized.
    pub fn profile_games_for_user(
        &self,
        user_id: uuid::Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ProfileGameRow>, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT gs.game_id, gs.seat_index, gs.player_id, \
                        gs.result, gs.team_score, gs.opp_score, \
                        json_extract(g.data, '$.state') \
                 FROM game_seats gs \
                 LEFT JOIN games g ON g.id = gs.game_id \
                 WHERE gs.user_id = ?1 \
                 ORDER BY gs.created_at DESC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![user_id.to_string(), limit, offset], |r| {
                let game_id: String = r.get(0)?;
                let player_id: String = r.get(2)?;
                Ok(ProfileGameRow {
                    game_id: Uuid::parse_str(&game_id).unwrap_or_default(),
                    seat_index: r.get(1)?,
                    player_id: Uuid::parse_str(&player_id).unwrap_or_default(),
                    result: r.get(3)?,
                    team_score: r.get(4)?,
                    opp_score: r.get(5)?,
                    live_state: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Stamp the terminal outcome onto all four seats of a game, each from its
    /// own team's perspective (seats 0 & 2 are team A, 1 & 3 are team B).
    /// `aborted` forces every seat's result to `"aborted"`; otherwise it's
    /// `won` / `lost` / `tied` by comparing the seat's team score to the
    /// opponents'. Idempotent — safe to call more than once for a game.
    pub fn record_game_results(
        &self,
        game_id: uuid::Uuid,
        team_a_score: i32,
        team_b_score: i32,
        aborted: bool,
    ) -> Result<(), String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE game_seats SET \
                team_score = CASE WHEN seat_index % 2 = 0 THEN ?2 ELSE ?3 END, \
                opp_score  = CASE WHEN seat_index % 2 = 0 THEN ?3 ELSE ?2 END, \
                result = CASE \
                    WHEN ?4 THEN 'aborted' \
                    WHEN (CASE WHEN seat_index % 2 = 0 THEN ?2 ELSE ?3 END) \
                       > (CASE WHEN seat_index % 2 = 0 THEN ?3 ELSE ?2 END) THEN 'won' \
                    WHEN (CASE WHEN seat_index % 2 = 0 THEN ?2 ELSE ?3 END) \
                       < (CASE WHEN seat_index % 2 = 0 THEN ?3 ELSE ?2 END) THEN 'lost' \
                    ELSE 'tied' END \
             WHERE game_id = ?1",
            rusqlite::params![game_id.to_string(), team_a_score, team_b_score, aborted],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn count_game_seats_for_user(&self, user_id: uuid::Uuid) -> Result<i64, String> {
        let conn = self.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT COUNT(*) FROM game_seats WHERE user_id = ?1",
            rusqlite::params![user_id.to_string()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())
    }

    /// Top players for a board, ranked by the conservative Glicko score
    /// `rating - RD_CONSERVATISM * rd` (descending; raw rating breaks ties).
    ///
    /// Only human seats count: the JOIN requires a non-NULL `user_id` and
    /// `is_bot = 0`, so anon seats and bot-account seats never appear.
    /// `min_games` gates on
    /// the all-time seat count. For `Month`, an extra `EXISTS` requires at
    /// least one seat in that UTC month; the score still uses the user's
    /// current rating (we keep no rating history).
    pub fn leaderboard(
        &self,
        window: crate::leaderboard::LeaderboardWindow,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<crate::leaderboard::LeaderboardRow>, String> {
        use crate::leaderboard::{
            LeaderboardRow, LeaderboardWindow, RD_CONSERVATISM, month_bounds,
        };
        let conn = self.pool.get().map_err(|e| e.to_string())?;

        let (month_clause, bounds) = match window {
            LeaderboardWindow::AllTime => (String::new(), None),
            LeaderboardWindow::Month { year, month } => (
                "AND EXISTS (SELECT 1 FROM game_seats m \
                 WHERE m.user_id = u.id AND m.is_bot = 0 \
                 AND m.created_at >= ?2 AND m.created_at < ?3)"
                    .to_string(),
                Some(month_bounds(year, month)),
            ),
        };

        // RD_CONSERVATISM and limit are compile-time numerics, not user
        // input, so interpolating them into the SQL is safe.
        let sql = format!(
            "SELECT u.username, u.rating, u.rd, COUNT(*) AS games \
             FROM users u \
             JOIN game_seats gs ON gs.user_id = u.id AND gs.is_bot = 0 \
             WHERE 1 = 1 {month_clause} \
             GROUP BY u.id \
             HAVING games >= ?1 \
             ORDER BY (u.rating - {RD_CONSERVATISM} * u.rd) DESC, u.rating DESC \
             LIMIT {limit}"
        );

        let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<LeaderboardRow> {
            let rating: f64 = r.get(1)?;
            let rd: f64 = r.get(2)?;
            Ok(LeaderboardRow {
                username: r.get(0)?,
                rating,
                rd,
                games_played: r.get(3)?,
                score: rating - RD_CONSERVATISM * rd,
            })
        };

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = match &bounds {
            None => stmt
                .query_map(rusqlite::params![min_games], map_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>(),
            Some((start, end)) => stmt
                .query_map(rusqlite::params![min_games, start, end], map_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>(),
        };
        rows.map_err(|e| e.to_string())
    }

    /// Run `f` inside a SQLite transaction. The closure receives the
    /// connection — call the `*_in` free functions in this module to perform
    /// individual writes. Commits on `Ok`; rolls back on `Err` (or any panic
    /// — the `Transaction` drops without commit). Returns the closure's
    /// result on success.
    pub fn with_tx<R>(
        &self,
        f: impl FnOnce(&Connection) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut conn = self.pool.get().map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        // `&tx` derefs to `&Connection`, so `_in` helpers accept it directly.
        let r = f(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(r)
    }
}

// ---------- transaction-scoped write helpers ----------
//
// These free functions hold no lock — they execute against the supplied
// connection (which may be a plain `Connection` or a `Transaction` that
// derefs to one). Use them via `SqliteStore::with_tx` to compose multi-step
// writes atomically; the public methods above are thin wrappers that lock
// the store's mutex and delegate here.

pub(crate) fn insert_user_in(
    conn: &Connection,
    new: &crate::auth::users::NewUser,
) -> Result<uuid::Uuid, String> {
    use crate::auth::users::canonicalize_username;
    let id = uuid::Uuid::new_v4();
    let canon = canonicalize_username(&new.username);
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

pub(crate) fn find_user_by_id_in(
    conn: &Connection,
    id: uuid::Uuid,
) -> Result<Option<crate::auth::users::User>, String> {
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

pub(crate) fn update_user_password_in(
    conn: &Connection,
    user_id: uuid::Uuid,
    new_hash: &str,
) -> Result<i32, String> {
    let new_version: i32 = conn
        .query_row(
            "UPDATE users SET password_hash = ?1, token_version = token_version + 1 \
         WHERE id = ?2 RETURNING token_version",
            rusqlite::params![new_hash, user_id.to_string()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(new_version)
}

pub(crate) fn set_user_email_verified_in(
    conn: &Connection,
    user_id: uuid::Uuid,
) -> Result<(), String> {
    conn.execute(
        "UPDATE users SET email_verified = 1 WHERE id = ?1",
        rusqlite::params![user_id.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn touch_user_login_in(conn: &Connection, user_id: uuid::Uuid) -> Result<(), String> {
    conn.execute(
        "UPDATE users SET last_login_at = datetime('now') WHERE id = ?1",
        rusqlite::params![user_id.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn insert_oauth_account_in(
    conn: &Connection,
    provider: &str,
    provider_uid: &str,
    user_id: uuid::Uuid,
    email: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO oauth_accounts (provider, provider_uid, user_id, email) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![provider, provider_uid, user_id.to_string(), email],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn claim_anon_game_seats_in(
    conn: &Connection,
    anon_id: uuid::Uuid,
    user_id: uuid::Uuid,
) -> Result<usize, String> {
    let n = conn
        .execute(
            "UPDATE game_seats SET user_id = ?1 WHERE anon_user_id = ?2 AND user_id IS NULL",
            rusqlite::params![user_id.to_string(), anon_id.to_string()],
        )
        .map_err(|e| e.to_string())?;
    Ok(n)
}

pub(crate) fn insert_auth_token_in(
    conn: &Connection,
    token_hash: &str,
    user_id: uuid::Uuid,
    purpose: &str,
    ttl_secs: i64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at) \
         VALUES (?1, ?2, ?3, datetime('now', ?4))",
        rusqlite::params![
            token_hash,
            user_id.to_string(),
            purpose,
            format!("+{ttl_secs} seconds")
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn clear_login_failures_in(
    conn: &Connection,
    user_id: uuid::Uuid,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM login_failures WHERE user_id = ?1",
        rusqlite::params![user_id.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn consume_auth_token_in(
    conn: &Connection,
    token_hash: &str,
    expected_purpose: &str,
) -> Result<crate::auth::tokens::ConsumedToken, String> {
    let row: Option<(String, String, String, Option<String>)> = conn
        .query_row(
            "SELECT user_id, purpose, expires_at, used_at FROM auth_tokens WHERE token_hash = ?1",
            rusqlite::params![token_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })?;
    let Some((user_id_s, purpose, expires_at, used_at)) = row else {
        return Err("token_invalid".into());
    };
    if used_at.is_some() {
        return Err("token_invalid".into());
    }
    if purpose != expected_purpose {
        return Err("token_invalid".into());
    }
    let now = chrono::Utc::now().naive_utc();
    if let Ok(when) = chrono::NaiveDateTime::parse_from_str(&expires_at, "%Y-%m-%d %H:%M:%S") {
        if when < now {
            return Err("token_invalid".into());
        }
    }
    conn.execute(
        "UPDATE auth_tokens SET used_at = datetime('now') WHERE token_hash = ?1",
        rusqlite::params![token_hash],
    )
    .map_err(|e| e.to_string())?;
    let user_id = uuid::Uuid::parse_str(&user_id_s).map_err(|e| e.to_string())?;
    Ok(crate::auth::tokens::ConsumedToken { user_id, purpose })
}

/// Parse a TEXT column as a UUID, surfacing corruption as a column
/// conversion error. Never panic and never substitute a value: the old
/// `unwrap_or_default()` turned a corrupt row into the nil UUID, which can
/// silently associate data with the wrong record.
fn uuid_col(idx: usize, s: &str) -> rusqlite::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn seat_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::auth::game_seats::SeatRow> {
    let game_id_s: String = r.get(0)?;
    let player_id_s: String = r.get(2)?;
    let user_id_s: Option<String> = r.get(3)?;
    let anon_id_s: Option<String> = r.get(4)?;
    Ok(crate::auth::game_seats::SeatRow {
        game_id: uuid_col(0, &game_id_s)?,
        seat_index: r.get(1)?,
        player_id: uuid_col(2, &player_id_s)?,
        user_id: user_id_s.as_deref().map(|s| uuid_col(3, s)).transpose()?,
        anon_user_id: anon_id_s.as_deref().map(|s| uuid_col(4, s)).transpose()?,
        is_bot: r.get::<_, i32>(5)? != 0,
    })
}

fn row_to_user(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::auth::users::User> {
    let id_s: String = r.get(0)?;
    let id = uuid_col(0, &id_s)?;
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
            [
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
            ],
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
    fn prune_stale_games_removes_terminal_and_runaway_rows() {
        // Startup must not deserialize finished games (disposable) or
        // never-terminating runaway games (the OOM): both are SQL-deleted
        // before `load_all_games` ever pulls them into memory.
        let store = SqliteStore::open(":memory:").unwrap();

        // Live, small game — must survive.
        let live = make_game();
        let live_id = *live.get_id();
        store.insert_game(&live).unwrap();

        // Terminal (aborted) game — disposable, must be pruned.
        let mut dead = make_game();
        dead.play(spades::GameTransition::Abort).unwrap();
        store.insert_game(&dead).unwrap();

        // Live but runaway: hands_played inflated past the cap. Crafting it
        // via serde avoids playing 1000+ tricks; it deserializes as a
        // NotStarted game whose history is implausibly long.
        let mut v = serde_json::to_value(make_game()).unwrap();
        let empty_trick = serde_json::json!([null, null, null, null]);
        v["hands_played"] = serde_json::Value::Array(vec![empty_trick; 1001]);
        let runaway: Game = serde_json::from_value(v).unwrap();
        store.insert_game(&runaway).unwrap();

        assert_eq!(store.load_all_games().unwrap().len(), 3);

        let pruned = store.prune_stale_games(1000).unwrap();
        assert_eq!(pruned, 2, "terminal + runaway rows are deleted");

        let remaining = store.load_all_games().unwrap();
        assert_eq!(remaining.len(), 1, "only the live, in-bounds game survives");
        assert_eq!(*remaining[0].get_id(), live_id);
    }

    #[test]
    fn load_all_games_skips_corrupt_rows() {
        // A single unreadable `games` blob (partial/legacy JSON that no longer
        // deserializes to a full `Game`) must NOT take down startup. The bad
        // row is skipped; every readable game still loads.
        let store = SqliteStore::open(":memory:").unwrap();

        let good = make_game();
        let good_id = *good.get_id();
        store.insert_game(&good).unwrap();

        // Hand-seed a corrupt row: valid JSON, but missing required fields.
        store
            .pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO games (id, data) VALUES (?1, ?2)",
                rusqlite::params!["corrupt-row", r#"{"hands_played": []}"#],
            )
            .unwrap();

        let loaded = store.load_all_games().unwrap();
        assert_eq!(loaded.len(), 1, "the readable game survives the bad row");
        assert_eq!(*loaded[0].get_id(), good_id);
    }

    #[test]
    fn pragmas_applied_on_open() {
        // Note: journal_mode=WAL is set in `open`, but `:memory:` databases
        // silently fall back to `memory` mode (WAL needs file backing). We
        // verify the file-agnostic pragmas here; journal_mode WAL is exercised
        // implicitly by every real-disk run.
        let store = SqliteStore::open(":memory:").unwrap();
        let conn = store.pool.get().unwrap();
        let busy: i64 = conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        // synchronous returns the enum value: OFF=0, NORMAL=1, FULL=2, EXTRA=3
        let sync: i64 = conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 5000);
        assert_eq!(sync, 1);
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
        let conn = store.pool.get().unwrap();
        for table in [
            "users",
            "oauth_accounts",
            "auth_tokens",
            "login_failures",
            "game_seats",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {} not created", table);
        }
    }

    #[test]
    fn users_username_canon_is_unique() {
        let store = SqliteStore::open(":memory:").unwrap();
        let conn = store.pool.get().unwrap();
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
        let id = store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();
        let u = store.find_user_by_id(id).unwrap().unwrap();
        assert_eq!(u.username, "Alice");
        assert_eq!(u.username_canon, "alice");
        assert!(!u.email_verified);
        assert_eq!(u.token_version, 0);
    }

    #[test]
    fn find_by_email_and_username_works() {
        let store = SqliteStore::open(":memory:").unwrap();
        store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();
        let by_email = store.find_user_by_email("alice@x.com").unwrap().unwrap();
        let by_username = store.find_user_by_username("ALICE").unwrap().unwrap();
        assert_eq!(by_email.id, by_username.id);
    }

    #[test]
    fn duplicate_username_rejected() {
        let store = SqliteStore::open(":memory:").unwrap();
        store.insert_user(&new_user("Alice", "a1@x.com")).unwrap();
        let err = store
            .insert_user(&new_user("alice", "a2@x.com"))
            .unwrap_err();
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
        let id = store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();
        let v1 = store.update_user_password(id, "$argon2id$new").unwrap();
        let v2 = store.update_user_password(id, "$argon2id$newer").unwrap();
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
    }

    #[test]
    fn email_verify_and_touch_login() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();
        store.set_user_email_verified(id).unwrap();
        store.touch_user_login(id).unwrap();
        let u = store.find_user_by_id(id).unwrap().unwrap();
        assert!(u.email_verified);
        assert!(u.last_login_at.is_some());
    }

    #[test]
    fn login_failure_count_resets_after_lockout_expires() {
        let store = SqliteStore::open(":memory:").unwrap();
        let id = store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();

        // Bump to 5 failures and lock.
        for _ in 0..5 {
            store.bump_login_failure(id).unwrap();
        }
        store.set_lockout(id, 1).unwrap();

        // Sleep past the lockout (1 second).
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Next bump should reset to 1 and clear locked_until.
        let n = store.bump_login_failure(id).unwrap();
        assert_eq!(n, 1, "failure count should reset after lockout expiry");

        let locked = store.get_lockout(id).unwrap();
        assert!(
            locked.is_none() || locked.as_deref() == Some(""),
            "locked_until should be NULL after reset"
        );
    }

    #[test]
    fn user_rating_round_trips_with_defaults() {
        let store = SqliteStore::open(":memory:").unwrap();
        let uid = store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();
        let r = store
            .get_user_rating(uid)
            .unwrap()
            .expect("freshly created user has a rating");
        assert_eq!(r.rating, 1500.0);
        assert_eq!(r.rd, 350.0);
        assert!((r.volatility - 0.06).abs() < 1e-9);

        let new = crate::ratings::Rating {
            rating: 1623.4,
            rd: 142.7,
            volatility: 0.0598,
        };
        store.set_user_rating(uid, &new).unwrap();
        let back = store.get_user_rating(uid).unwrap().unwrap();
        assert!((back.rating - new.rating).abs() < 1e-9);
        assert!((back.rd - new.rd).abs() < 1e-9);
        assert!((back.volatility - new.volatility).abs() < 1e-9);
    }

    #[test]
    fn user_rating_returns_none_for_missing_user() {
        let store = SqliteStore::open(":memory:").unwrap();
        let missing = uuid::Uuid::new_v4();
        assert!(store.get_user_rating(missing).unwrap().is_none());
    }

    #[test]
    fn with_tx_commits_on_ok() {
        let store = SqliteStore::open(":memory:").unwrap();
        let user_id = store
            .with_tx(|conn| {
                let uid = insert_user_in(conn, &new_user("Alice", "alice@x.com"))?;
                set_user_email_verified_in(conn, uid)?;
                Ok(uid)
            })
            .unwrap();
        let user = store
            .find_user_by_id(user_id)
            .unwrap()
            .expect("user persisted");
        assert!(
            user.email_verified,
            "both writes inside the tx are visible after commit"
        );
    }

    #[test]
    fn with_tx_rolls_back_on_err() {
        let store = SqliteStore::open(":memory:").unwrap();
        // Returning Err mid-transaction must roll back the earlier insert —
        // otherwise the user would be visible despite the explicit failure.
        let result: Result<uuid::Uuid, String> = store.with_tx(|conn| {
            let uid = insert_user_in(conn, &new_user("Alice", "alice@x.com"))?;
            let _ = uid;
            Err("simulated mid-tx failure".to_string())
        });
        assert!(result.is_err());
        // Username should not exist — INSERT rolled back.
        assert!(
            store.find_user_by_username("Alice").unwrap().is_none(),
            "rolled-back insert must not be visible"
        );
    }

    #[test]
    fn game_seat_crud_and_lookups() {
        // Cover the seat CRUD surface: insert → read → update owner → lookup
        // by player → list by user → count by user. All operations target
        // the per-(game, seat) row that auth-gates hand reads and chat.
        let store = SqliteStore::open(":memory:").unwrap();
        let alice = store
            .insert_user(&new_user("alice", "alice@x.com"))
            .unwrap();
        let bob = store.insert_user(&new_user("bob", "bob@x.com")).unwrap();

        let game_id = Uuid::new_v4();
        let alice_player = Uuid::new_v4();
        let bob_player = Uuid::new_v4();

        // Insert seats for both users at seats 0 and 1.
        store
            .insert_game_seat(
                game_id,
                0,
                alice_player,
                crate::auth::game_seats::SeatOwner {
                    user_id: Some(alice),
                    anon_user_id: None,
                    is_bot: false,
                },
            )
            .unwrap();
        store
            .insert_game_seat(
                game_id,
                1,
                bob_player,
                crate::auth::game_seats::SeatOwner {
                    user_id: Some(bob),
                    anon_user_id: None,
                    is_bot: false,
                },
            )
            .unwrap();

        // Lookups round-trip:
        let s = store.game_seat(game_id, 0).unwrap().unwrap();
        assert_eq!(s.seat_index, 0);
        assert_eq!(s.user_id, Some(alice));
        let s = store
            .game_seat_by_player_id(game_id, bob_player)
            .unwrap()
            .unwrap();
        assert_eq!(s.seat_index, 1);

        // Missing rows return None, not error:
        assert!(store.game_seat(game_id, 2).unwrap().is_none());
        assert!(
            store
                .game_seat_by_player_id(game_id, Uuid::new_v4())
                .unwrap()
                .is_none()
        );

        // Update Bob's seat to anon ownership (e.g., he logged out mid-game).
        let new_anon = Uuid::new_v4();
        store
            .update_game_seat_owner(
                game_id,
                1,
                crate::auth::game_seats::SeatOwner {
                    user_id: None,
                    anon_user_id: Some(new_anon),
                    is_bot: false,
                },
            )
            .unwrap();
        let s = store.game_seat(game_id, 1).unwrap().unwrap();
        assert_eq!(s.user_id, None);
        assert_eq!(s.anon_user_id, Some(new_anon));

        // Per-user pagination + count:
        assert_eq!(store.count_game_seats_for_user(alice).unwrap(), 1);
        assert_eq!(
            store.count_game_seats_for_user(bob).unwrap(),
            0,
            "bob's seat ownership was transferred away"
        );
        let alice_seats = store.game_seats_for_user(alice, 10, 0).unwrap();
        assert_eq!(alice_seats.len(), 1);
        assert_eq!(alice_seats[0].game_id, game_id);
        // Offset past the end is empty.
        assert!(store.game_seats_for_user(alice, 10, 5).unwrap().is_empty());
    }

    #[test]
    fn game_seats_for_game_returns_all_seats_in_index_order() {
        // The transition/delete authz paths previously issued one query per
        // seat (0..4). `game_seats_for_game` returns the whole table in a
        // single round-trip, ordered by seat_index so callers can index
        // directly.
        let store = SqliteStore::open(":memory:").unwrap();
        let game_id = Uuid::new_v4();
        let other_game = Uuid::new_v4();
        let players = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];

        // Insert out of order to prove ordering is by seat_index, not insert order.
        for &i in &[2usize, 0, 3, 1] {
            store
                .insert_game_seat(
                    game_id,
                    i as i32,
                    players[i],
                    crate::auth::game_seats::SeatOwner {
                        user_id: None,
                        anon_user_id: Some(Uuid::new_v4()),
                        is_bot: false,
                    },
                )
                .unwrap();
        }
        // A seat in a different game must not leak into the result.
        store
            .insert_game_seat(
                other_game,
                0,
                Uuid::new_v4(),
                crate::auth::game_seats::SeatOwner {
                    user_id: None,
                    anon_user_id: Some(Uuid::new_v4()),
                    is_bot: false,
                },
            )
            .unwrap();

        let seats = store.game_seats_for_game(game_id).unwrap();
        assert_eq!(seats.len(), 4, "only this game's seats are returned");
        for (i, seat) in seats.iter().enumerate() {
            assert_eq!(seat.seat_index, i as i32, "seats are ordered by index");
            assert_eq!(seat.player_id, players[i]);
        }

        // A game with no seats yields an empty Vec, not an error.
        assert!(
            store
                .game_seats_for_game(Uuid::new_v4())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn oauth_account_insert_and_find() {
        let store = SqliteStore::open(":memory:").unwrap();
        let alice = store
            .insert_user(&new_user("alice", "alice@x.com"))
            .unwrap();

        // No row before insertion.
        assert!(
            store
                .find_oauth_account("google", "google-uid-123")
                .unwrap()
                .is_none()
        );

        store
            .insert_oauth_account("google", "google-uid-123", alice, "alice@x.com")
            .unwrap();

        // Round-trip works.
        let found = store
            .find_oauth_account("google", "google-uid-123")
            .unwrap();
        assert_eq!(found, Some(alice));

        // Unknown provider/uid still returns None.
        assert!(
            store
                .find_oauth_account("github", "google-uid-123")
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .find_oauth_account("google", "other-uid")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn cleanup_expired_tokens_removes_expired_and_used_keeps_valid() {
        // Three rows: one valid (1 hr in future, unused), one backdated to
        // be expired, one consumed. Sweep removes the latter two and leaves
        // the valid one untouched.
        let store = SqliteStore::open(":memory:").unwrap();
        let alice = store
            .insert_user(&new_user("alice", "alice@x.com"))
            .unwrap();

        store
            .insert_auth_token("hash-valid", alice, "verify_email", 3600)
            .unwrap();
        store
            .insert_auth_token("hash-expired", alice, "verify_email", 3600)
            .unwrap();
        store
            .insert_auth_token("hash-consumed", alice, "verify_email", 3600)
            .unwrap();
        // Backdate `hash-expired` by direct SQL — the public API only
        // accepts non-negative TTLs.
        store.with_tx(|conn| {
            conn.execute(
                "UPDATE auth_tokens SET expires_at = datetime('now', '-1 hour') WHERE token_hash = 'hash-expired'",
                [],
            ).map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();
        store
            .consume_auth_token("hash-consumed", "verify_email")
            .unwrap();

        let removed = store.cleanup_expired_tokens().unwrap();
        assert_eq!(removed, 2, "sweep removes expired + consumed");

        // Consuming an already-swept token errors out (no row found).
        assert!(
            store
                .consume_auth_token("hash-expired", "verify_email")
                .is_err()
        );
        // The valid one still works.
        assert!(
            store
                .consume_auth_token("hash-valid", "verify_email")
                .is_ok()
        );
    }

    #[test]
    fn with_tx_rolls_back_on_late_constraint_violation() {
        // Realistic atomicity case from the register flow: insert succeeds,
        // a later write inside the same tx violates a constraint, and the
        // whole tx rolls back so the earlier user row is not orphaned.
        let store = SqliteStore::open(":memory:").unwrap();
        // Pre-existing user — their username will collide with the one we
        // attempt to insert inside the tx.
        let _alice = store
            .insert_user(&new_user("Alice", "alice@x.com"))
            .unwrap();

        let result: Result<(), String> = store.with_tx(|conn| {
            // First write: insert a different user, which would succeed alone.
            insert_user_in(conn, &new_user("Bob", "bob@x.com"))?;
            // Second write: attempt to insert a user whose username collides
            // with the pre-existing Alice — this fails with "username_taken".
            insert_user_in(conn, &new_user("Alice", "alice2@x.com"))?;
            Ok(())
        });
        assert_eq!(result.unwrap_err(), "username_taken");
        // Bob's insert must have been rolled back — only Alice remains.
        assert!(
            store.find_user_by_username("Bob").unwrap().is_none(),
            "first insert rolled back when second one failed"
        );
        assert!(
            store.find_user_by_username("Alice").unwrap().is_some(),
            "pre-existing user still present"
        );
    }

    /// Insert `n` distinct game-seats owned by `user` (seat 0 of `n`
    /// different games). Used to push a user past the MIN_GAMES gate.
    fn seed_seats(store: &SqliteStore, user: Uuid, n: usize) {
        for _ in 0..n {
            store
                .insert_game_seat(
                    Uuid::new_v4(),
                    0,
                    Uuid::new_v4(),
                    crate::auth::game_seats::SeatOwner {
                        user_id: Some(user),
                        anon_user_id: None,
                        is_bot: false,
                    },
                )
                .unwrap();
        }
    }

    #[test]
    fn leaderboard_ranks_by_conservative_score() {
        use crate::leaderboard::LeaderboardWindow;
        use crate::ratings::Rating;
        let store = SqliteStore::open(":memory:").unwrap();
        let high_rd = store.insert_user(&new_user("highrd", "h@x.com")).unwrap();
        let low_rd = store.insert_user(&new_user("lowrd", "l@x.com")).unwrap();
        seed_seats(&store, high_rd, 5);
        seed_seats(&store, low_rd, 5);
        // high_rd: 1800 - 2*300 = 1200
        store
            .set_user_rating(
                high_rd,
                &Rating {
                    rating: 1800.0,
                    rd: 300.0,
                    volatility: 0.06,
                },
            )
            .unwrap();
        // low_rd: 1600 - 2*50 = 1500  → ranks ABOVE high_rd despite lower raw rating
        store
            .set_user_rating(
                low_rd,
                &Rating {
                    rating: 1600.0,
                    rd: 50.0,
                    volatility: 0.06,
                },
            )
            .unwrap();

        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].username, "lowrd");
        assert_eq!(rows[1].username, "highrd");
        assert!((rows[0].score - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn leaderboard_excludes_players_below_min_games() {
        use crate::leaderboard::LeaderboardWindow;
        let store = SqliteStore::open(":memory:").unwrap();
        let vet = store.insert_user(&new_user("vet", "v@x.com")).unwrap();
        let rook = store.insert_user(&new_user("rook", "r@x.com")).unwrap();
        seed_seats(&store, vet, 5);
        seed_seats(&store, rook, 4); // below the gate
        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].username, "vet");
    }

    #[test]
    fn leaderboard_caps_at_limit() {
        use crate::leaderboard::LeaderboardWindow;
        use crate::ratings::Rating;
        let store = SqliteStore::open(":memory:").unwrap();
        for i in 0..12 {
            let u = store
                .insert_user(&new_user(&format!("p{i}"), &format!("p{i}@x.com")))
                .unwrap();
            seed_seats(&store, u, 5);
            store
                .set_user_rating(
                    u,
                    &Rating {
                        rating: 1500.0 + i as f64,
                        rd: 50.0,
                        volatility: 0.06,
                    },
                )
                .unwrap();
        }
        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 10, "top-10 cap");
        assert_eq!(rows[0].username, "p11", "highest score first");
    }

    #[test]
    fn leaderboard_excludes_anon_and_bot_seats() {
        use crate::leaderboard::LeaderboardWindow;
        let store = SqliteStore::open(":memory:").unwrap();
        let real = store.insert_user(&new_user("real", "r@x.com")).unwrap();
        seed_seats(&store, real, 5);
        // Anon seats (NULL user_id) must never surface.
        for _ in 0..5 {
            store
                .insert_game_seat(
                    Uuid::new_v4(),
                    0,
                    Uuid::new_v4(),
                    crate::auth::game_seats::SeatOwner {
                        user_id: None,
                        anon_user_id: Some(Uuid::new_v4()),
                        is_bot: false,
                    },
                )
                .unwrap();
        }
        // A bot ACCOUNT (real user_id, but is_bot seats) must also never surface.
        let bot = store
            .insert_user(&new_user("botaccount", "bot@x.com"))
            .unwrap();
        for _ in 0..5 {
            store
                .insert_game_seat(
                    Uuid::new_v4(),
                    0,
                    Uuid::new_v4(),
                    crate::auth::game_seats::SeatOwner {
                        user_id: Some(bot),
                        anon_user_id: None,
                        is_bot: true,
                    },
                )
                .unwrap();
        }
        let rows = store
            .leaderboard(LeaderboardWindow::AllTime, 5, 10)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].username, "real");
    }

    #[test]
    fn leaderboard_month_window_filters_by_activity() {
        use crate::leaderboard::LeaderboardWindow;
        use chrono::Datelike;
        let store = SqliteStore::open(":memory:").unwrap();
        let active = store.insert_user(&new_user("active", "a@x.com")).unwrap();
        let inactive = store.insert_user(&new_user("inactive", "i@x.com")).unwrap();
        seed_seats(&store, active, 5); // created now → current month
        seed_seats(&store, inactive, 5);
        // Backdate inactive's seats to January 2020.
        store
            .with_tx(|conn| {
                conn.execute(
                    "UPDATE game_seats SET created_at = '2020-01-15 12:00:00' WHERE user_id = ?1",
                    rusqlite::params![inactive.to_string()],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
            .unwrap();

        // January 2020 board: only the backdated player.
        let jan = store
            .leaderboard(
                LeaderboardWindow::Month {
                    year: 2020,
                    month: 1,
                },
                5,
                10,
            )
            .unwrap();
        assert_eq!(jan.len(), 1);
        assert_eq!(jan[0].username, "inactive");

        // Current month board: only the player active now.
        let now = chrono::Utc::now().naive_utc();
        let cur = store
            .leaderboard(
                LeaderboardWindow::Month {
                    year: now.year(),
                    month: now.month(),
                },
                5,
                10,
            )
            .unwrap();
        assert!(cur.iter().any(|r| r.username == "active"));
        assert!(!cur.iter().any(|r| r.username == "inactive"));
    }

    /// Insert a row with raw SQL, bypassing FK checks — simulating on-disk
    /// corruption, which by definition didn't respect the constraints.
    fn insert_corrupt(store: &SqliteStore, sql: &str, params: &[&dyn rusqlite::ToSql]) {
        let conn = store.pool.get().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        conn.execute(sql, params).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    }

    // A corrupt UUID column must surface as an error, never be silently
    // replaced (the old `unwrap_or_default()` turned it into the nil UUID,
    // which can associate data with the wrong record) and never panic.

    #[test]
    fn list_api_tokens_errors_on_corrupt_token_id() {
        let store = SqliteStore::open(":memory:").unwrap();
        let user_id = Uuid::new_v4();
        insert_corrupt(
            &store,
            "INSERT INTO api_tokens (id, user_id, token_hash, name) \
             VALUES ('not-a-uuid', ?1, 'hash', 'token')",
            &[&user_id.to_string()],
        );
        assert!(store.list_api_tokens(user_id).is_err());
    }

    #[test]
    fn find_oauth_account_errors_on_corrupt_user_id() {
        let store = SqliteStore::open(":memory:").unwrap();
        insert_corrupt(
            &store,
            "INSERT INTO oauth_accounts (provider, provider_uid, user_id, email) \
             VALUES ('github', 'uid-1', 'not-a-uuid', 'a@b.c')",
            &[],
        );
        assert!(store.find_oauth_account("github", "uid-1").is_err());
    }

    #[test]
    fn game_seat_errors_on_corrupt_player_id() {
        let store = SqliteStore::open(":memory:").unwrap();
        let game_id = Uuid::new_v4();
        insert_corrupt(
            &store,
            "INSERT INTO game_seats (game_id, seat_index, player_id, user_id, anon_user_id, is_bot) \
             VALUES (?1, 0, 'not-a-uuid', NULL, NULL, 0)",
            &[&game_id.to_string()],
        );
        assert!(store.game_seat(game_id, 0).is_err());
    }
}
