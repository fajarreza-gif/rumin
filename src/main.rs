use ai_rumin::bench::BenchRunner;
use ai_rumin::config::RuminConfig;
use ai_rumin::consolidate::MemoryConsolidator;
use ai_rumin::db::MemoryDb;
use ai_rumin::episodic::EpisodicEngine;
use ai_rumin::graph::CodeGraph;
use ai_rumin::hippocampal::HippocampalEngine;
use ai_rumin::hook::CircuitBreaker;
use ai_rumin::indexer::Indexer;
use ai_rumin::mcp::MCPServer;
use ai_rumin::session::SessionManager;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::trace::TraceIngestor;
use ai_rumin::types::ContextBudget;
use ai_rumin::watcher::FileWatcher;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "ai-rumin",
    author = "air-mem team",
    version = "0.1.0",
    about = "High-performance Rust MCP code-memory engine with Python semantic sidecar"
)]
struct Cli {
    /// Path to repository to analyze (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    repo_path: PathBuf,

    /// Path to Python sidecar worker script
    #[arg(long)]
    sidecar_path: Option<PathBuf>,

    /// Run in standard MCP stdio JSON-RPC 2.0 mode
    #[arg(long, default_value_t = true)]
    stdio: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize .rumin directory and default configuration
    Init,

    /// Index the codebase and print topological summary statistics
    Index,

    /// Return topological architectural digest and scale metrics of the codebase
    Digest,

    /// Incrementally synchronize changed and deleted files
    Sync {
        /// Force complete re-check of files
        #[arg(short, long, default_value_t = false)]
        force: bool,
    },

    /// Save or restore a high-speed binary checkpoint via bincode
    Checkpoint {
        /// Action to perform: 'save' or 'load'
        #[arg(default_value = "save")]
        action: String,

        /// Custom path for the binary checkpoint
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Watch the repository in real-time and trigger incremental syncs
    Watch,

    /// Compute topological graph deltas against a base checkpoint
    Diff {
        /// Checkpoint file path to diff against
        #[arg(short, long)]
        checkpoint: Option<PathBuf>,
    },

    /// Run parser throughput and PPR query latency benchmarks
    Bench,

    /// Calculate blast radius and hazard impact starting from symbol IDs
    Hazards {
        /// Symbol IDs or file paths to inspect
        symbols: Vec<String>,
    },

    /// Verify structural integrity of the graph (dangling references, orphaned interfaces)
    Verify,

    /// Audit architectural invariants and cross-layer isolation rules
    Audit,

    /// Record an episodic debugging lesson or patch trial
    RecordEpisode {
        /// Title of the episode
        #[arg(short, long)]
        title: String,

        /// Detailed content and explanation of outcome
        #[arg(short, long)]
        content: String,

        /// Was the trial successful
        #[arg(long, default_value_t = true)]
        success: bool,

        /// Bound symbol qualified IDs (comma-separated)
        #[arg(long, value_delimiter = ',')]
        symbols: Vec<String>,

        /// Bound file paths (comma-separated)
        #[arg(long, value_delimiter = ',')]
        files: Vec<String>,

        /// Searchable tags (comma-separated)
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },

    /// Search past debugging trials and episodic bug fixes
    SearchEpisodes {
        /// Search keyword or error query
        query: String,

        /// Maximum results to return
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },

    /// Manage conversational task sessions and working notes
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },

    /// Algorithmic memory consolidation: prune dead transient nodes & distill shortcuts
    Consolidate,

    /// Ingest a tool execution trace (JSONL) into memory
    IngestTrace {
        /// Path to JSONL trace file
        trace_path: PathBuf,

        /// Optional session ID to attach trace
        #[arg(short, long)]
        session_id: Option<String>,
    },

    /// Agent safety circuit breaker checks and git hook installation
    Hook {
        #[command(subcommand)]
        action: HookCommands,
    },

    /// Run an associative hippocampal query against the indexed codebase
    Query {
        /// Natural language task, error message, or symbol name
        query: String,

        /// Maximum context token budget
        #[arg(short, long, default_value_t = 1500)]
        budget: usize,
    },

    /// Start the Model Context Protocol (MCP) stdio server
    #[command(alias = "mcp")]
    Serve,
}

#[derive(Subcommand, Debug)]
enum SessionCommands {
    /// Create a new session
    Create { name: String },

    /// List all sessions
    List,

    /// Inspect a session and its working notes
    Inspect { session_id: String },

    /// Append a working note to a session
    Note { session_id: String, content: String },
}

#[derive(Subcommand, Debug)]
enum HookCommands {
    /// Check if reading a line count violates token circuit breaker
    CheckSlice {
        #[arg(short, long)]
        lines: usize,
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Check if a command is hazardous
    CheckCmd {
        command: String,
    },

    /// Install git pre-commit safety verification hook
    Install,
}

async fn load_or_sync_graph(
    repo_root: &std::path::Path,
    sidecar_script: &std::path::Path,
) -> anyhow::Result<CodeGraph> {
    let default_checkpoint = repo_root.join(".rumin/checkpoints/default.bin");
    let mut graph = if default_checkpoint.exists() {
        match CodeGraph::load_binary_checkpoint(&default_checkpoint) {
            Ok(g) => g,
            Err(_) => CodeGraph::new(),
        }
    } else {
        CodeGraph::new()
    };

    let sidecar = SidecarClient::new(sidecar_script);
    let indexer = Indexer::new(repo_root);
    let _ = indexer.sync_repository(&mut graph, &sidecar, false).await?;

    if let Ok(db) = MemoryDb::open(repo_root) {
        let episodic = EpisodicEngine::new(Arc::new(db));
        let _ = episodic.bind_episodes_to_graph(&mut graph);
    }

    Ok(graph)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging (directed to stderr so stdio JSON-RPC is untouched)
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let cli = Cli::parse();
    let repo_root = if cli.repo_path == std::path::Path::new(".") {
        // Walk upwards from current working directory to find nearest .git or .rumin project root
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut cur = cwd.clone();
        let mut root = cwd;
        loop {
            if cur.join(".git").exists() || cur.join(".rumin").exists() {
                root = cur;
                break;
            }
            if !cur.pop() {
                break;
            }
        }
        root.canonicalize().unwrap_or(root)
    } else {
        cli.repo_path.canonicalize().unwrap_or(cli.repo_path)
    };

    // Resolve sidecar worker script relative to executable, known install locations, or current repo
    let sidecar_script = if let Some(p) = cli.sidecar_path {
        p
    } else {
        let exe_sidecar = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("sidecar/worker.py")));

        let shared_locations = [
            PathBuf::from("/home/dilb/.local/share/ai-rumin/sidecar/worker.py"),
            PathBuf::from("/usr/local/share/ai-rumin/sidecar/worker.py"),
            PathBuf::from("/home/dilb/dev/air-mem/ai-rumin/sidecar/worker.py"),
            PathBuf::from("sidecar/worker.py"),
            repo_root.join("sidecar/worker.py"),
        ];

        if let Some(ref p) = exe_sidecar {
            if p.exists() {
                p.clone()
            } else if let Some(found) = shared_locations.iter().find(|loc| loc.exists()) {
                found.clone()
            } else {
                repo_root.join("sidecar/worker.py")
            }
        } else if let Some(found) = shared_locations.iter().find(|loc| loc.exists()) {
            found.clone()
        } else {
            repo_root.join("sidecar/worker.py")
        }
    };

    match cli.command {
        Some(Commands::Init) => {
            let rumin_dir = RuminConfig::init_project(&repo_root)?;
            println!("🚀 Initialized ai-rumin workspace at {:?}", rumin_dir);
            println!("📁 Config written to {:?}", repo_root.join(RuminConfig::CONFIG_FILE));
            println!("📄 Ignore rules written to {:?}", repo_root.join(RuminConfig::IGNORE_FILE));
            Ok(())
        }

        Some(Commands::Index) => {
            println!("🔍 Indexing codebase at: {:?}", repo_root);
            let mut graph = CodeGraph::new();
            let sidecar = SidecarClient::new(&sidecar_script);
            let indexer = Indexer::new(&repo_root);

            let count = indexer.index_repository(&mut graph, &sidecar).await?;
            if let Ok(db) = MemoryDb::open(&repo_root) {
                let episodic = EpisodicEngine::new(Arc::new(db));
                let _ = episodic.bind_episodes_to_graph(&mut graph);
            }
            let stats = graph.summary_stats();

            println!("\n✅ Successfully indexed {} files.", count);
            println!("📊 Knowledge Graph Topology:");
            println!("   - Total Nodes: {}", stats.get("total_nodes").unwrap_or(&0));
            println!("   - Total Edges: {}", stats.get("total_edges").unwrap_or(&0));
            println!("   - Classes/Structs: {}", stats.get("classes").unwrap_or(&0));
            println!("   - Functions/Methods: {}", stats.get("functions").unwrap_or(&0));
            println!("   - SQL Tables: {}", stats.get("sql_tables").unwrap_or(&0));
            println!("   - Make Targets: {}", stats.get("make_targets").unwrap_or(&0));
            Ok(())
        }

        Some(Commands::Digest) => {
            let graph = load_or_sync_graph(&repo_root, &sidecar_script).await?;
            let stats = graph.summary_stats();
            println!("{}", serde_json::to_string_pretty(&stats)?);
            Ok(())
        }

        Some(Commands::Sync { force }) => {
            let default_cp = repo_root.join(".rumin/checkpoints/default.bin");
            let mut graph = if !force && default_cp.exists() {
                CodeGraph::load_binary_checkpoint(&default_cp).unwrap_or_else(|_| CodeGraph::new())
            } else {
                CodeGraph::new()
            };
            let sidecar = SidecarClient::new(&sidecar_script);
            let indexer = Indexer::new(&repo_root);

            let stats = indexer.sync_repository(&mut graph, &sidecar, force).await?;
            if let Ok(db) = MemoryDb::open(&repo_root) {
                let episodic = EpisodicEngine::new(Arc::new(db));
                let _ = episodic.bind_episodes_to_graph(&mut graph);
            }

            println!("\n✅ Incremental sync completed in {}ms", stats.duration_ms);
            println!("   + Added: {}", stats.added);
            println!("   ~ Modified: {}", stats.modified);
            println!("   - Deleted: {}", stats.deleted);
            println!("   = Unchanged: {}", stats.unchanged);
            println!("   📊 Total indexed nodes in graph: {}", stats.total_indexed);
            Ok(())
        }

        Some(Commands::Checkpoint { action, path }) => {
            let default_path = repo_root.join(".rumin/checkpoints/default.bin");
            let checkpoint_path = path.unwrap_or(default_path);

            if let Some(parent) = checkpoint_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            if action == "save" {
                println!("💾 Creating binary checkpoint at: {:?}", checkpoint_path);
                let mut graph = CodeGraph::new();
                let sidecar = SidecarClient::new(&sidecar_script);
                let indexer = Indexer::new(&repo_root);
                indexer.index_repository(&mut graph, &sidecar).await?;
                if let Ok(db) = MemoryDb::open(&repo_root) {
                    let episodic = EpisodicEngine::new(Arc::new(db));
                    let _ = episodic.bind_episodes_to_graph(&mut graph);
                }

                let start = std::time::Instant::now();
                graph.save_binary_checkpoint(&checkpoint_path)?;
                let duration = start.elapsed().as_millis();
                let file_size = std::fs::metadata(&checkpoint_path).map(|m| m.len()).unwrap_or(0);

                println!("✅ Exported {} nodes and {} edges in {}ms ({:.2} KB)",
                    graph.node_count(), graph.edge_count(), duration, file_size as f64 / 1024.0);
            } else {
                println!("📦 Hydrating graph from checkpoint: {:?}", checkpoint_path);
                let start = std::time::Instant::now();
                let graph = CodeGraph::load_binary_checkpoint(&checkpoint_path)?;
                let duration = start.elapsed().as_millis();
                println!("✅ Restored {} nodes and {} edges in {}ms",
                    graph.node_count(), graph.edge_count(), duration);
            }
            Ok(())
        }

        Some(Commands::Watch) => {
            println!("👀 Starting real-time file watcher for {:?}", repo_root);
            let graph = Arc::new(RwLock::new(CodeGraph::new()));
            let sidecar = Arc::new(SidecarClient::new(&sidecar_script));

            {
                let mut g = graph.write().await;
                let indexer = Indexer::new(&repo_root);
                indexer.index_repository(&mut g, &sidecar).await?;
                if let Ok(db) = MemoryDb::open(&repo_root) {
                    let episodic = EpisodicEngine::new(Arc::new(db));
                    let _ = episodic.bind_episodes_to_graph(&mut g);
                }
                println!("✅ Initial index ready: {} nodes.", g.node_count());
            }

            let watcher = FileWatcher::new(&repo_root, graph.clone(), sidecar.clone());
            let _handle = watcher.start()?;
            println!("📡 Watching for file updates. Press Ctrl+C to terminate...");
            tokio::signal::ctrl_c().await?;
            println!("🛑 Shutting down file watcher.");
            Ok(())
        }

        Some(Commands::Diff { checkpoint }) => {
            let default_path = repo_root.join(".rumin/checkpoints/default.bin");
            let cp_path = checkpoint.unwrap_or(default_path);

            println!("📊 Comparing working tree with checkpoint: {:?}", cp_path);
            let base_graph = CodeGraph::load_binary_checkpoint(&cp_path)?;

            let mut current_graph = CodeGraph::new();
            let sidecar = SidecarClient::new(&sidecar_script);
            let indexer = Indexer::new(&repo_root);
            indexer.index_repository(&mut current_graph, &sidecar).await?;

            let delta = base_graph.diff(&current_graph);
            println!("\n=== TOPOLOGICAL GRAPH DIFF ===");
            println!("+ Added Entities: {}", delta.added_entities.len());
            for ent in &delta.added_entities {
                println!("  + [{:?}] {} ({})", ent.entity_type, ent.name, ent.file_path);
            }
            println!("~ Modified Entities: {}", delta.modified_entities.len());
            for ent in &delta.modified_entities {
                println!("  ~ [{:?}] {} ({})", ent.entity_type, ent.name, ent.file_path);
            }
            println!("- Removed Entities: {}", delta.removed_entities.len());
            for id in &delta.removed_entities {
                println!("  - {}", id);
            }
            println!("+ Added Edges: {}", delta.added_edges.len());
            println!("- Removed Edges: {}", delta.removed_edges.len());
            Ok(())
        }

        Some(Commands::Bench) => {
            println!("⚡ Running ai-rumin performance benchmark suite on {:?}", repo_root);
            let sidecar = SidecarClient::new(&sidecar_script);
            let report = BenchRunner::run_benchmark(&repo_root, &sidecar).await?;

            println!("\n╔════════════════════════════════════════════════════════════╗");
            println!("║              AI-RUMIN PERFORMANCE BENCHMARK REPORT          ║");
            println!("╠════════════════════════════════════════════════════════════╣");
            println!("║ Total Files Indexed:           {:<27} ║", report.total_files_indexed);
            println!("║ Total Graph Nodes:             {:<27} ║", report.total_nodes);
            println!("║ Total Graph Edges:             {:<27} ║", report.total_edges);
            println!("║ Indexing Time:                 {:<24} ms ║", report.indexing_time_ms);
            println!("║ Indexing Throughput:           {:<21.2} files/s ║", report.indexing_throughput_files_per_sec);
            println!("║ PPR Queries Executed:          {:<27} ║", report.ppr_queries_executed);
            println!("║ PPR Avg Latency:               {:<24.3} ms ║", report.ppr_avg_latency_ms);
            println!("║ PPR Throughput:                {:<23.1} QPS ║", report.ppr_throughput_qps);
            println!("║ Estimated Graph Memory:        {:<24} KB ║", report.estimated_memory_kb);
            println!("╚════════════════════════════════════════════════════════════╝");
            Ok(())
        }

        Some(Commands::Hazards { symbols }) => {
            if symbols.is_empty() {
                println!("⚠️ No symbols provided. Specify one or more symbol IDs or file paths.");
                return Ok(());
            }

            println!("🚨 Analyzing blast radius hazards for: {:?}", symbols);
            let graph = load_or_sync_graph(&repo_root, &sidecar_script).await?;

            let report = graph.blast_radius(&symbols);
            println!("\n=== BLAST RADIUS HAZARD REPORT ===");
            println!("Total Impacted Downstream Symbols: {}", report.total_impacted);
            println!("\n🔴 HIGH RISK (Hop 1 Direct Callers/Dependents): {}", report.high_risk.len());
            for item in &report.high_risk {
                println!("  - [{:?}] {} (Risk: {:.2}) [{}:{}]", item.entity_type, item.name, item.risk_score, item.file_path, item.hops);
            }
            println!("\n🟡 MEDIUM RISK (Hop 2 Transitive Callers): {}", report.medium_risk.len());
            for item in &report.medium_risk {
                println!("  - [{:?}] {} (Risk: {:.2}) [{}:{}]", item.entity_type, item.name, item.risk_score, item.file_path, item.hops);
            }
            println!("\n🟢 LOW RISK (Hop 3+ Ripple Impact): {}", report.low_risk.len());
            for item in &report.low_risk {
                println!("  - [{:?}] {} (Risk: {:.2}) [{}:{}]", item.entity_type, item.name, item.risk_score, item.file_path, item.hops);
            }
            Ok(())
        }

        Some(Commands::Verify) => {
            println!("🔍 Verifying structural integrity for {:?}", repo_root);
            let graph = load_or_sync_graph(&repo_root, &sidecar_script).await?;

            let issues = graph.verify();
            println!("\n=== STRUCTURAL VERIFICATION REPORT ===");
            if issues.is_empty() {
                println!("✅ Zero structural anomalies found in the graph.");
            } else {
                println!("⚠️ Found {} potential integrity issues:", issues.len());
                for issue in &issues {
                    println!("  [{}] {} (Symbol: {}, File: {})", issue.severity.to_uppercase(), issue.message, issue.symbol_id, issue.file_path);
                }
            }
            Ok(())
        }

        Some(Commands::Audit) => {
            println!("🛡️ Auditing architectural boundary rules for {:?}", repo_root);
            let graph = load_or_sync_graph(&repo_root, &sidecar_script).await?;

            let violations = graph.audit();
            println!("\n=== ARCHITECTURAL AUDIT REPORT ===");
            if violations.is_empty() {
                println!("✅ Zero architectural boundary violations detected.");
            } else {
                println!("🚨 Detected {} boundary violations:", violations.len());
                for v in &violations {
                    println!("  [{}] Rule '{}': {} (From: {} -> To: {})",
                        v.severity.to_uppercase(), v.rule_name, v.message, v.from_symbol, v.to_symbol);
                }
            }
            Ok(())
        }

        Some(Commands::RecordEpisode { title, content, success, symbols, files, tags }) => {
            let db = Arc::new(MemoryDb::open(&repo_root)?);
            let engine = EpisodicEngine::new(db);
            let ep = engine.record_episode(&title, &content, success, &symbols, &files, &tags)?;
            println!("✅ Successfully recorded episode '{}' (ID: {})", ep.title, ep.id);
            Ok(())
        }

        Some(Commands::SearchEpisodes { query, limit }) => {
            let db = Arc::new(MemoryDb::open(&repo_root)?);
            let engine = EpisodicEngine::new(db);
            let matches = engine.search_episodes(&query, limit)?;
            println!("🔍 Found {} matching episodes for query '{}':\n", matches.len(), query);
            for ep in matches {
                println!("--- [{}] {} (Success: {}) ---", ep.id, ep.title, ep.success);
                println!("Content: {}", ep.content);
                if !ep.tags.is_empty() {
                    println!("Tags: {:?}", ep.tags);
                }
                if !ep.bound_symbols.is_empty() {
                    println!("Bound Symbols: {:?}", ep.bound_symbols);
                }
                println!();
            }
            Ok(())
        }

        Some(Commands::Session { action }) => {
            let db = Arc::new(MemoryDb::open(&repo_root)?);
            let session_mgr = SessionManager::new(db);

            match action {
                SessionCommands::Create { name } => {
                    let sess = session_mgr.create_session(&name, None)?;
                    println!("🚀 Created session '{}' (ID: {})", sess.name, sess.id);
                }
                SessionCommands::List => {
                    let sessions = session_mgr.list_sessions()?;
                    println!("📋 Active & Recent Sessions ({}):\n", sessions.len());
                    for s in sessions {
                        println!("  - [{}] {} (Status: {}, Updated: {})", s.id, s.name, s.status, s.updated_at);
                    }
                }
                SessionCommands::Inspect { session_id } => {
                    if let Some(sess) = session_mgr.get_session(&session_id)? {
                        println!("📋 Session: {} ({}) [Status: {}]", sess.name, sess.id, sess.status);
                        let notes = session_mgr.get_working_notes(&session_id)?;
                        println!("📝 Working Notes ({}):", notes.len());
                        for n in notes {
                            println!("  - [{}] {}", n.id, n.content);
                        }
                    } else {
                        println!("❌ Session '{}' not found", session_id);
                    }
                }
                SessionCommands::Note { session_id, content } => {
                    let note = session_mgr.add_working_note(&session_id, &content)?;
                    println!("📝 Added note [{}] to session {}", note.id, note.session_id);
                }
            }
            Ok(())
        }

        Some(Commands::Consolidate) => {
            println!("🧠 Consolidating topological memory for {:?}", repo_root);
            let mut graph = CodeGraph::new();
            let sidecar = SidecarClient::new(&sidecar_script);
            let indexer = Indexer::new(&repo_root);
            indexer.index_repository(&mut graph, &sidecar).await?;

            let report = MemoryConsolidator::consolidate(&mut graph)?;
            println!("\n✅ Memory consolidation completed in {}ms", report.duration_ms);
            println!("   - Pruned dead transient nodes: {}", report.pruned_nodes);
            println!("   - Distilled shortcut edges created: {}", report.shortcut_edges_created);
            println!("   - Graph topology: {} nodes, {} edges", report.total_nodes, report.total_edges);
            Ok(())
        }

        Some(Commands::IngestTrace { trace_path, session_id }) => {
            println!("📥 Ingesting execution trace from {:?}", trace_path);
            let db = Arc::new(MemoryDb::open(&repo_root)?);
            let ingestor = TraceIngestor::new(db);

            let report = ingestor.ingest_file(&trace_path, session_id.as_deref())?;
            println!("\n✅ Ingested {} execution steps in {}ms", report.steps_ingested, report.duration_ms);
            println!("   - Unique files touched: {}", report.unique_files_touched);
            if let Some(sess) = report.session_id {
                println!("   - Associated session: {}", sess);
            }
            Ok(())
        }

        Some(Commands::Hook { action }) => {
            match action {
                HookCommands::CheckSlice { lines, limit } => {
                    let res = CircuitBreaker::check_slice(lines, limit);
                    if res.allowed {
                        println!("✅ Slice check passed: {} lines is within threshold.", lines);
                    } else {
                        println!("🚨 BLOCKED: {}", res.reason.unwrap_or_default());
                        std::process::exit(1);
                    }
                }
                HookCommands::CheckCmd { command } => {
                    let res = CircuitBreaker::check_command(&command);
                    if res.allowed {
                        println!("✅ Command check passed.");
                    } else {
                        println!("🚨 BLOCKED: {}", res.reason.unwrap_or_default());
                        std::process::exit(1);
                    }
                }
                HookCommands::Install => {
                    let path = CircuitBreaker::install_git_hook(&repo_root)?;
                    println!("🛡️ Installed git pre-commit safety hook to {:?}", path);
                }
            }
            Ok(())
        }

        Some(Commands::Query { query, budget }) => {
            let graph = load_or_sync_graph(&repo_root, &sidecar_script).await?;

            let engine = HippocampalEngine::default();
            let context_budget = ContextBudget {
                total_token_limit: budget,
                max_symbols: 10,
                include_signatures: true,
                expand_bodies: false,
            };

            let res = engine.query(&graph, &query, &context_budget);
            println!("{}", res.formatted_context);
            Ok(())
        }

        Some(Commands::Serve) | None => {
            // Default to running MCP stdio server
            let server = MCPServer::new(repo_root, sidecar_script);
            server.run_stdio().await?;
            Ok(())
        }
    }
}
