use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct MemoryDb {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl MemoryDb {
    pub fn open(repo_root: &Path) -> Result<Self> {
        let db_dir = repo_root.join(".rumin");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("memory.db");

        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS episodes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                success INTEGER NOT NULL,
                bound_symbols TEXT NOT NULL,
                bound_files TEXT NOT NULL,
                tags TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_episodes_title ON episodes(title);
            CREATE INDEX IF NOT EXISTS idx_episodes_created_at ON episodes(created_at);

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                metadata TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at);

            CREATE TABLE IF NOT EXISTS working_notes (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_notes_session ON working_notes(session_id);

            CREATE TABLE IF NOT EXISTS traces (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                step_index INTEGER NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_traces_session ON traces(session_id);
            "#,
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
            path: db_path,
        })
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
