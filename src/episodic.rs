use crate::db::MemoryDb;
use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};
use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub title: String,
    pub content: String,
    pub success: bool,
    pub bound_symbols: Vec<String>,
    pub bound_files: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: u64,
}

pub struct EpisodicEngine {
    db: Arc<MemoryDb>,
}

impl EpisodicEngine {
    pub fn new(db: Arc<MemoryDb>) -> Self {
        Self { db }
    }

    pub fn record_episode(
        &self,
        title: &str,
        content: &str,
        success: bool,
        bound_symbols: &[String],
        bound_files: &[String],
        tags: &[String],
    ) -> Result<Episode> {
        let conn = self.db.conn();
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS episode_seq (id INTEGER PRIMARY KEY AUTOINCREMENT)", []);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM episode_seq", [], |r| r.get(0)).unwrap_or(0);
        if count == 0 {
            let max_existing: i64 = conn.query_row(
                "SELECT COALESCE(MAX(CAST(SUBSTR(id, 4) AS INTEGER)), 0) FROM episodes WHERE id LIKE 'ep_%'",
                [],
                |r| r.get(0),
            ).unwrap_or(0);
            if max_existing > 0 {
                let _ = conn.execute("INSERT INTO episode_seq (id) VALUES (?1)", [max_existing]);
            }
        }
        conn.execute("INSERT INTO episode_seq DEFAULT VALUES", [])?;
        let seq = conn.last_insert_rowid();
        let id = format!("ep_{}", seq);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let symbols_json = serde_json::to_string(bound_symbols)?;
        let files_json = serde_json::to_string(bound_files)?;
        let tags_json = serde_json::to_string(tags)?;

        conn.execute(
            "INSERT INTO episodes (id, title, content, success, bound_symbols, bound_files, tags, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, title, content, if success { 1 } else { 0 }, symbols_json, files_json, tags_json, now],
        )?;

        Ok(Episode {
            id,
            title: title.to_string(),
            content: content.to_string(),
            success,
            bound_symbols: bound_symbols.to_vec(),
            bound_files: bound_files.to_vec(),
            tags: tags.to_vec(),
            created_at: now,
        })
    }

    pub fn list_episodes(&self, limit: usize) -> Result<Vec<Episode>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, title, content, success, bound_symbols, bound_files, tags, created_at
             FROM episodes ORDER BY created_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let content: String = row.get(2)?;
            let success_int: i32 = row.get(3)?;
            let symbols_str: String = row.get(4)?;
            let files_str: String = row.get(5)?;
            let tags_str: String = row.get(6)?;
            let created_at: u64 = row.get(7)?;

            let bound_symbols: Vec<String> = serde_json::from_str(&symbols_str).unwrap_or_default();
            let bound_files: Vec<String> = serde_json::from_str(&files_str).unwrap_or_default();
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

            Ok(Episode {
                id,
                title,
                content,
                success: success_int != 0,
                bound_symbols,
                bound_files,
                tags,
                created_at,
            })
        })?;

        let mut episodes = Vec::new();
        for r in rows {
            episodes.push(r?);
        }
        Ok(episodes)
    }

    pub fn search_episodes(&self, query: &str, limit: usize) -> Result<Vec<Episode>> {
        let q_lower = query.to_lowercase();
        let all = self.list_episodes(1000)?;

        let mut matches: Vec<(usize, Episode)> = all
            .into_iter()
            .filter_map(|ep| {
                let mut score = 0;
                let title_lower = ep.title.to_lowercase();
                let content_lower = ep.content.to_lowercase();

                for word in q_lower.split_whitespace() {
                    if title_lower.contains(word) {
                        score += 5;
                    }
                    if ep.tags.iter().any(|t| t.to_lowercase().contains(word)) {
                        score += 4;
                    }
                    if ep.bound_symbols.iter().any(|s| s.to_lowercase().contains(word)) {
                        score += 3;
                    }
                    if content_lower.contains(word) {
                        score += 1;
                    }
                }

                if score > 0 {
                    Some((score, ep))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by_key(|a| std::cmp::Reverse(a.0));
        Ok(matches.into_iter().take(limit).map(|m| m.1).collect())
    }

    /// Cross-link episodes directly into the active CodeGraph as structural nodes!
    pub fn bind_episodes_to_graph(&self, graph: &mut CodeGraph) -> Result<usize> {
        let episodes = self.list_episodes(500)?;
        let mut bound_count = 0;

        for ep in episodes {
            let ep_node_id = format!("episode::{}", ep.id);
            let mut entity = CodeEntity::new(
                &ep_node_id,
                &ep.title,
                EntityType::Rule,
                format!(".rumin/episodes/{}.md", ep.id),
                1,
                1,
            );
            entity.signature = Some(format!("Episode [{}] (Success: {})", ep.title, ep.success));
            entity.docstring = Some(ep.content.clone());

            graph.add_entity(entity);
            bound_count += 1;

            // Link to bound symbols
            for sym in &ep.bound_symbols {
                if graph.get_entity(sym).is_some() {
                    graph.add_edge(&ep_node_id, sym, EdgeKind::References);
                }
            }

            // Link to bound files
            for file in &ep.bound_files {
                if let Some(file_entities) = graph.file_to_entities.get(file).cloned() {
                    for target_sym in file_entities {
                        graph.add_edge(&ep_node_id, &target_sym, EdgeKind::References);
                    }
                }
            }
        }

        Ok(bound_count)
    }
}
