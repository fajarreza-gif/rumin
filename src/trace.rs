use crate::db::MemoryDb;
use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceIngestionReport {
    pub steps_ingested: usize,
    pub unique_files_touched: usize,
    pub session_id: Option<String>,
    pub duration_ms: u128,
}

pub struct TraceIngestor {
    db: Arc<MemoryDb>,
}

impl TraceIngestor {
    pub fn new(db: Arc<MemoryDb>) -> Self {
        Self { db }
    }

    pub fn ingest_file(&self, trace_path: &Path, session_id: Option<&str>) -> Result<TraceIngestionReport> {
        let start = std::time::Instant::now();
        let file = File::open(trace_path)?;
        let reader = BufReader::new(file);

        let mut steps_ingested = 0;
        let mut touched_files: HashSet<String> = HashSet::new();

        let conn = self.db.conn();
        let _ = conn.execute_batch("BEGIN TRANSACTION;");
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for (idx, line) in reader.lines().enumerate() {
            let line_str = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            if line_str.trim().is_empty() {
                continue;
            }

            // Attempt to parse line as JSON
            let (action, target) = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line_str) {
                let action = val
                    .get("type")
                    .or_else(|| val.get("action"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("step")
                    .to_string();

                let target = val
                    .get("target")
                    .or_else(|| val.get("file_path"))
                    .or_else(|| val.get("tool_action"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if !target.is_empty() {
                    touched_files.insert(target.clone());
                }

                (action, target)
            } else {
                ("generic".to_string(), line_str[..line_str.len().min(80)].to_string())
            };

            let id = format!("tr_{}", &Uuid::new_v4().to_string()[..8]);
            let _ = conn.execute(
                "INSERT INTO traces (id, session_id, step_index, action, target, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, session_id, idx as i64, action, target, now],
            );

            steps_ingested += 1;
        }

        let _ = conn.execute_batch("COMMIT;");

        let duration_ms = start.elapsed().as_millis();
        Ok(TraceIngestionReport {
            steps_ingested,
            unique_files_touched: touched_files.len(),
            session_id: session_id.map(|s| s.to_string()),
            duration_ms,
        })
    }
}
