//! SQLite persistence for the hub.
//!
//! One database file holds everything: users, sessions, guilds, telescopes,
//! credentials, and updater state. Access goes through a single
//! `Arc<Mutex<Connection>>`; rusqlite is synchronous and every query here is
//! short, so a plain mutex is enough.
//!
//! Migrations are a hand-rolled list keyed off `PRAGMA user_version` rather
//! than a migration crate, to avoid coupling our rusqlite version to a
//! third-party crate's supported range (rusqlite itself must stay on the same
//! libsqlite3-sys line as matrix-sdk).

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ordered schema migrations. Entry at index `i` brings the schema to
/// `user_version = i + 1`. Append only — never edit or reorder a shipped
/// entry.
const MIGRATIONS: &[&str] = &[
    // V1: instance-level key/value settings.
    "CREATE TABLE hub_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    ) STRICT;",
    // V2: Discord login. Users, their guild snapshot from OAuth, server-side
    // sessions, and single-use OAuth state rows. Snowflakes are stored as
    // INTEGER (u64 bit-cast to i64) and serialized to JSON as strings.
    "CREATE TABLE users (
        discord_user_id INTEGER PRIMARY KEY,
        username TEXT NOT NULL,
        email TEXT,
        email_verified INTEGER NOT NULL DEFAULT 0,
        avatar_url TEXT,
        created_at INTEGER NOT NULL,
        last_auth_at INTEGER NOT NULL
    ) STRICT;
    CREATE TABLE user_guilds (
        discord_user_id INTEGER NOT NULL
            REFERENCES users(discord_user_id) ON DELETE CASCADE,
        guild_id INTEGER NOT NULL,
        guild_name TEXT NOT NULL,
        permissions INTEGER NOT NULL,
        is_owner INTEGER NOT NULL DEFAULT 0,
        last_seen_at INTEGER NOT NULL,
        PRIMARY KEY (discord_user_id, guild_id)
    ) STRICT;
    CREATE INDEX idx_user_guilds_guild ON user_guilds(guild_id);
    CREATE TABLE sessions (
        session_id TEXT PRIMARY KEY,
        csrf_token TEXT NOT NULL,
        discord_user_id INTEGER NOT NULL
            REFERENCES users(discord_user_id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    ) STRICT;
    CREATE INDEX idx_sessions_user ON sessions(discord_user_id);
    CREATE TABLE oauth_states (
        nonce TEXT PRIMARY KEY,
        pkce_verifier TEXT NOT NULL,
        next_path TEXT NOT NULL,
        issued_at INTEGER NOT NULL,
        consumed_at INTEGER
    ) STRICT;",
    // V3: tenancy. A Discord guild is a tenant; telescopes belong to a guild;
    // pairing tokens are one-time secrets exchanged for rig credentials.
    // Only the SHA-256 of a pairing token is stored.
    "CREATE TABLE guilds (
        guild_id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        registered_by INTEGER NOT NULL REFERENCES users(discord_user_id),
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    ) STRICT;
    CREATE TABLE telescopes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        guild_id INTEGER NOT NULL REFERENCES guilds(guild_id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        discord_channel_id INTEGER,
        image_cooldown_seconds INTEGER NOT NULL DEFAULT 60,
        write_policy TEXT NOT NULL DEFAULT 'disabled'
            CHECK (write_policy IN ('disabled', 'roles')),
        allowed_role_ids TEXT NOT NULL DEFAULT '[]',
        created_by INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        UNIQUE (guild_id, name)
    ) STRICT;
    CREATE INDEX idx_telescopes_guild ON telescopes(guild_id);
    CREATE TABLE pairing_tokens (
        token_hash TEXT PRIMARY KEY,
        telescope_id INTEGER NOT NULL REFERENCES telescopes(id) ON DELETE CASCADE,
        created_by INTEGER NOT NULL,
        issued_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        consumed_at INTEGER
    ) STRICT;
    CREATE INDEX idx_pairing_tokens_telescope ON pairing_tokens(telescope_id);",
];

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database mutex poisoned")]
    Poisoned,
}

/// Current unix time in seconds, for `*_at` columns.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, DbError> {
        // `PRAGMA journal_mode` returns a row (the resulting mode), so it
        // goes through query_row instead of pragma_update.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run a closure against the connection under the lock.
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, DbError> {
        let guard = self.conn.lock().map_err(|_| DbError::Poisoned)?;
        f(&guard).map_err(DbError::from)
    }

    pub fn schema_version(&self) -> Result<u32, DbError> {
        self.with_conn(|conn| conn.query_row("PRAGMA user_version", [], |r| r.get(0)))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO hub_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
                rusqlite::params![key, value, unix_now()],
            )
            .map(|_| ())
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT value FROM hub_settings WHERE key = ?1",
                rusqlite::params![key],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
    }
}

/// Apply any migrations beyond the database's current `user_version`. Each
/// migration and its version bump commit atomically, so a crash mid-migration
/// leaves the previous version intact.
fn migrate(conn: &Connection) -> Result<(), DbError> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = i + 1;
        conn.execute_batch(&format!(
            "BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_migrates_to_latest() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap() as usize, MIGRATIONS.len());
    }

    #[test]
    fn migrate_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            migrate(conn).expect("re-running migrations must be a no-op");
            Ok(())
        })
        .unwrap();
        assert_eq!(db.schema_version().unwrap() as usize, MIGRATIONS.len());
    }

    #[test]
    fn migrate_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!("chatstronomy-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);

        {
            let db = Db::open(&path).unwrap();
            db.set_setting("instance_name", "test-hub").unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap() as usize, MIGRATIONS.len());
        assert_eq!(
            db.get_setting("instance_name").unwrap().as_deref(),
            Some("test-hub")
        );

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_upsert_and_missing() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("missing").unwrap(), None);
        db.set_setting("k", "v1").unwrap();
        db.set_setting("k", "v2").unwrap();
        assert_eq!(db.get_setting("k").unwrap().as_deref(), Some("v2"));
    }

    #[test]
    fn foreign_keys_enabled() {
        let db = Db::open_in_memory().unwrap();
        let on: i64 = db
            .with_conn(|conn| conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(on, 1);
    }
}
