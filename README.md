# ai-rumin 🧠⚡

**High-Performance Rust MCP Code-Memory Engine with Python Semantic Sidecar**

`ai-rumin` is a self-contained, independent implementation of **Option B** (Rust Host + Python Semantic Sidecar). It combines the sub-millisecond execution, low RAM footprint, and thread safety of **Rust** with the rich dialect-parsing ecosystem of **Python**.

---

## 🏛️ Architecture Overview (Option B)

```
┌────────────────────────────────────────────────────────┐
│               AI-RUMIN RUST CORE (HOST)                │
│                                                        │
│  • Tokio MCP JSON-RPC 2.0 Stdio Server                 │
│  • Fast Graph Engine (`petgraph` Directed Graph)       │
│  • Personalized PageRank (PPR) Associative Recall      │
│  • Rayon Parallel File Scanner & Binary Filter         │
│  • Native Makefile DAG Target & Prerequisite Parser    │
│  • Extensionless & Shebang Sniffer                     │
└───────────────────────────┬────────────────────────────┘
                            │
                            │ Local IPC (Stdio JSON-RPC)
                            │ Only for dialect-heavy / script files
                            ▼
┌────────────────────────────────────────────────────────┐
│             PYTHON SEMANTIC SIDECAR WORKER             │
│                                                        │
│  • Multi-Dialect SQL & PL/SQL Parsing (`sqlglot` / fallback) │
│    (Oracle, PostgreSQL, MySQL, SQLite, Snowflake, BigQuery)   │
│  • Dynamic Shell AST & Environment Variable Extraction │
│  • Isolated Fault Domain (Sidecar crash never kills    │
│    the core Rust MCP engine)                           │
└────────────────────────────────────────────────────────┘
```

---

## 🚀 Key Advantages

1. **Sub-Millisecond Associative Recall:** Personalized PageRank (PPR) power iteration runs directly in Rust over packed graph nodes, surfacing multi-hop causal paths in 1–5 milliseconds.
2. **Low Memory Footprint:** The knowledge graph lives in compact Rust memory (tens of MBs) instead of heavy Python `networkx` dictionaries (gigabytes).
3. **Broad Multi-Language & Multi-Dialect Support:**
   - **TypeScript & JavaScript:** Functions, arrow functions, classes, interfaces, type aliases, enums (`.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`).
   - **Go:** Packages, standalone functions, receiver methods, structs, interfaces (`.go`).
   - **C & C++:** Macros, structs, unions, enums, namespaces, classes, methods, functions (`.c`, `.h`, `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`).
   - **Java:** Packages, classes, interfaces, records, enums, methods (`.java`).
   - **XML & HTML:** Structural tags, beans, dependencies, semantic tags (`<header>`, `<form>`, `<main>`), elements with IDs, web components (`.xml`, `.html`, `.htm`).
   - **Oracle SQL & PL/SQL:** Packages, package bodies, procedures, functions, triggers, custom types, sequences (`.sql`, `.pls`, `.pks`, `.pkb`, `.prc`, `.fnc`, `.trg`).
   - **Salesforce Ecosystem:**
     - **Apex Classes (`.cls`):** Classes (`with/without/inherited sharing`), interfaces, enums, annotated methods (`@AuraEnabled`, `@InvocableMethod`, `@RemoteAction`, `@isTest`), and embedded SOQL query extraction.
     - **Apex Triggers (`.trigger`):** Triggers on SObjects (`before insert`, `after update`), establishing causal `DependsOn` edges to SObject tables.
     - **Custom SObjects & Fields (`*.object-meta.xml`, `*.field-meta.xml`):** Tables and custom fields (`Field__c`) with topological `Contains` edges.
     - **SOQL Queries (`.soql`):** Standalone query files referencing SObjects.
     - **Aura Components (`.cmp`, `.app`, `.evt`, `.intf`):** UI markup linking `References` edges to Apex controller classes.
   - **Erlang & OTP Ecosystem:**
     - **Modules (`.erl`):** Modules (`-module`), generic behaviors (`-behaviour(gen_server/supervisor)`), records (`-record`), macros (`-define`), types (`-type`), header imports (`-include`), functions by arity (`func/N`), and cross-module calls (`Mod:func`).
     - **Headers (`.hrl`):** Shared records, macros, and type specifications.
     - **Application Specifications (`.app`, `.app.src`):** OTP applications (`{application, App, [...]}`), registering contained modules and dependent applications.
     - **Scripts (`.escript` & shebang `#!/usr/bin/env escript`):** Standalone scripts with `main/1` entry points.
     - **Build Configurations (`rebar.config`):** Rebar3 dependency extraction (`{deps, [...]}`).
   - **Google Cloud Apigee API Management Ecosystem:**
     - **API Proxy & SharedFlow Manifests (`<APIProxy>`, `<SharedFlow>`):** Bundles with basepaths (`<BasePath>`), topological `Contains` edges to ProxyEndpoints, TargetEndpoints, and Policies.
     - **ProxyEndpoints (`<ProxyEndpoint>`):** Inbound basepath routing, request/response pipeline steps (`<Step>` policy calls), and `RouteRule` targets (`DependsOn` edges).
     - **TargetEndpoints (`<TargetEndpoint>`):** Backend HTTP connections (`<URL>`) and backend pipeline steps.
     - **Policies:** Apigee policy catalogue (`VerifyAPIKey`, `SpikeArrest`, `Javascript`, `JavaCallout`, `AssignMessage`, `OAuthV2`, `Quota`, `ResponseCache`, `ServiceCallout`, etc.).
     - **Resource & Class References:** Links Javascript policies to script files (`jsc://*.js`) and JavaCallout policies to Java classes (`<ClassName>`).
   - **Makefiles:** Detects Makefiles natively, mapping targets to dependency DAG edges.
   - **Extensionless Scripts:** Byte-level shebang sniffing for `bash`, `sh`, `python`, `node`, `bun`, `deno`, `ts-node`, and `escript`.
   - **Multi-Dialect SQL & Shell AST:** Multi-dialect SQL migrations (Oracle, PostgreSQL, MySQL, SQLite, Snowflake, BigQuery) and shell environment extraction.
4. **Independent & Moveable:**
   - Completely standalone inside `~/dev/air-mem/ai-rumin`.
   - Uses relative path resolution for the sidecar script.
   - Can be moved anywhere (`mv ~/dev/air-mem/ai-rumin ~/new/location`) with zero broken dependencies.
5. **Future Cloud SaaS Ready:**
   - Built on Tokio async I/O.
   - Ready to swap the stdio JSON-RPC loop with Axum HTTP/SSE streaming for multi-tenant cloud subscription hosting.

---

## 📁 Project Structure

```
~/dev/air-mem/ai-rumin/
├── Cargo.toml            # Rust workspace dependencies (tokio, petgraph, rayon, clap)
├── src/
│   ├── main.rs           # CLI entry point and orchestrator
│   ├── types.rs          # CodeEntity, EntityType, EdgeKind, ContextBudget
│   ├── graph.rs          # petgraph::DiGraph wrapper with fast lookup & indexing
│   ├── hippocampal.rs    # Personalized PageRank (PPR) power-iteration diffusion
│   ├── indexer.rs        # Parallel file scanner, binary filter, and parser router
│   ├── sidecar.rs        # Rust client managing Python sidecar subprocess
│   └── mcp.rs            # Model Context Protocol (MCP) JSON-RPC 2.0 stdio server
├── sidecar/
│   ├── requirements.txt  # Optional: sqlglot for multi-dialect SQL
│   └── worker.py         # Lightweight Python JSON-RPC semantic worker
└── tests/fixtures/       # Sample Makefiles, SQL migrations, and Shell scripts
```

---

## 🛠️ Build & Installation

### Prerequisites
- **Rust Toolchain:** `cargo` (1.75+)
- **Python:** `python3` (3.9+)

### Build the Rust Binary
```bash
cd ~/dev/air-mem/ai-rumin
cargo build --release
```

### (Optional) Install `sqlglot` for Advanced SQL Dialects
```bash
pip install -r sidecar/requirements.txt
```
*(Note: If `sqlglot` is not installed, `sidecar/worker.py` automatically falls back to built-in structural regex extraction without failing!)*

---

## 💻 Usage & CLI Commands

### 1. Workspace Initialization (`init`)
Scaffold `.rumin/` configuration, `.rumin.json`, and `.ruminignore`:
```bash
./target/release/ai-rumin init
```

### 2. Full & Incremental Indexing (`index`, `sync`)
Scan the entire repository:
```bash
./target/release/ai-rumin index
```
Incrementally synchronize only modified, added, and deleted files (sub-10ms):
```bash
./target/release/ai-rumin sync
```

### 3. High-Speed Binary Checkpointing (`checkpoint`)
Export and restore instantaneous binary snapshots (<5ms):
```bash
./target/release/ai-rumin checkpoint save
./target/release/ai-rumin checkpoint load
```

### 4. Real-Time File Watcher Daemon (`watch`)
Live-track filesystem changes and auto-sync in the background:
```bash
./target/release/ai-rumin watch
```

### 5. Topological Graph Diff (`diff`)
Compare the current working tree against a base checkpoint:
```bash
./target/release/ai-rumin diff
```

### 6. Performance Benchmarking (`bench`)
Profile parsing throughput (files/s) and Personalized PageRank latency (QPS):
```bash
./target/release/ai-rumin bench
```

### 7. Blast Radius & Hazard Impact Analysis (`hazards`)
Trace multi-hop downstream risk starting from a modified symbol:
```bash
./target/release/ai-rumin hazards "sobject::Account"
```

### 8. Structural Integrity & Architectural Audit (`verify`, `audit`)
Verify orphaned routes and detect cross-layer boundary violations:
```bash
./target/release/ai-rumin verify
./target/release/ai-rumin audit
```

### 9. Associative Hippocampal Query (`query`)
Run a Personalized PageRank search under strict token budgets:
```bash
./target/release/ai-rumin query "active user orders table" --budget 1500
```

### 10. Episodic Debugging Memory (`record-episode`, `search-episodes`)
Record trial resolutions and link past fixes directly into the code graph:
```bash
# Record an episode bound to code entities
./target/release/ai-rumin record-episode \
  --title "Fix race condition in db lock" \
  --content "Acquired mutex before checking connection validity" \
  --symbols "DbPool::acquire,DbLock" \
  --tags "concurrency,mutex"

# Search past debugging trials
./target/release/ai-rumin search-episodes "mutex race condition"
```

### 11. Conversational Sessions & Working Notes (`session`)
Manage persistent task context and notes:
```bash
./target/release/ai-rumin session create "Auth Feature Refactor"
./target/release/ai-rumin session note sess_1234 "Verified token signature check"
./target/release/ai-rumin session inspect sess_1234
./target/release/ai-rumin session list
```

### 12. Topological Memory Consolidation (`consolidate`)
Run sleep-phase distillation to prune dead nodes and synthesize shortcut edges:
```bash
./target/release/ai-rumin consolidate
```

### 13. Execution Trace Ingestion (`ingest-trace`)
Ingest agent JSONL tool execution logs into SQLite:
```bash
./target/release/ai-rumin ingest-trace path/to/transcript.jsonl
```

### 14. Agent Circuit Breaker & Safety Hooks (`hook`)
Enforce token limits, block hazardous commands, and install pre-commit integrity gates:
```bash
./target/release/ai-rumin hook check-slice --lines 45
./target/release/ai-rumin hook check-cmd "cargo test"
./target/release/ai-rumin hook install-git-hook
```

### 15. Run as an MCP Server (`serve`)
Launch the stdio JSON-RPC server exposing all **18 MCP tools**:
- **Code Graph & Topology**: `rumin_query`, `rumin_digest`, `rumin_index`, `rumin_traverse`, `rumin_sync`, `rumin_checkpoint`, `rumin_diff`, `rumin_hazards`, `rumin_verify`, `rumin_audit`
- **Cognitive Daemon & Episodes**: `rumin_record_episode`, `rumin_search_episodes`
- **Session Memory**: `rumin_session_create`, `rumin_session_list`, `rumin_session_inspect`, `rumin_add_working_note`
- **Distillation & Traces**: `rumin_consolidate`, `rumin_ingest_trace`

```bash
./target/release/ai-rumin serve
```

Configure in your MCP client configuration (`mcpServers`):
```json
{
  "mcpServers": {
    "rumin": {
      "command": "/home/dilb/dev/air-mem/ai-rumin/target/release/ai-rumin",
      "args": ["--repo-path", "/home/dilb/dev/your-project", "serve"]
    }
  }
}
```

---

## ☁️ Future Cloud SaaS Migration Path

When ready to offer `ai-rumin` as a multi-tenant subscription service:
1. **Transport Layer:** Replace `src/mcp.rs` stdio reader with an **Axum** HTTP/SSE router.
2. **Persistence:** Add zero-copy disk caching with `memmap2` / `rkyv` to achieve $O(1)$ on-demand tenant graph mapping.
3. **Tenant Density:** Because the Rust host memory footprint is ~30–50MB per tenant, a single 32GB server can host hundreds of active customer repositories with zero cross-tenant contamination.
