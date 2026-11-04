use crate::config::RuminConfig;
use crate::graph::CodeGraph;
use crate::sidecar::SidecarClient;
use crate::types::{CodeEntity, EdgeKind, EntityType};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeBridge {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeclarativeBridgesConfig {
    #[serde(default)]
    pub semantic_bridges: Vec<DeclarativeBridge>,
    #[serde(default)]
    pub csv_files: Vec<String>,
}

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
    pub fn is_supported_file(file_name: &str, extension: &str) -> bool {
        Self::is_supported_file_path(Path::new(file_name), extension)
    }

    #[inline]
    pub fn is_supported_file_path(path: &Path, extension: &str) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let path_str = path.to_string_lossy();
        if file_name == "Makefile" || file_name == "GNUmakefile" || file_name == "rebar.config" {
            return true;
        }
        if parsers::is_supported_json_file(file_name) || parsers::is_omnistudio_json_path(&path_str, file_name) {
            return true;
        }
        if parsers::is_supported_yaml_file(&path_str, file_name) {
            return true;
        }
        if file_name.ends_with(".app.src") {
            return true;
        }
        if file_name.ends_with("-meta.xml") {
            return true;
        }
        if extension.is_empty() {
            return true;
        }
        matches!(
            extension,
            "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
                | "go" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx"
                | "java" | "cls" | "trigger" | "object" | "field" | "soql"
                | "erl" | "hrl" | "escript" | "cmp" | "evt" | "intf" | "app" | "page" | "component"
                | "xml" | "xsd" | "pom" | "html" | "htm"
                | "sql" | "pls" | "pks" | "pkb" | "prc" | "fnc" | "trg"
                | "sh" | "bash" | "zsh" | "mk"
                | "vb" | "vba" | "bas" | "frm" | "ctl" | "vbs"
        )
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
    pub fn is_supported_json_file(file_name: &str) -> bool {
        parsers::is_supported_json_file(file_name)
    }

    #[inline]
    pub fn is_omnistudio_json_path(rel_path: &str, file_name: &str) -> bool {
        parsers::is_omnistudio_json_path(rel_path, file_name)
    }

    #[inline]
    pub fn parse_omnistudio_json(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_omnistudio_json(rel_path, content, graph);
    }

    #[inline]
    pub fn parse_json_manifest(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_json_manifest(rel_path, content, graph);
    }

    #[inline]
    pub fn is_supported_yaml_file(rel_path: &str, file_name: &str) -> bool {
        parsers::is_supported_yaml_file(rel_path, file_name)
    }

    #[inline]
    pub fn parse_yaml_manifest(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_yaml_manifest(rel_path, content, graph);
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

    #[inline]
    pub fn is_vb_cls(content: &str) -> bool {
        parsers::is_vb_cls(content)
    }

    #[inline]
    pub fn parse_vb(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        parsers::parse_vb(rel_path, content, graph);
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

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if !Self::is_supported_file_path(path, extension) {
            return Ok(false);
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };

        // 1. Check for Makefiles
        if file_name == "Makefile" || file_name == "GNUmakefile" || extension == "mk" {
            Self::parse_makefile(&rel_path, &content, graph);
            return Ok(true);
        }

        // 2. Check for SQL & PL/SQL files
        if matches!(extension, "sql" | "pls" | "pks" | "pkb" | "prc" | "fnc" | "trg") {
            crate::parsers::parse_plsql(&rel_path, &content, graph);
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
            if Self::is_vb_cls(&content) {
                Self::parse_vb(&rel_path, &content, graph);
            } else {
                Self::parse_apex_class(&rel_path, &content, graph);
            }
            Ok(true)
        } else if matches!(extension, "vb" | "vba" | "bas" | "frm" | "ctl" | "vbs") {
            Self::parse_vb(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "trigger") {
            Self::parse_apex_trigger(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".oip-meta.xml") || file_name.ends_with(".os-meta.xml") || file_name.ends_with(".ouc-meta.xml") {
            parsers::parse_omnistudio_metadata(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".approvalProcess-meta.xml") {
            parsers::parse_approval_process(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".decisionMatrix-meta.xml") {
            parsers::parse_decision_matrix(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with("-meta.xml") || matches!(extension, "object" | "field") {
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
        } else if matches!(extension, "cmp" | "evt" | "intf" | "page" | "component") {
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
        } else if parsers::is_supported_json_file(file_name)
            || parsers::is_omnistudio_json_path(&rel_path, file_name)
            || (matches!(extension, "json") && parsers::is_omnistudio_json_content(&content))
        {
            if parsers::is_omnistudio_json_path(&rel_path, file_name) || parsers::is_omnistudio_json_content(&content) {
                parsers::parse_omnistudio_json(&rel_path, &content, graph);
            } else {
                Self::parse_json_manifest(&rel_path, &content, graph);
            }
            Ok(true)
        } else if parsers::is_supported_yaml_file(&rel_path, file_name)
            || (matches!(extension, "yaml" | "yml") && parsers::is_supported_yaml_content(&content))
        {
            Self::parse_yaml_manifest(&rel_path, &content, graph);
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

        let config = RuminConfig::load(&self.repo_root);
        let mut ignore_patterns = config.ignored_patterns;
        let ignore_path = self.repo_root.join(RuminConfig::IGNORE_FILE);
        if let Ok(content) = std::fs::read_to_string(&ignore_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    let pat = trimmed.trim_end_matches('/');
                    if !pat.is_empty() {
                        ignore_patterns.push(pat.to_string());
                    }
                }
            }
        }

        let ignore_filter = ignore_patterns.clone();
        let walker = WalkDir::new(&self.repo_root).into_iter().filter_entry(move |e| {
            let name = e.file_name().to_string_lossy();
            if matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | "venv" | ".venv" | "__pycache__" | ".aimem" | ".cargo" | ".rumin" | "build" | "dist" | "out" | "vendor" | ".next" | "coverage" | ".pytest_cache" | ".mypy_cache" | ".tox" | "tmp" | ".tmp"
            ) {
                return false;
            }
            if let Ok(meta) = e.metadata() {
                if meta.is_symlink() && !e.path().exists() {
                    return false;
                }
            } else {
                return false;
            }
            !ignore_filter.iter().any(|pat| name == pat.as_str())
        });

        for entry in walker.filter_map(|e| e.ok()) {
            if let Some(c) = cancel {
                if c.is_cancelled() {
                    bail!("Indexing cancelled by token");
                }
            }

            let path = entry.path();
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !Self::is_supported_file_path(path, extension) {
                continue;
            }

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

            let is_ignored = ignore_patterns.iter().any(|pat| {
                if pat.starts_with("*.") {
                    rel_path.ends_with(&pat[1..])
                } else {
                    rel_path == *pat
                        || rel_path.starts_with(&format!("{}/", pat))
                        || rel_path.contains(&format!("/{}", pat))
                }
            });
            if is_ignored {
                continue;
            }

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

        let total_work = delta.added.len() + delta.modified.len();
        if total_work > 0 {
            info!("Applying delta: {} files to index ({} added, {} modified, {} deleted)",
                total_work, delta.added.len(), delta.modified.len(), delta.deleted.len());
        }

        // 1. Process deletions
        for rel_path in delta.deleted {
            graph.remove_file(&rel_path);
            manifest.files.remove(&rel_path);
            stats.deleted += 1;
        }

        let mut processed = 0;
        // 2. Process modifications
        for (rel_path, path, new_record) in delta.modified {
            graph.remove_file(&rel_path);
            if Self::index_single_file(&self.repo_root, &path, graph, sidecar).await? {
                stats.modified += 1;
            }
            manifest.files.insert(rel_path, new_record);
            processed += 1;
            if total_work >= 25 && processed % 25 == 0 {
                info!("Indexing progress: {}/{} files...", processed, total_work);
            }
        }

        // 3. Process additions
        for (rel_path, path, new_record) in delta.added {
            if Self::index_single_file(&self.repo_root, &path, graph, sidecar).await? {
                stats.added += 1;
            }
            manifest.files.insert(rel_path, new_record);
            processed += 1;
            if total_work >= 25 && processed % 25 == 0 {
                info!("Indexing progress: {}/{} files...", processed, total_work);
            }
        }

        // 4. Ingest declarative semantic bridges (Layer B)
        let bridge_count = Self::load_semantic_bridges(&self.repo_root, graph);
        if bridge_count > 0 {
            stats.semantic_bridges = Some(bridge_count);
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
        stats.manifest_tracked_files = Some(manifest.files.len());
        let bridge_msg = match stats.semantic_bridges {
            Some(count) if count > 0 => format!(" (linked {} semantic bridges)", count),
            _ => String::new(),
        };
        stats.status_message = Some(if stats.added == 0 && stats.modified == 0 && stats.deleted == 0 {
            format!(
                "Up-to-date: All {} tracked files already match on-disk state (cached via manifest/watcher). Use force=true to re-index.{}",
                stats.unchanged, bridge_msg
            )
        } else {
            format!(
                "Synchronized: +{} added, ~{} modified, -{} deleted, {} unchanged.{}",
                stats.added, stats.modified, stats.deleted, stats.unchanged, bridge_msg
            )
        });

        info!(
            "Sync completed in {}ms: +{} added, ~{} modified, -{} deleted, {} unchanged",
            stats.duration_ms, stats.added, stats.modified, stats.deleted, stats.unchanged
        );
        Ok(stats)
    }

    /// Surgically re-index a single file in sub-50ms (Gap 13).
    pub async fn sync_single_file(
        &self,
        file_path: &Path,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
    ) -> Result<SingleFileReport> {
        let start = std::time::Instant::now();
        let abs_path = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            self.repo_root.join(file_path)
        };

        let rel_path = match abs_path.strip_prefix(&self.repo_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => abs_path.to_string_lossy().to_string(),
        };

        let manifest_path = self.repo_root.join(".rumin/manifest.json");
        let mut manifest = FileManifest::load_or_new(&manifest_path);

        if !abs_path.exists() {
            // File was deleted
            graph.remove_file(&rel_path);
            manifest.files.remove(&rel_path);
            let _ = manifest.save(&manifest_path);
            let default_checkpoint = self.repo_root.join(".rumin/checkpoints/default.bin");
            let _ = graph.save_binary_checkpoint(&default_checkpoint);
            return Ok(SingleFileReport {
                file_path: rel_path,
                action: "deleted".to_string(),
                entities_count: 0,
                duration_ms: start.elapsed().as_millis(),
            });
        }

        // Remove old entities for this file before re-indexing
        graph.remove_file(&rel_path);

        let indexed = Self::index_single_file(&self.repo_root, &abs_path, graph, sidecar).await?;
        let entity_count = graph.file_to_entities.get(&rel_path).map(|v| v.len()).unwrap_or(0);

        if indexed {
            if let Ok(content) = std::fs::read(&abs_path) {
                let mut hasher = sha2::Sha256::new();
                use sha2::Digest;
                hasher.update(&content);
                let hash = format!("{:x}", hasher.finalize());
                let meta = std::fs::metadata(&abs_path)?;
                let mtime_nanos = meta.modified().map(|m| m.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)).unwrap_or(0);
                manifest.files.insert(
                    rel_path.clone(),
                    ManifestRecord::Full {
                        mtime_nanos,
                        size: meta.len(),
                        hash,
                    },
                );
                let _ = manifest.save(&manifest_path);
            }
            let default_checkpoint = self.repo_root.join(".rumin/checkpoints/default.bin");
            let _ = graph.save_binary_checkpoint(&default_checkpoint);
        }

        if rel_path.starts_with(".rumin/bridges") || rel_path.starts_with(".rumin/mappings") {
            Self::load_semantic_bridges(&self.repo_root, graph);
        }

        Ok(SingleFileReport {
            file_path: rel_path,
            action: if indexed { "indexed".to_string() } else { "skipped".to_string() },
            entities_count: entity_count,
            duration_ms: start.elapsed().as_millis(),
        })
    }

    /// Layer B: Declarative Ingestion Mapping Integration (Cross-Database Semantic Bridges)
    /// Ingests semantic bridges from `.rumin/bridges.json`, `.rumin/mappings.json`,
    /// `.rumin/bridges.csv`, `.rumin/mappings.csv`, or any files in `.rumin/bridges/`.
    pub fn load_semantic_bridges(repo_root: &Path, graph: &mut CodeGraph) -> usize {
        let mut total_bridges = 0;
        let rumin_dir = repo_root.join(".rumin");
        if !rumin_dir.exists() {
            return 0;
        }

        // 1. Check direct JSON configuration files (.rumin/bridges.json, .rumin/mappings.json)
        let json_candidates = ["bridges.json", "mappings.json"];
        for candidate in &json_candidates {
            let json_path = rumin_dir.join(candidate);
            if json_path.is_file() {
                total_bridges += Self::load_json_bridges(&json_path, repo_root, graph);
            }
        }

        // 2. Check direct CSV mapping files (.rumin/bridges.csv, .rumin/mappings.csv)
        let csv_candidates = ["bridges.csv", "mappings.csv"];
        for candidate in &csv_candidates {
            let csv_path = rumin_dir.join(candidate);
            if csv_path.is_file() {
                total_bridges += Self::parse_csv_bridges(&csv_path, graph);
            }
        }

        // 3. Scan .rumin/bridges/ and .rumin/mappings/ directories if present
        for sub in &["bridges", "mappings"] {
            let sub_dir = rumin_dir.join(sub);
            if sub_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(sub_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if ext.eq_ignore_ascii_case("json") {
                                    total_bridges += Self::load_json_bridges(&path, repo_root, graph);
                                } else if ext.eq_ignore_ascii_case("csv") {
                                    total_bridges += Self::parse_csv_bridges(&path, graph);
                                }
                            }
                        }
                    }
                }
            }
        }

        if total_bridges > 0 {
            info!("Loaded {} declarative semantic bridges into knowledge graph", total_bridges);
        }
        total_bridges
    }

    fn load_json_bridges(path: &Path, repo_root: &Path, graph: &mut CodeGraph) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let rel_path = path.strip_prefix(repo_root).unwrap_or(path).to_string_lossy().to_string();
        let mut count = 0;

        // Try parsing as DeclarativeBridgesConfig { semantic_bridges: [...], csv_files: [...] }
        if let Ok(config) = serde_json::from_str::<DeclarativeBridgesConfig>(&content) {
            for bridge in config.semantic_bridges {
                if Self::inject_semantic_bridge(&bridge.source, &bridge.target, bridge.r#type.as_deref(), &rel_path, graph) {
                    count += 1;
                }
            }
            for csv_rel in config.csv_files {
                let csv_path = repo_root.join(csv_rel);
                if csv_path.is_file() {
                    count += Self::parse_csv_bridges(&csv_path, graph);
                }
            }
            return count;
        }

        // Fallback: try parsing directly as Vec<DeclarativeBridge>
        if let Ok(bridges) = serde_json::from_str::<Vec<DeclarativeBridge>>(&content) {
            for bridge in bridges {
                if Self::inject_semantic_bridge(&bridge.source, &bridge.target, bridge.r#type.as_deref(), &rel_path, graph) {
                    count += 1;
                }
            }
            return count;
        }

        count
    }

    pub fn inject_semantic_bridge(
        source: &str,
        target: &str,
        bridge_type: Option<&str>,
        rel_path: &str,
        graph: &mut CodeGraph,
    ) -> bool {
        let src = source.trim();
        let tgt = target.trim();
        if src.is_empty() || tgt.is_empty() || src == tgt {
            return false;
        }

        let btype = bridge_type.unwrap_or("ETL_MAPPING");

        // 1. Ensure source entity exists
        if graph.get_entity(src).is_none() {
            let mut src_ent = CodeEntity::new(src, src, EntityType::Table, rel_path, 1, 1);
            src_ent.signature = Some(format!("Bridge Table {}", src));
            src_ent.metadata.insert("bridge_type".to_string(), btype.to_string());
            graph.add_entity(src_ent);
        }

        // 2. Ensure target entity exists
        if graph.get_entity(tgt).is_none() {
            let mut tgt_ent = CodeEntity::new(tgt, tgt, EntityType::Table, rel_path, 1, 1);
            tgt_ent.signature = Some(format!("Bridge Table {}", tgt));
            tgt_ent.metadata.insert("bridge_type".to_string(), btype.to_string());
            graph.add_entity(tgt_ent);
        }

        // 3. Inject bidirectional semantic bridge edges
        graph.add_edge(src, tgt, EdgeKind::SemanticBridge);
        graph.add_edge(tgt, src, EdgeKind::SemanticBridge);

        true
    }

    pub fn parse_csv_bridges(path: &Path, graph: &mut CodeGraph) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let file_name = path.to_string_lossy().to_string();
        let mut lines = content.lines();
        let header_line = match lines.next() {
            Some(h) => h,
            None => return 0,
        };

        let headers: Vec<String> = header_line
            .split(',')
            .map(|h| h.trim().trim_matches('"').trim_matches('\'').to_ascii_uppercase())
            .collect();

        if headers.is_empty() {
            return 0;
        }

        // Detect source and target column indices
        let mut src_col: Option<usize> = None;
        let mut tgt_col: Option<usize> = None;
        let mut type_col: Option<usize> = None;

        for (idx, h) in headers.iter().enumerate() {
            match h.as_str() {
                "SOURCE" | "SOURCE_SYMBOL" | "SOURCE_TABLE" | "SOURCE_ENTITY" | "FROM"
                | "CATALIST_ROW_ID" | "CATALIST_PART_NUM" | "SIEBEL_TABLE" => {
                    if src_col.is_none() {
                        src_col = Some(idx);
                    }
                }
                "TARGET" | "TARGET_SYMBOL" | "TARGET_SOBJECT" | "TARGET_ENTITY" | "TO"
                | "SF_PRODUCT_CODE" | "SF_GLOBAL_KEY" | "SF_SOBJECT" | "SF_PRODUCT_NAME" => {
                    if tgt_col.is_none() {
                        tgt_col = Some(idx);
                    }
                }
                "TYPE" | "BRIDGE_TYPE" | "MAPPING_TYPE" | "ACTION" | "MIGRATION_ACTION" => {
                    if type_col.is_none() {
                        type_col = Some(idx);
                    }
                }
                _ => {}
            }
        }

        // Fallback if headers were not recognized: col 0 = source, col 1 = target (if >= 2 columns)
        let src_idx = src_col.unwrap_or(0);
        let tgt_idx = tgt_col.unwrap_or_else(|| if headers.len() > 1 { 1 } else { 0 });

        if src_idx == tgt_idx {
            return 0;
        }

        let mut count = 0;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = trimmed.split(',').collect();
            if parts.len() > src_idx && parts.len() > tgt_idx {
                let src_val = parts[src_idx].trim().trim_matches('"').trim_matches('\'');
                let tgt_val = parts[tgt_idx].trim().trim_matches('"').trim_matches('\'');
                let type_val = type_col
                    .and_then(|c| parts.get(c))
                    .map(|t| t.trim().trim_matches('"').trim_matches('\''));

                if !src_val.is_empty() && !tgt_val.is_empty() {
                    if Self::inject_semantic_bridge(src_val, tgt_val, type_val, &file_name, graph) {
                        count += 1;
                    }
                }
            }
        }

        count
    }
}

/// Single file surgical re-indexing report (Gap 13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleFileReport {
    pub file_path: String,
    pub action: String,
    pub entities_count: usize,
    pub duration_ms: u128,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_tracked_files: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_bridges: Option<usize>,
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
