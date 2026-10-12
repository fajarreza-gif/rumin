use crate::db::MemoryDb;
use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingNote {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub created_at: u64,
}

pub struct SessionManager {
    db: Arc<MemoryDb>,
}

impl SessionManager {
    pub fn new(db: Arc<MemoryDb>) -> Self {
        Self { db }
    }

    pub fn create_session(&self, name: &str, metadata: Option<serde_json::Value>) -> Result<Session> {
        let conn = self.db.conn();
        let id = format!("sess_{}", &Uuid::new_v4().to_string()[..8]);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = metadata.unwrap_or_else(|| serde_json::json!({}));
        let meta_str = meta.to_string();

        conn.execute(
            "INSERT INTO sessions (id, name, status, created_at, updated_at, metadata)
             VALUES (?1, ?2, 'active', ?3, ?4, ?5)",
            params![id, name, now, now, meta_str],
        )?;

        Ok(Session {
            id,
            name: name.to_string(),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
            metadata: meta,
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, status, created_at, updated_at, metadata
             FROM sessions ORDER BY updated_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let status: String = row.get(2)?;
            let created_at: u64 = row.get(3)?;
            let updated_at: u64 = row.get(4)?;
            let meta_str: String = row.get(5)?;
            let metadata = serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({}));

            Ok(Session {
                id,
                name,
                status,
                created_at,
                updated_at,
                metadata,
            })
        })?;

        let mut sessions = Vec::new();
        for r in rows {
            sessions.push(r?);
        }
        Ok(sessions)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, status, created_at, updated_at, metadata
             FROM sessions WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map([session_id], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let status: String = row.get(2)?;
            let created_at: u64 = row.get(3)?;
            let updated_at: u64 = row.get(4)?;
            let meta_str: String = row.get(5)?;
            let metadata = serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({}));

            Ok(Session {
                id,
                name,
                status,
                created_at,
                updated_at,
                metadata,
            })
        })?;

        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    pub fn add_working_note(&self, session_id: &str, content: &str) -> Result<WorkingNote> {
        let conn = self.db.conn();
        let id = format!("note_{}", &Uuid::new_v4().to_string()[..8]);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        conn.execute(
            "INSERT INTO working_notes (id, session_id, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, session_id, content, now],
        )?;

        // Update session updated_at
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;

        Ok(WorkingNote {
            id,
            session_id: session_id.to_string(),
            content: content.to_string(),
            created_at: now,
        })
    }

    pub fn get_working_notes(&self, session_id: &str) -> Result<Vec<WorkingNote>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, content, created_at
             FROM working_notes WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([session_id], |row| {
            Ok(WorkingNote {
                id: row.get(0)?,
                session_id: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut notes = Vec::new();
        for r in rows {
            notes.push(r?);
        }
        Ok(notes)
    }
}
