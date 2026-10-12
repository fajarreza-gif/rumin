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
use anyhow::Result;
use petgraph::Direction;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Notify, RwLock};
use tracing::{debug, error, info};

pub struct MCPServer {
    repo_root: PathBuf,
    graph: Arc<RwLock<CodeGraph>>,
    engine: Arc<HippocampalEngine>,
    sidecar: Arc<SidecarClient>,
    db: Arc<MemoryDb>,
    episodic: Arc<EpisodicEngine>,
    session: Arc<SessionManager>,
    trace: Arc<TraceIngestor>,
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
            repo_root: repo_root.clone(),
            graph: Arc::new(RwLock::new(CodeGraph::new())),
            engine: Arc::new(HippocampalEngine::default()),
            sidecar: Arc::new(SidecarClient::new(sidecar_path)),
            db,
            episodic,
            session,
            trace,
            ready: Arc::new(Notify::new()),
            is_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn db(&self) -> &Arc<MemoryDb> {
        &self.db
    }

    pub fn episodic(&self) -> &Arc<EpisodicEngine> {
        &self.episodic
    }

    pub fn session(&self) -> &Arc<SessionManager> {
        &self.session
    }

    pub fn trace(&self) -> &Arc<TraceIngestor> {
        &self.trace
    }

    pub fn graph(&self) -> &Arc<RwLock<CodeGraph>> {
        &self.graph
    }

    pub async fn handle_request_for_test(&self, req: Value) -> Option<Value> {
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
            let default_cp = repo_root_clone.join(".rumin/checkpoints/default.bin");
            let mut loaded_from_cp = false;
            if default_cp.exists() {
                if let Ok(loaded) = CodeGraph::load_binary_checkpoint(&default_cp) {
                    let mut g = graph_clone.write().await;
                    *g = loaded;
                    let _ = episodic_clone.bind_episodes_to_graph(&mut g);
                    loaded_from_cp = true;
                    info!("Hydrated graph from checkpoint {:?} ({} nodes)", default_cp, g.node_count());
                }
            }

            // Perform fast incremental sync
            let mut g = graph_clone.write().await;
            let indexer = Indexer::new(&repo_root_clone);
            if let Ok(stats) = indexer.sync_repository(&mut g, &sidecar_clone, false).await {
                if !loaded_from_cp || stats.added > 0 || stats.modified > 0 || stats.deleted > 0 {
                    let _ = episodic_clone.bind_episodes_to_graph(&mut g);
                    let _ = g.save_binary_checkpoint(&default_cp);
                }
            }
            is_ready_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            ready_clone.notify_waiters();
        });

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            let request: Value = match serde_json::from_str(&line) {
                Ok(val) => val,
                Err(err) => {
                    error!("Invalid JSON received: {}", err);
                    let err_response = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32700,
                            "message": "Parse error"
                        }
                    });
                    let mut out = serde_json::to_string(&err_response)?;
                    out.push('\n');
                    stdout.write_all(out.as_bytes()).await?;
                    stdout.flush().await?;
                    continue;
                }
            };

            if let Some(response) = self.handle_request(request).await {
                let mut out = serde_json::to_string(&response)?;
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
            }
        }

        Ok(())
    }

    async fn handle_request(&self, req: Value) -> Option<Value> {
        let req_id = match req.get("id") {
            Some(Value::Null) | None => None,
            Some(val) => Some(val.clone()),
        };
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));

        // In JSON-RPC 2.0, notifications (no ID) MUST NOT receive a response
        if req_id.is_none() {
            debug!("Received notification: {}", method);
            return None;
        }

        match method {
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "ai-rumin",
                        "version": "0.1.0"
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
                                    "budget": { "type": "integer", "description": "Max context tokens (default: 1500)" }
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
                            "description": "Traverse incoming or outgoing dependencies for a specific symbol ID.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "symbol_id": { "type": "string", "description": "Qualified symbol ID" },
                                    "direction": { "type": "string", "enum": ["in", "out"], "description": "Traversal direction" }
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

            _ => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            })),
        }
    }

    async fn call_tool(&self, name: &str, args: Value) -> String {
        match name {
            "rumin_query" => {
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let budget_tokens = args.get("budget").and_then(|b| b.as_u64()).unwrap_or(1500) as usize;

                let budget = ContextBudget {
                    total_token_limit: budget_tokens,
                    max_symbols: 12,
                    include_signatures: true,
                    expand_bodies: false,
                };

                let graph_guard = self.graph.read().await;
                let res = self.engine.query(&graph_guard, query, &budget);
                res.formatted_context
            }

            "rumin_digest" => {
                let graph_guard = self.graph.read().await;
                let stats = graph_guard.summary_stats();
                serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
            }

            "rumin_index" => {
                let mut graph_guard = self.graph.write().await;
                let indexer = Indexer::new(&self.repo_root);
                match indexer.index_repository(&mut graph_guard, &self.sidecar).await {
                    Ok(count) => {
                        let _ = self.episodic.bind_episodes_to_graph(&mut graph_guard);
                        format!("Successfully re-indexed {} files. Graph now has {} nodes.", count, graph_guard.node_count())
                    }
                    Err(e) => format!("Error during indexing: {}", e),
                }
            }

            "rumin_traverse" => {
                let symbol_id = args.get("symbol_id").and_then(|s| s.as_str()).unwrap_or("");
                let dir_str = args.get("direction").and_then(|d| d.as_str()).unwrap_or("out");
                let direction = if dir_str == "in" { Direction::Incoming } else { Direction::Outgoing };

                let graph_guard = self.graph.read().await;
                let neighbors = graph_guard.neighbors(symbol_id, direction);

                let mut lines = vec![format!("Traversal for symbol: {} ({:?})", symbol_id, direction)];
                for (neighbor, edge) in neighbors {
                    lines.push(format!("  - [{:?}] {} ({}:{})", edge, neighbor.id, neighbor.file_path, neighbor.line_start));
                }
                lines.join("\n")
            }

            "rumin_sync" => {
                let force = args.get("force").and_then(|f| f.as_bool()).unwrap_or(false);
                let mut graph_guard = self.graph.write().await;
                let indexer = Indexer::new(&self.repo_root);
                match indexer.sync_repository(&mut graph_guard, &self.sidecar, force).await {
                    Ok(stats) => {
                        let _ = self.episodic.bind_episodes_to_graph(&mut graph_guard);
                        serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => format!("Error during sync: {}", e),
                }
            }

            "rumin_checkpoint" => {
                let action = args.get("action").and_then(|a| a.as_str()).unwrap_or("save");
                let default_path = self.repo_root.join(".rumin/checkpoints/default.bin");
                let checkpoint_path = args.get("path")
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from)
                    .unwrap_or(default_path);

                if let Some(parent) = checkpoint_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                if action == "save" {
                    let graph_guard = self.graph.read().await;
                    match graph_guard.save_binary_checkpoint(&checkpoint_path) {
                        Ok(_) => format!("Successfully exported binary checkpoint to {:?}", checkpoint_path),
                        Err(e) => format!("Error saving checkpoint: {}", e),
                    }
                } else {
                    let mut graph_guard = self.graph.write().await;
                    match CodeGraph::load_binary_checkpoint(&checkpoint_path) {
                        Ok(loaded) => {
                            let node_count = loaded.node_count();
                            *graph_guard = loaded;
                            format!("Successfully loaded binary checkpoint from {:?} ({} nodes)", checkpoint_path, node_count)
                        }
                        Err(e) => format!("Error loading checkpoint: {}", e),
                    }
                }
            }

            "rumin_diff" => {
                let default_path = self.repo_root.join(".rumin/checkpoints/default.bin");
                let checkpoint_path = args.get("checkpoint_path")
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from)
                    .unwrap_or(default_path);

                match CodeGraph::load_binary_checkpoint(&checkpoint_path) {
                    Ok(base_graph) => {
                        let graph_guard = self.graph.read().await;
                        let delta = base_graph.diff(&graph_guard);
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

                let graph_guard = self.graph.read().await;
                let report = graph_guard.blast_radius(&symbols);
                serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
            }

            "rumin_verify" => {
                let graph_guard = self.graph.read().await;
                let issues = graph_guard.verify();
                serde_json::to_string_pretty(&issues).unwrap_or_else(|_| "[]".to_string())
            }

            "rumin_audit" => {
                let graph_guard = self.graph.read().await;
                let violations = graph_guard.audit();
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

                match self.episodic.record_episode(title, content, success, &bound_symbols, &bound_files, &tags) {
                    Ok(ep) => {
                        let mut graph_guard = self.graph.write().await;
                        let _ = self.episodic.bind_episodes_to_graph(&mut graph_guard);
                        format!("Successfully recorded episode '{}' (ID: {}).", ep.title, ep.id)
                    }
                    Err(e) => format!("Error recording episode: {}", e),
                }
            }

            "rumin_search_episodes" => {
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;

                match self.episodic.search_episodes(query, limit) {
                    Ok(episodes) => serde_json::to_string_pretty(&episodes).unwrap_or_else(|_| "[]".to_string()),
                    Err(e) => format!("Error searching episodes: {}", e),
                }
            }

            "rumin_session_create" => {
                let name = args.get("name").and_then(|n| n.as_str()).unwrap_or("New Session");
                let metadata = args.get("metadata").cloned();

                match self.session.create_session(name, metadata) {
                    Ok(sess) => serde_json::to_string_pretty(&sess).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => format!("Error creating session: {}", e),
                }
            }

            "rumin_session_list" => {
                match self.session.list_sessions() {
                    Ok(sessions) => serde_json::to_string_pretty(&sessions).unwrap_or_else(|_| "[]".to_string()),
                    Err(e) => format!("Error listing sessions: {}", e),
                }
            }

            "rumin_session_inspect" => {
                let session_id = args.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
                match self.session.get_session(session_id) {
                    Ok(Some(sess)) => {
                        let notes = self.session.get_working_notes(session_id).unwrap_or_default();
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

                match self.session.add_working_note(session_id, content) {
                    Ok(note) => format!("Added note '{}' to session {}.", note.id, note.session_id),
                    Err(e) => format!("Error adding note: {}", e),
                }
            }

            "rumin_consolidate" => {
                let mut graph_guard = self.graph.write().await;
                match MemoryConsolidator::consolidate(&mut graph_guard) {
                    Ok(report) => serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => format!("Error consolidating memory: {}", e),
                }
            }

            "rumin_ingest_trace" => {
                let trace_path_str = args.get("trace_path").and_then(|p| p.as_str()).unwrap_or("");
                let session_id = args.get("session_id").and_then(|s| s.as_str());
                let path = PathBuf::from(trace_path_str);

                match self.trace.ingest_file(&path, session_id) {
                    Ok(report) => serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => format!("Error ingesting trace: {}", e),
                }
            }

            _ => format!("Unknown tool: {}", name),
        }
    }
}
