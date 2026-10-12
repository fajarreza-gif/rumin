use crate::consolidate::MemoryConsolidator;
use crate::db::MemoryDb;
use crate::episodic::EpisodicEngine;
use crate::graph::CodeGraph;
use crate::hippocampal::HippocampalEngine;
use crate::indexer::Indexer;
use crate::session::SessionManager;
use crate::sidecar::SidecarClient;
use crate::trace::TraceIngestor;
use crate::types::ContextBudget;
use crate::watcher::FileWatcher;
use anyhow::Result;
use arc_swap::ArcSwap;
use petgraph::Direction;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Notify, RwLock};
use tracing::{error, info};

pub struct MCPServer {
    repo_root: Arc<RwLock<PathBuf>>,
    graph: Arc<ArcSwap<CodeGraph>>,
    engine: Arc<HippocampalEngine>,
    sidecar: Arc<SidecarClient>,
    db: Arc<RwLock<Arc<MemoryDb>>>,
    episodic: Arc<RwLock<Arc<EpisodicEngine>>>,
    session: Arc<RwLock<Arc<SessionManager>>>,
    trace: Arc<RwLock<Arc<TraceIngestor>>>,
    ready: Arc<Notify>,
    is_ready: Arc<AtomicBool>,
}

impl MCPServer {
    pub fn new(repo_root: PathBuf, sidecar_path: PathBuf) -> Self {
        let db = Arc::new(MemoryDb::open(&repo_root).expect("Failed to initialize .rumin/memory.db"));
        let episodic = Arc::new(EpisodicEngine::new(db.clone()));
        let session = Arc::new(SessionManager::new(db.clone()));
        let trace = Arc::new(TraceIngestor::new(db.clone()));

        Self {
            repo_root: Arc::new(RwLock::new(repo_root)),
            graph: Arc::new(ArcSwap::new(Arc::new(CodeGraph::new()))),
            engine: Arc::new(HippocampalEngine::default()),
            sidecar: Arc::new(SidecarClient::new(sidecar_path)),
            db: Arc::new(RwLock::new(db)),
            episodic: Arc::new(RwLock::new(episodic)),
            session: Arc::new(RwLock::new(session)),
            trace: Arc::new(RwLock::new(trace)),
            ready: Arc::new(Notify::new()),
            is_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn db(&self) -> Arc<MemoryDb> {
        self.db.read().await.clone()
    }

    pub async fn episodic(&self) -> Arc<EpisodicEngine> {
        self.episodic.read().await.clone()
    }

    pub async fn session(&self) -> Arc<SessionManager> {
        self.session.read().await.clone()
    }

    pub async fn trace(&self) -> Arc<TraceIngestor> {
        self.trace.read().await.clone()
    }

    pub fn graph(&self) -> Arc<CodeGraph> {
        self.graph.load_full()
    }

    pub fn graph_swap(&self) -> &Arc<ArcSwap<CodeGraph>> {
        &self.graph
    }

    pub fn mark_ready(&self) {
        self.is_ready.store(true, std::sync::atomic::Ordering::SeqCst);
        self.ready.notify_waiters();
    }

    pub async fn handle_request_for_test(&self, req: Value) -> Option<Value> {
        self.mark_ready();
        self.handle_request(req).await
    }

    pub async fn run_stdio(&self) -> Result<()> {
        info!("Starting ai-rumin MCP Stdio JSON-RPC 2.0 Server...");

        // Hydrate graph asynchronously in background so stdin is immediately ready for JSON-RPC initialize
        let graph_clone = self.graph.clone();
        let episodic_clone = self.episodic.clone();
        let repo_root_clone = self.repo_root.clone();
        let sidecar_clone = self.sidecar.clone();
        let ready_clone = self.ready.clone();
        let is_ready_clone = self.is_ready.clone();

        tokio::spawn(async move {
            let repo_root = repo_root_clone.read().await.clone();
            let default_cp = repo_root.join(".rumin/checkpoints/default.bin");
            let mut loaded_from_cp = false;
            let mut g = if default_cp.exists() {
                if let Ok(loaded) = CodeGraph::load_binary_checkpoint(&default_cp) {
                    info!("Hydrated graph from checkpoint {:?} ({} nodes)", default_cp, loaded.node_count());
                    loaded_from_cp = true;
                    loaded
                } else {
                    CodeGraph::new()
                }
            } else {
                CodeGraph::new()
            };

            let ep = episodic_clone.read().await.clone();
            let _ = ep.bind_episodes_to_graph(&mut g);

            // Fast incremental sync
            let indexer = Indexer::new(&repo_root);
            if let Ok(stats) = indexer.sync_repository(&mut g, &sidecar_clone, false).await {
                if !loaded_from_cp || stats.added > 0 || stats.modified > 0 || stats.deleted > 0 {
                    let _ = ep.bind_episodes_to_graph(&mut g);
                    let _ = g.save_binary_checkpoint(&default_cp);
                }
            }

            graph_clone.store(Arc::new(g));
            is_ready_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            ready_clone.notify_waiters();
        });

        // Start background FileWatcher so graph remains in sync across file modifications
        let cur_repo = self.repo_root.read().await.clone();
        let watcher = FileWatcher::new(cur_repo, self.graph.clone(), self.sidecar.clone());
        let _watcher_guard = watcher.start().ok();
        if _watcher_guard.is_some() {
            info!("Live FileWatcher started in background for MCP server.");
        }

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            let req: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    error!("Malformed JSON input: {}", e);
                    continue;
                }
            };

            if let Some(resp) = self.handle_request(req).await {
                let serialized = serde_json::to_string(&resp)?;
                stdout.write_all(serialized.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }

        Ok(())
    }

    async fn handle_request(&self, req: Value) -> Option<Value> {
        let req_id = req.get("id").cloned();
        let method = match req.get("method").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => return None,
        };
        let params = req.get("params").cloned().unwrap_or(json!({}));

        match method {
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "ai-rumin",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            })),

            "ping" => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {}
            })),

            "notifications/initialized" => None,

            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "tools": [
                        {
                            "name": "rumin_query",
                            "description": "Retrieve multi-hop associative subgraph and context for an issue or symbol under strict token budgets using Personalized PageRank.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string", "description": "Natural language query or symbol name" },
                                    "budget": { "type": "integer", "description": "Max context tokens (default: 1500)" },
                                    "expand_bodies": { "type": "boolean", "description": "Include actual AST code snippet slices directly in response" }
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "rumin_digest",
                            "description": "Return topological architectural digest and scale metrics of the codebase.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "rumin_index",
                            "description": "Trigger a full parallel re-indexing of the repository.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "rumin_traverse",
                            "description": "Traverse directional callers, callees, or implementations of a symbol.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "symbol_id": { "type": "string", "description": "Qualified symbol ID or name" },
                                    "direction": { "type": "string", "enum": ["in", "out", "callers", "callees"], "description": "Traversal direction" },
                                    "depth": { "type": "integer", "description": "Traversal depth hops (1-5, default: 1)" }
                                },
                                "required": ["symbol_id"]
                            }
                        },
                        {
                            "name": "rumin_sync",
                            "description": "Perform fast incremental sync across repository, updating modified/added files and pruning deleted ones.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "force": { "type": "boolean", "description": "Force full re-check of all files" }
                                }
                            }
                        },
                        {
                            "name": "rumin_checkpoint",
                            "description": "Export or restore a high-speed compact binary checkpoint of the knowledge graph via bincode.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "action": { "type": "string", "enum": ["save", "load"], "description": "Checkpoint action" },
                                    "path": { "type": "string", "description": "Custom checkpoint file path (optional)" }
                                },
                                "required": ["action"]
                            }
                        },
                        {
                            "name": "rumin_diff",
                            "description": "Compute topological delta (added, modified, removed entities and edges) against a checkpoint.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "checkpoint_path": { "type": "string", "description": "Checkpoint file path to diff against" }
                                }
                            }
                        },
                        {
                            "name": "rumin_hazards",
                            "description": "Perform blast radius analysis and risk scoring starting from modified symbols or files.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "symbols": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "Root symbol IDs or file paths"
                                    }
                                },
                                "required": ["symbols"]
                            }
                        },
                        {
                            "name": "rumin_verify",
                            "description": "Verify structural integrity of the graph, detecting dangling references and orphaned interfaces.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "rumin_audit",
                            "description": "Audit architectural invariants and cross-layer isolation rules.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "rumin_record_episode",
                            "description": "Record an episodic debugging trial, patch outcome, or environment lesson into persistent memory.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "title": { "type": "string", "description": "Brief summary title" },
                                    "content": { "type": "string", "description": "Detailed explanation of solution or outcome" },
                                    "success": { "type": "boolean", "description": "True if positive resolution, False if failed trial" },
                                    "bound_symbols": { "type": "array", "items": { "type": "string" }, "description": "Touched symbol qualified IDs" },
                                    "bound_files": { "type": "array", "items": { "type": "string" }, "description": "Touched file paths" },
                                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Searchable tags e.g. ['bug-fix', 'auth']" }
                                },
                                "required": ["title", "content"]
                            }
                        },
                        {
                            "name": "rumin_search_episodes",
                            "description": "Search episodic memories, past debugging trials, and bug-fix solutions.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string", "description": "Search keyword or error description" },
                                    "limit": { "type": "integer", "description": "Maximum number of episodes to return" }
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "rumin_session_create",
                            "description": "Create a new agent task session for conversation-scoped tracking.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "Session title or task name" },
                                    "metadata": { "type": "object", "description": "Optional session metadata" }
                                },
                                "required": ["name"]
                            }
                        },
                        {
                            "name": "rumin_session_list",
                            "description": "List all active and completed agent task sessions.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "rumin_session_inspect",
                            "description": "Inspect session metadata and its sequence of recorded working notes.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "session_id": { "type": "string", "description": "Session ID e.g. sess_abc123" }
                                },
                                "required": ["session_id"]
                            }
                        },
                        {
                            "name": "rumin_add_working_note",
                            "description": "Append a scratchpad working note or intermediate task state to a session.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "session_id": { "type": "string", "description": "Target session ID" },
                                    "content": { "type": "string", "description": "Note content or intermediate findings" }
                                },
                                "required": ["session_id", "content"]
                            }
                        },
                        {
                            "name": "rumin_consolidate",
                            "description": "Execute algorithmic memory consolidation (sleep phase): prunes dead transient nodes and builds distilled multi-hop shortcuts.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "rumin_ingest_trace",
                            "description": "Ingest tool execution traces (JSONL) and register action frequencies in memory.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "trace_path": { "type": "string", "description": "Path to JSONL execution trace file" },
                                    "session_id": { "type": "string", "description": "Optional session ID to associate" }
                                },
                                "required": ["trace_path"]
                            }
                        },
                        {
                            "name": "rumin_switch_repo",
                            "description": "Dynamically switch active repository root, re-initializing memory database, checkpoints, and graph.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "repo_path": { "type": "string", "description": "Absolute path to target repository directory" }
                                },
                                "required": ["repo_path"]
                            }
                        }
                    ]
                }
            })),

            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !self.is_ready.load(std::sync::atomic::Ordering::SeqCst) {
                    self.ready.notified().await;
                }
                let result = self.call_tool(name, args).await;

                Some(json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": result
                            }
                        ]
                    }
                }))
            }

            _ => {
                if req_id.is_none() {
                    None
                } else {
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method '{}' not found", method)
                        }
                    }))
                }
            }
        }
    }

    async fn call_tool(&self, name: &str, args: Value) -> String {
        match name {
            "rumin_query" => {
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let budget_tokens = args.get("budget").and_then(|b| b.as_u64()).unwrap_or(1500) as usize;
                let expand_bodies = args.get("expand_bodies").and_then(|e| e.as_bool()).unwrap_or(false);

                let budget = ContextBudget {
                    total_token_limit: budget_tokens,
                    max_symbols: if expand_bodies { 8 } else { 12 },
                    include_signatures: true,
                    expand_bodies,
                };

                let graph_snapshot = self.graph.load();
                let res = self.engine.query(&graph_snapshot, query, &budget);
                res.formatted_context
            }

            "rumin_digest" => {
                let graph_snapshot = self.graph.load();
                let stats = graph_snapshot.summary_stats();
                serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
            }

            "rumin_index" => {
                let repo_root = self.repo_root.read().await.clone();
                let mut new_graph = (*self.graph.load_full()).clone();
                let indexer = Indexer::new(&repo_root);
                match indexer.index_repository(&mut new_graph, &self.sidecar).await {
                    Ok(count) => {
                        let ep = self.episodic.read().await.clone();
                        let _ = ep.bind_episodes_to_graph(&mut new_graph);
                        let node_count = new_graph.node_count();
                        self.graph.store(Arc::new(new_graph));
                        format!("Successfully re-indexed {} files. Graph now has {} nodes.", count, node_count)
                    }
                    Err(e) => format!("Error during indexing: {}", e),
                }
            }

            "rumin_traverse" => {
                let symbol_id = args.get("symbol_id").and_then(|s| s.as_str()).unwrap_or("");
                let dir_str = args.get("direction").and_then(|d| d.as_str()).unwrap_or("out");
                let max_depth = args.get("depth").and_then(|d| d.as_u64()).unwrap_or(1).clamp(1, 5) as usize;
                let direction = if dir_str == "in" || dir_str == "callers" { Direction::Incoming } else { Direction::Outgoing };

                let graph_snapshot = self.graph.load();

                use std::collections::{HashSet, VecDeque};
                let mut visited = HashSet::new();
                let mut queue = VecDeque::new();
                queue.push_back((symbol_id.to_string(), 0usize));
                visited.insert(symbol_id.to_string());

                let mut lines = Vec::new();
                lines.push(format!("Traversal ({} hops, {:?}) from '{}':", max_depth, direction, symbol_id));

                while let Some((curr_id, depth)) = queue.pop_front() {
                    if depth >= max_depth {
                        continue;
                    }

                    for (neighbor, edge_kind) in graph_snapshot.neighbors(&curr_id, direction) {
                        let indent = "  ".repeat(depth + 1);
                        lines.push(format!("{}- [{:?}] {} ({:?})", indent, edge_kind, neighbor.id, neighbor.entity_type));
                        if visited.insert(neighbor.id.clone()) {
                            queue.push_back((neighbor.id.clone(), depth + 1));
                        }
                    }
                }

                if lines.len() == 1 {
                    lines.push("  (no connected symbols found)".to_string());
                }
                lines.join("\n")
            }

            "rumin_sync" => {
                let repo_root = self.repo_root.read().await.clone();
                let force = args.get("force").and_then(|f| f.as_bool()).unwrap_or(false);
                let mut new_graph = (*self.graph.load_full()).clone();
                let indexer = Indexer::new(&repo_root);
                match indexer.sync_repository(&mut new_graph, &self.sidecar, force).await {
                    Ok(stats) => {
                        let ep = self.episodic.read().await.clone();
                        let _ = ep.bind_episodes_to_graph(&mut new_graph);
                        self.graph.store(Arc::new(new_graph));
                        serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => format!("Error during sync: {}", e),
                }
            }

            "rumin_checkpoint" => {
                let action = args.get("action").and_then(|a| a.as_str()).unwrap_or("save");
                let repo_root = self.repo_root.read().await.clone();
                let default_path = repo_root.join(".rumin/checkpoints/default.bin");
                let checkpoint_path = args.get("path")
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from)
                    .unwrap_or(default_path);

                if let Some(parent) = checkpoint_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                if action == "save" {
                    let graph_snapshot = self.graph.load();
                    match graph_snapshot.save_binary_checkpoint(&checkpoint_path) {
                        Ok(_) => format!("Successfully exported binary checkpoint to {:?}", checkpoint_path),
                        Err(e) => format!("Error saving checkpoint: {}", e),
                    }
                } else {
                    match CodeGraph::load_binary_checkpoint(&checkpoint_path) {
                        Ok(loaded) => {
                            let node_count = loaded.node_count();
                            self.graph.store(Arc::new(loaded));
                            format!("Successfully loaded binary checkpoint from {:?} ({} nodes)", checkpoint_path, node_count)
                        }
                        Err(e) => format!("Error loading checkpoint: {}", e),
                    }
                }
            }

            "rumin_diff" => {
                let repo_root = self.repo_root.read().await.clone();
                let default_path = repo_root.join(".rumin/checkpoints/default.bin");
                let checkpoint_path = args.get("checkpoint_path")
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from)
                    .unwrap_or(default_path);

                match CodeGraph::load_binary_checkpoint(&checkpoint_path) {
                    Ok(base_graph) => {
                        let graph_snapshot = self.graph.load();
                        let delta = base_graph.diff(&graph_snapshot);
                        serde_json::to_string_pretty(&delta).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => format!("Error reading checkpoint for diff: {}", e),
                }
            }

            "rumin_hazards" => {
                let symbols: Vec<String> = args.get("symbols")
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();

                let graph_snapshot = self.graph.load();
                let report = graph_snapshot.blast_radius(&symbols);
                serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
            }

            "rumin_verify" => {
                let graph_snapshot = self.graph.load();
                let issues = graph_snapshot.verify();
                serde_json::to_string_pretty(&issues).unwrap_or_else(|_| "[]".to_string())
            }

            "rumin_audit" => {
                let graph_snapshot = self.graph.load();
                let violations = graph_snapshot.audit();
                serde_json::to_string_pretty(&violations).unwrap_or_else(|_| "[]".to_string())
            }

            "rumin_record_episode" => {
                let title = args.get("title").and_then(|t| t.as_str()).unwrap_or("");
                let content = args.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let success = args.get("success").and_then(|s| s.as_bool()).unwrap_or(true);
                let bound_symbols: Vec<String> = args.get("bound_symbols")
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let bound_files: Vec<String> = args.get("bound_files")
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let tags: Vec<String> = args.get("tags")
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();

                let ep_engine = self.episodic.read().await.clone();
                match ep_engine.record_episode(title, content, success, &bound_symbols, &bound_files, &tags) {
                    Ok(ep) => {
                        let mut new_graph = (*self.graph.load_full()).clone();
                        let _ = ep_engine.bind_episodes_to_graph(&mut new_graph);
                        self.graph.store(Arc::new(new_graph));
                        format!("Successfully recorded episode '{}' (ID: {}).", ep.title, ep.id)
                    }
                    Err(e) => format!("Error recording episode: {}", e),
                }
            }

            "rumin_search_episodes" => {
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;

                let ep_engine = self.episodic.read().await.clone();
                match ep_engine.search_episodes(query, limit) {
                    Ok(episodes) => serde_json::to_string_pretty(&episodes).unwrap_or_else(|_| "[]".to_string()),
                    Err(e) => format!("Error searching episodes: {}", e),
                }
            }

            "rumin_session_create" => {
                let name = args.get("name").and_then(|n| n.as_str()).unwrap_or("New Session");
                let metadata = args.get("metadata").cloned();

                let sess_engine = self.session.read().await.clone();
                match sess_engine.create_session(name, metadata) {
                    Ok(sess) => serde_json::to_string_pretty(&sess).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => format!("Error creating session: {}", e),
                }
            }

            "rumin_session_list" => {
                let sess_engine = self.session.read().await.clone();
                match sess_engine.list_sessions() {
                    Ok(sessions) => serde_json::to_string_pretty(&sessions).unwrap_or_else(|_| "[]".to_string()),
                    Err(e) => format!("Error listing sessions: {}", e),
                }
            }

            "rumin_session_inspect" => {
                let session_id = args.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
                let sess_engine = self.session.read().await.clone();
                match sess_engine.get_session(session_id) {
                    Ok(Some(sess)) => {
                        let notes = sess_engine.get_working_notes(session_id).unwrap_or_default();
                        let result = json!({
                            "session": sess,
                            "working_notes": notes
                        });
                        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
                    }
                    Ok(None) => format!("Session '{}' not found", session_id),
                    Err(e) => format!("Error inspecting session: {}", e),
                }
            }

            "rumin_add_working_note" => {
                let session_id = args.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
                let content = args.get("content").and_then(|c| c.as_str()).unwrap_or("");

                let sess_engine = self.session.read().await.clone();
                match sess_engine.add_working_note(session_id, content) {
                    Ok(note) => format!("Added note '{}' to session {}.", note.id, note.session_id),
                    Err(e) => format!("Error adding note: {}", e),
                }
            }

            "rumin_consolidate" => {
                let mut new_graph = (*self.graph.load_full()).clone();
                match MemoryConsolidator::consolidate(&mut new_graph) {
                    Ok(report) => {
                        self.graph.store(Arc::new(new_graph));
                        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => format!("Error consolidating memory: {}", e),
                }
            }

            "rumin_ingest_trace" => {
                let trace_path_str = args.get("trace_path").and_then(|p| p.as_str()).unwrap_or("");
                let session_id = args.get("session_id").and_then(|s| s.as_str());
                let path = PathBuf::from(trace_path_str);

                let trace_engine = self.trace.read().await.clone();
                match trace_engine.ingest_file(&path, session_id) {
                    Ok(report) => serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => format!("Error ingesting trace: {}", e),
                }
            }

            "rumin_switch_repo" => {
                let repo_path_str = args.get("repo_path").and_then(|p| p.as_str()).unwrap_or("");
                if repo_path_str.is_empty() {
                    return "Error: repo_path argument is required".to_string();
                }
                let new_path = PathBuf::from(repo_path_str);
                if !new_path.exists() || !new_path.is_dir() {
                    return format!("Error: repository path {:?} does not exist or is not a directory", new_path);
                }

                match MemoryDb::open(&new_path) {
                    Ok(new_db) => {
                        let new_db_arc = Arc::new(new_db);
                        let new_episodic = Arc::new(EpisodicEngine::new(new_db_arc.clone()));
                        let new_session = Arc::new(SessionManager::new(new_db_arc.clone()));
                        let new_trace = Arc::new(TraceIngestor::new(new_db_arc.clone()));

                        let default_cp = new_path.join(".rumin/checkpoints/default.bin");
                        let mut new_graph = if default_cp.exists() {
                            CodeGraph::load_binary_checkpoint(&default_cp).unwrap_or_default()
                        } else {
                            CodeGraph::new()
                        };
                        let _ = new_episodic.bind_episodes_to_graph(&mut new_graph);

                        *self.repo_root.write().await = new_path.clone();
                        *self.db.write().await = new_db_arc;
                        *self.episodic.write().await = new_episodic;
                        *self.session.write().await = new_session;
                        *self.trace.write().await = new_trace;
                        self.graph.store(Arc::new(new_graph));

                        let stats = self.graph.load().summary_stats();
                        json!({
                            "status": "switched",
                            "repo_root": new_path.to_string_lossy(),
                            "stats": stats
                        }).to_string()
                    }
                    Err(e) => format!("Error initializing database for new repository: {}", e),
                }
            }

            _ => format!("Unknown tool: {}", name),
        }
    }
}
