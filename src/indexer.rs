use crate::graph::CodeGraph;
use crate::sidecar::SidecarClient;
use crate::parsers;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use walkdir::WalkDir;

pub struct Indexer {
    repo_root: PathBuf,
}

impl Indexer {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    #[inline]
    pub fn is_binary_file(path: &Path) -> bool {
        parsers::is_binary_file(path)
    }

    #[inline]
    pub fn detect_shebang(header: &[u8]) -> Option<&'static str> {
        parsers::detect_shebang(header)
    }

    #[inline]
    pub fn parse_makefile(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_makefile(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_typescript(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_typescript(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_go(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_go(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_c(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_c(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_cpp(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_cpp(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_java(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_java(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_xml(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_xml(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_html(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_html(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_plsql(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_plsql(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_apex_class(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_apex_class(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_apex_trigger(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_apex_trigger(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_salesforce_metadata(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_salesforce_metadata(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_soql(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_soql(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_aura(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_aura(rel_path, content, graph);
    }

    #[inline]
    pub fn count_erlang_args(args_str: &str) -> usize {
        parsers::count_erlang_args(args_str)
    }

    #[inline]
    pub fn parse_erlang_module(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_erlang_module(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_erlang_app(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_erlang_app(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_rebar_config(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_rebar_config(rel_path, content, graph);
    }

    #[inline]
    pub fn is_apigee_file(path: &Path, content: &str) -> bool {
        parsers::is_apigee_file(path, content)
    }

    #[inline]
    pub fn parse_apigee(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_apigee(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_rust(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_rust(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_python(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_python(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_generic_source(rel_path: &str, content: &str, lang: &str, graph: &mut CodeGraph) {
        parsers::parse_generic_source(rel_path, content, lang, graph);
    }

    /// Index a single file, routing to the appropriate language parser or sidecar.
    pub async fn index_single_file(
        repo_root: &Path,
        path: &Path,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
    ) -> Result<bool> {
        if !path.is_file() || Self::is_binary_file(path) {
            return Ok(false);
        }

        let rel_path = match path.strip_prefix(repo_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => path.to_string_lossy().to_string(),
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // 1. Check for Makefiles
        if file_name == "Makefile" || file_name == "GNUmakefile" || extension == "mk" {
            Self::parse_makefile(&rel_path, &content, graph);
            return Ok(true);
        }

        // 2. Check for SQL & PL/SQL files
        if matches!(extension, "sql" | "pls" | "pks" | "pkb" | "prc" | "fnc" | "trg") {
            let dialect = if matches!(extension, "pls" | "pks" | "pkb" | "prc" | "fnc" | "trg") {
                Some("oracle")
            } else {
                None
            };
            match sidecar.parse_sql(&rel_path, &content, dialect).await {
                Ok(entities) => {
                    for ent in entities {
                        graph.add_entity(ent);
                    }
                }
                Err(err) => {
                    debug!("Sidecar SQL parse fallback for {}: {}", rel_path, err);
                }
            }
            return Ok(true);
        }

        // 3. Shell scripts
        let is_shell_ext = matches!(extension, "sh" | "bash" | "zsh");
        let is_shell_shebang = Self::detect_shebang(content.as_bytes()) == Some("bash");
        if is_shell_ext || is_shell_shebang {
            match sidecar.parse_shell(&rel_path, &content).await {
                Ok(entities) => {
                    for ent in entities {
                        graph.add_entity(ent);
                    }
                }
                Err(err) => {
                    debug!("Sidecar Shell parse fallback for {}: {}", rel_path, err);
                }
            }
            return Ok(true);
        }

        // 4. Standard programming languages & markup formats
        let shebang = Self::detect_shebang(content.as_bytes());
        if matches!(extension, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") || shebang == Some("typescript") {
            Self::parse_typescript(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "go") {
            Self::parse_go(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "c" | "h") {
            Self::parse_c(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "cpp" | "cc" | "cxx" | "hpp" | "hxx") {
            Self::parse_cpp(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "java") {
            Self::parse_java(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "cls") {
            Self::parse_apex_class(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "trigger") {
            Self::parse_apex_trigger(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".object-meta.xml") || file_name.ends_with(".field-meta.xml") || matches!(extension, "object" | "field") {
            Self::parse_salesforce_metadata(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "soql") {
            Self::parse_soql(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "erl" | "hrl" | "escript") || shebang == Some("erlang") {
            Self::parse_erlang_module(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".app.src") {
            Self::parse_erlang_app(&rel_path, &content, graph);
            Ok(true)
        } else if file_name == "rebar.config" {
            Self::parse_rebar_config(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "cmp" | "evt" | "intf") {
            Self::parse_aura(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "app") {
            if content.trim_start().starts_with('<') {
                Self::parse_aura(&rel_path, &content, graph);
            } else {
                Self::parse_erlang_app(&rel_path, &content, graph);
            }
            Ok(true)
        } else if matches!(extension, "xml" | "xsd" | "pom") {
            if Self::is_apigee_file(path, &content) {
                Self::parse_apigee(&rel_path, &content, graph);
            } else {
                Self::parse_xml(&rel_path, &content, graph);
            }
            Ok(true)
        } else if matches!(extension, "html" | "htm") {
            Self::parse_html(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "rs") {
            Self::parse_rust(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "py") || shebang == Some("python") {
            Self::parse_python(&rel_path, &content, graph);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Index the entire repository, coordinating native parsing and the Python sidecar.
    pub async fn index_repository(&self, graph: &mut CodeGraph, sidecar: &SidecarClient) -> Result<usize> {
        let stats = self.sync_repository(graph, sidecar, true).await?;
        Ok(stats.added + stats.modified)
    }

    /// Compute file system changes (additions, modifications, deletions) completely decoupled
    /// from in-memory graph mutations. Supports cooperative cancellation.
    pub fn compute_delta(
        &self,
        manifest: &FileManifest,
        force: bool,
        cancel: Option<&CancellationToken>,
    ) -> Result<SyncDelta> {
        let mut delta = SyncDelta::default();
        let mut seen_paths: HashSet<String> = HashSet::new();

        let walker = WalkDir::new(&self.repo_root).into_iter().filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | "venv" | ".venv" | "__pycache__" | ".aimem" | ".cargo" | ".rumin" | "build" | "dist" | "out" | "vendor" | ".next" | "coverage" | ".pytest_cache" | ".mypy_cache" | ".tox" | "tmp" | ".tmp"
            )
        });

        for entry in walker.filter_map(|e| e.ok()) {
            if let Some(c) = cancel {
                if c.is_cancelled() {
                    bail!("Indexing cancelled by token");
                }
            }

            let path = entry.path();
            if !path.is_file() || Self::is_binary_file(path) {
                continue;
            }

            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let file_size = metadata.len();
            // Skip giant files >10MB to avoid OOM
            if file_size > 10 * 1024 * 1024 {
                continue;
            }

            let rel_path = match path.strip_prefix(&self.repo_root) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => path.to_string_lossy().to_string(),
            };
            seen_paths.insert(rel_path.clone());

            let mtime_secs = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let mtime_nanos = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);

            // Tier 1 Fast-path: Check metadata (size + mtime)
            if !force {
                if let Some(old_record) = manifest.files.get(&rel_path) {
                    let old_mtime = old_record.mtime_nanos();
                    let matches_mtime = if old_mtime > 100_000_000_000 {
                        old_mtime == mtime_nanos
                    } else {
                        old_mtime == mtime_secs
                    };

                    let matches_size = match old_record.size() {
                        Some(sz) => sz == file_size,
                        None => true, // legacy manifest without size
                    };

                    if matches_mtime && matches_size {
                        delta.unchanged += 1;
                        continue;
                    }
                }
            }

            // Tier 2: Read content and compute hash
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let hash = {
                let mut hasher = Sha256::new();
                hasher.update(content.as_bytes());
                format!("{:x}", hasher.finalize())
            };

            let new_record = ManifestRecord::Full {
                mtime_nanos,
                size: file_size,
                hash: hash.clone(),
            };

            if let Some(old_record) = manifest.files.get(&rel_path) {
                let old_mtime = old_record.mtime_nanos();
                let matches_mtime = if old_mtime > 100_000_000_000 {
                    old_mtime == mtime_nanos
                } else {
                    old_mtime == mtime_secs
                };

                if !force && matches_mtime && old_record.hash() == hash {
                    delta.unchanged += 1;
                    continue;
                }

                // File modified
                delta.modified.push((rel_path, path.to_path_buf(), new_record));
            } else {
                // New file added
                delta.added.push((rel_path, path.to_path_buf(), new_record));
            }
        }

        // Deleted files
        for rel_path in manifest.files.keys() {
            if !seen_paths.contains(rel_path) {
                delta.deleted.push(rel_path.clone());
            }
        }

        Ok(delta)
    }

    /// Apply computed delta to `CodeGraph` and `FileManifest`.
    pub async fn apply_delta(
        &self,
        delta: SyncDelta,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
        manifest: &mut FileManifest,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats {
            unchanged: delta.unchanged,
            ..Default::default()
        };

        // 1. Process deletions
        for rel_path in delta.deleted {
            graph.remove_file(&rel_path);
            manifest.files.remove(&rel_path);
            stats.deleted += 1;
        }

        // 2. Process modifications
        for (rel_path, path, new_record) in delta.modified {
            graph.remove_file(&rel_path);
            if Self::index_single_file(&self.repo_root, &path, graph, sidecar).await? {
                stats.modified += 1;
            }
            manifest.files.insert(rel_path, new_record);
        }

        // 3. Process additions
        for (rel_path, path, new_record) in delta.added {
            if Self::index_single_file(&self.repo_root, &path, graph, sidecar).await? {
                stats.added += 1;
            }
            manifest.files.insert(rel_path, new_record);
        }

        Ok(stats)
    }

    /// Incremental repository synchronization using mtime and content hashes.
    pub async fn sync_repository(
        &self,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
        force: bool,
    ) -> Result<SyncStats> {
        self.sync_repository_cancellable(graph, sidecar, force, None).await
    }

    /// Incremental repository synchronization with cooperative cancellation.
    pub async fn sync_repository_cancellable(
        &self,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
        force: bool,
        cancel: Option<&CancellationToken>,
    ) -> Result<SyncStats> {
        let start = std::time::Instant::now();
        let manifest_path = self.repo_root.join(".rumin/manifest.json");
        let mut manifest = if force {
            FileManifest::default()
        } else {
            FileManifest::load_or_new(&manifest_path)
        };

        let delta = self.compute_delta(&manifest, force, cancel)?;
        let mut stats = self.apply_delta(delta, graph, sidecar, &mut manifest).await?;

        let _ = manifest.save(&manifest_path);
        let default_checkpoint = self.repo_root.join(".rumin/checkpoints/default.bin");
        if let Some(parent) = default_checkpoint.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = graph.save_binary_checkpoint(&default_checkpoint);

        stats.total_indexed = graph.node_count();
        stats.duration_ms = start.elapsed().as_millis();

        info!(
            "Sync completed in {}ms: +{} added, ~{} modified, -{} deleted, {} unchanged",
            stats.duration_ms, stats.added, stats.modified, stats.deleted, stats.unchanged
        );
        Ok(stats)
    }
}

/// Delta computed during repository scan.
#[derive(Debug, Default)]
pub struct SyncDelta {
    pub added: Vec<(String, PathBuf, ManifestRecord)>,
    pub modified: Vec<(String, PathBuf, ManifestRecord)>,
    pub deleted: Vec<String>,
    pub unchanged: usize,
}

/// Statistics returned by incremental sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub total_indexed: usize,
    pub duration_ms: u128,
}

/// Manifest record supporting both legacy format and versioned metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ManifestRecord {
    Legacy(u64, String),
    Full {
        mtime_nanos: u64,
        size: u64,
        hash: String,
    },
}

impl ManifestRecord {
    pub fn mtime_nanos(&self) -> u64 {
        match self {
            Self::Legacy(m, _) => *m,
            Self::Full { mtime_nanos, .. } => *mtime_nanos,
        }
    }

    pub fn size(&self) -> Option<u64> {
        match self {
            Self::Legacy(_, _) => None,
            Self::Full { size, .. } => Some(*size),
        }
    }

    pub fn hash(&self) -> &str {
        match self {
            Self::Legacy(_, h) => h,
            Self::Full { hash, .. } => hash,
        }
    }
}

/// On-disk manifest caching file mtimes and SHA256 hashes for incremental sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileManifest {
    #[serde(default = "default_manifest_version")]
    pub version: u32,
    pub files: HashMap<String, ManifestRecord>,
}

fn default_manifest_version() -> u32 {
    1
}

impl FileManifest {
    pub fn load_or_new(manifest_path: &Path) -> Self {
        if manifest_path.exists() {
            if let Ok(content) = std::fs::read_to_string(manifest_path) {
                if let Ok(manifest) = serde_json::from_str(&content) {
                    return manifest;
                }
            }
        }
        Self {
            version: 1,
            files: HashMap::new(),
        }
    }

    pub fn save(&self, manifest_path: &Path) -> Result<()> {
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(manifest_path, json)?;
        Ok(())
    }
}
