//! SQLite access: a single serialized connection behind a mutex. The couple
//! app's write volume is tiny, so one connection keeps transactional
//! semantics trivially correct (see ADR-0091).

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

pub const SCHEMA_VERSION: i64 = 1;

// `meta` is created before migrations run so the version row exists.
const MIGRATION_V1: &str = r#"
CREATE TABLE users (
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE auth_tokens (
  token_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  revoked_at INTEGER
);
CREATE INDEX idx_tokens_user ON auth_tokens(user_id);
CREATE TABLE relations (
  id TEXT PRIMARY KEY,
  user_a TEXT NOT NULL REFERENCES users(id),
  user_b TEXT NOT NULL REFERENCES users(id),
  status TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_interaction_at INTEGER NOT NULL,
  dissolved_at INTEGER
);
CREATE INDEX idx_relations_members ON relations(user_a, user_b);
CREATE TABLE invites (
  code TEXT PRIMARY KEY,
  created_by TEXT NOT NULL REFERENCES users(id),
  expires_at INTEGER NOT NULL,
  used_at INTEGER,
  used_by TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_invites_creator ON invites(created_by, created_at);
CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  sender_id TEXT NOT NULL,
  body TEXT NOT NULL,
  client_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (relation_id, client_id),
  UNIQUE (relation_id, seq)
);
CREATE INDEX idx_messages_relation ON messages(relation_id, seq);
CREATE TABLE message_reads (
  relation_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  last_message_seq INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (relation_id, user_id)
);
CREATE TABLE letters (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  sender_id TEXT NOT NULL,
  body TEXT NOT NULL,
  client_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  read_at INTEGER,
  UNIQUE (relation_id, client_id)
);
CREATE TABLE pokes (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  sender_id TEXT NOT NULL,
  client_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (relation_id, client_id)
);
CREATE INDEX idx_pokes_created ON pokes(relation_id, created_at);
CREATE TABLE focus_sessions (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  initiator_id TEXT NOT NULL,
  client_id TEXT NOT NULL,
  topic TEXT,
  duration_secs INTEGER NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  invited_at INTEGER NOT NULL,
  accepted_at INTEGER,
  started_at INTEGER,
  end_at INTEGER,
  paused_ms INTEGER NOT NULL DEFAULT 0,
  paused_at INTEGER,
  ended_at INTEGER,
  ended_early INTEGER NOT NULL DEFAULT 0,
  UNIQUE (relation_id, client_id)
);
CREATE INDEX idx_focus_relation ON focus_sessions(relation_id, state);
CREATE TABLE todos (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  client_id TEXT NOT NULL,
  creator_id TEXT NOT NULL,
  title TEXT NOT NULL,
  detail TEXT,
  priority TEXT NOT NULL,
  due_at INTEGER,
  completed_at INTEGER,
  completed_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER,
  UNIQUE (relation_id, client_id)
);
CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  client_id TEXT NOT NULL,
  author_id TEXT NOT NULL,
  content TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1,
  updated_by TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  deleted_at INTEGER,
  UNIQUE (relation_id, client_id)
);
CREATE TABLE anniversaries (
  id TEXT PRIMARY KEY,
  relation_id TEXT NOT NULL,
  client_id TEXT NOT NULL,
  creator_id TEXT NOT NULL,
  name TEXT NOT NULL,
  date TEXT NOT NULL,
  repeat_yearly INTEGER NOT NULL,
  note TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER,
  UNIQUE (relation_id, client_id)
);
CREATE TABLE events (
  seq INTEGER NOT NULL,
  id TEXT NOT NULL UNIQUE,
  relation_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  server_time INTEGER NOT NULL,
  version INTEGER NOT NULL,
  payload TEXT NOT NULL,
  PRIMARY KEY (relation_id, seq)
);
CREATE TABLE presence (
  user_id TEXT PRIMARY KEY,
  online INTEGER NOT NULL,
  state TEXT NOT NULL,
  last_seen_at INTEGER NOT NULL,
  focus_session_id TEXT
);
"#;

#[derive(Clone)]
pub struct Db {
    // Debug is derived on the Arc only; the connection itself never logs
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Db(sqlite)")
    }
}

impl Db {
    /// Opens (creating directories as needed) and migrates the database.
    pub fn open(path: &Path, wal: bool) -> Result<Self, String> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| format!("create db directory: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open db: {e}"))?;
        if wal {
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(|e| format!("set WAL: {e}"))?;
        }
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(|e| format!("set synchronous: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("set foreign_keys: {e}"))?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("open memory db: {e}"))?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .map_err(|e| e.to_string())?;
        let current: i64 = tx
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if current > SCHEMA_VERSION {
            return Err(format!(
                "database schema {current} is newer than this server ({SCHEMA_VERSION})"
            ));
        }
        if current == 0 {
            tx.execute_batch(MIGRATION_V1)
                .map_err(|e| format!("migrate v1: {e}"))?;
            tx.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                [SCHEMA_VERSION.to_string()],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// Serial access guard. Handlers must not hold it across await points.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_migrates_and_records_version() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock();
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION.to_string());
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for expected in [
            "users",
            "relations",
            "messages",
            "letters",
            "pokes",
            "focus_sessions",
            "todos",
            "notes",
            "anniversaries",
            "events",
            "presence",
        ] {
            assert!(tables.iter().any(|t| t == expected), "missing {expected}");
        }
    }

    #[test]
    fn future_schema_is_rejected_not_silently_used() {
        let dir = std::env::temp_dir().join(format!("insync-future-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future.db");
        let db = Db::open(&path, false).unwrap();
        {
            let conn = db.lock();
            conn.execute(
                "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        }
        drop(db);
        let err = Db::open(&path, false).unwrap_err();
        assert!(err.contains("newer"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
