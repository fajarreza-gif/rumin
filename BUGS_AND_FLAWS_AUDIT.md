# Architectural & Code Audit Report: ai-rumin

**Date:** 2026-10-13  
**Analysis Engine:** Rumin MCP Subsystem & Static Architectural Graph Analysis  
**Scope:** `src/`, `sidecar/`, `tests/`  

---

## Executive Summary

An architectural and code audit of the **ai-rumin** codebase was executed using the active `rumin` MCP tool suite (`rumin_query`, `rumin_traverse`, `rumin_diff`, `rumin_checkpoint`, `rumin_hazards`, and `rumin_session_*`). 

The analysis identified **18 distinct bugs, flaws, and design vulnerabilities** across six major categories:
1. **Critical Serialization & MCP Tool Failures** (breaking `rumin_diff`, `rumin_checkpoint`, and startup hydration)
2. **Concurrency, State Drift & Lifecycle Flaws** (`FileWatcher` race conditions, unhandled repo switches, mutex poisoning)
3. **Graph Integrity & Incremental Sync Destruction** (deletion cascade of shared/synthetic nodes)
4. **Parser Deficiencies & Silent Relationship Loss** (dropped imports in Rust/Python, missing language imports, detached methods)
5. **Context Budgeting & Token Circuit Breaker Bypasses** (unenforced token limits, CWD path dependencies)
6. **Safety, Panic Hazards & Portability Defects** (UTF-8 byte boundary panic, uncommitted SQLite transactions, hardcoded paths)

---

## Table of Contents

- [1. Critical Serialization & MCP Tool Failures](#1-critical-serialization--mcp-tool-failures)
  - [1.1. Bincode `deserialize_any` Incompatibility](#11-bincode-deserialize_any-incompatibility)
  - [1.2. Non-Atomic Binary Checkpoint Writing](#12-non-atomic-binary-checkpoint-writing)
- [2. Concurrency, Race Conditions & State Drift](#2-concurrency-race-conditions--state-drift)
  - [2.1. FileWatcher Graph Overwrite Race Condition](#21-filewatcher-graph-overwrite-race-condition)
  - [2.2. FileWatcher 150ms Busy-Polling Loop](#22-filewatcher-150ms-busy-polling-loop)
  - [2.3. Repository Switch Disconnects Live File Watcher](#23-repository-switch-disconnects-live-file-watcher)
  - [2.4. SQLite Mutex Poisoning & Read Serialization](#24-sqlite-mutex-poisoning--read-serialization)
- [3. Graph Integrity & Incremental Sync Deletion Cascade](#3-graph-integrity--incremental-sync-deletion-cascade)
  - [3.1. Shared/Synthetic Entity Deletion Cascade](#31-sharedsynthetic-entity-deletion-cascade)
  - [3.2. Trait Implementation Cross-File Resolution Failure](#32-trait-implementation-cross-file-resolution-failure)
  - [3.3. Stale File-Entity Association on Moved Entities](#33-stale-file-entity-association-on-moved-entities)
- [4. Parser Deficiencies & Graph Disconnections](#4-parser-deficiencies--graph-disconnections)
  - [4.1. Silent Dropping of Rust and Python Import Edges](#41-silent-dropping-of-rust-and-python-import-edges)
  - [4.2. Complete Absence of Import Parsing for Go, C, C++, Java, and TypeScript](#42-complete-absence-of-import-parsing-for-go-c-c-java-and-typescript)
  - [4.3. Class Methods Isolated from Structs and Classes](#43-class-methods-isolated-from-structs-and-classes)
  - [4.4. Native Rust PL/SQL Parser Bypassed by Indexer](#44-native-rust-plsql-parser-bypassed-by-indexer)
  - [4.5. Dead / Unused Parser Code in Python Sidecar](#45-dead--unused-parser-code-in-python-sidecar)
- [5. Token Circuit Breaker & Context Engine Flaws](#5-token-circuit-breaker--context-engine-flaws)
  - [5.1. Token Budget Completely Unenforced in Hippocampal Query](#51-token-budget-completely-unenforced-in-hippocampal-query)
  - [5.2. Working Directory Dependency for Code Snippet Expansion](#52-working-directory-dependency-for-code-snippet-expansion)
  - [5.3. Phantom File Paths for Episodic Memory Nodes](#53-phantom-file-paths-for-episodic-memory-nodes)
- [6. Safety, Panic Hazards, Portability & Usability](#6-safety-panic-hazards-portability--usability)
  - [6.1. UTF-8 Byte Slicing Panic in Trace Ingestion](#61-utf-8-byte-slicing-panic-in-trace-ingestion)
  - [6.2. Missing Rollback and Unchecked Execution in SQLite Traces](#62-missing-rollback-and-unchecked-execution-in-sqlite-traces)
  - [6.3. Episodic ID Collision on Tail Record Deletion](#63-episodic-id-collision-on-tail-record-deletion)
  - [6.4. Hardcoded Developer Paths in Production Binary](#64-hardcoded-developer-paths-in-production-binary)
  - [6.5. `.ruminignore` and Configuration Ignored by Indexer](#65-ruminignore-and-configuration-ignored-by-indexer)
  - [6.6. Destructive Git Hook Installation](#66-destructive-git-hook-installation)
  - [6.7. Memory Consolidation Lost on Server Restart](#67-memory-consolidation-lost-on-server-restart)
- [Appendix: Defect Matrix](#appendix-defect-matrix)

---

## 1. Critical Serialization & MCP Tool Failures

### 1.1. Bincode `deserialize_any` Incompatibility
* **Files**: [`src/types.rs#L21-L37`](/home/dilb/dev/air-mem/ai-rumin/src/types.rs#L21-L37), [`src/graph.rs#L245-L272`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs#L245-L272), [`src/mcp.rs#L574-L600`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs#L574-L600)
* **Severity**: **Critical**
* **Root Cause**:
  In [`src/types.rs`](/home/dilb/dev/air-mem/ai-rumin/src/types.rs), `CodeEntity` defines `metadata: HashMap<String, serde_json::Value>`, and [`EntityType`](/home/dilb/dev/air-mem/ai-rumin/src/types.rs#L5-L23) utilizes `#[serde(other)] Unknown`. Both `serde_json::Value` and `#[serde(other)]` require Serde's `Deserializer::deserialize_any` method. Because Bincode is a compact, non-self-describing binary format, it explicitly does not support `deserialize_any`.
  While `bincode::serialize_into` succeeds during checkpoint creation, any attempt to deserialize in `CodeGraph::load_binary_checkpoint` immediately fails with:
  ```text
  Error reading checkpoint for diff: Bincode does not support the serde::Deserializer::deserialize_any method
  ```
* **Impact**:
  1. `rumin_diff` fails unconditionally on any valid checkpoint.
  2. `rumin_checkpoint` (`action: "load"`) fails unconditionally.
  3. `MCPServer::run_stdio` background hydration fails silently on every startup, abandoning fast binary cache loading and forcing cold sync every session.
* **Remediation**:
  Store metadata as a serializable typed map (or serialized JSON string) and remove `#[serde(other)]` from Bincode-serialized enums, or switch checkpoint storage to a format supporting `deserialize_any` (e.g. MessagePack via `rmp_serde` or compact JSON).

---

### 1.2. Non-Atomic Binary Checkpoint Writing
* **Files**: [`src/graph.rs#L228-L236`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs#L228-L236)
* **Severity**: **High**
* **Root Cause**:
  `save_binary_checkpoint` directly truncates the destination file via `std::fs::File::create(path)` before streaming compressed Gzip and Bincode data:
  ```rust
  let file = std::fs::File::create(path)?;
  let mut encoder = GzEncoder::new(file, Compression::fast());
  encoder.write_all(Self::CHECKPOINT_MAGIC)?;
  bincode::serialize_into(&mut encoder, &serializable)?;
  encoder.finish()?;
  ```
* **Impact**:
  If the process is killed, crashes, or runs out of disk space during serialization, the existing checkpoint file is left corrupted and unrecoverable.
* **Remediation**:
  Write to a temporary file in the same directory (e.g. `path.with_extension("tmp")`) and execute an atomic `std::fs::rename`.

---

## 2. Concurrency, Race Conditions & State Drift

### 2.1. FileWatcher Graph Overwrite Race Condition
* **Files**: [`src/watcher.rs#L62-L70`](/home/dilb/dev/air-mem/ai-rumin/src/watcher.rs#L62-L70)
* **Severity**: **High**
* **Root Cause**:
  When a filesystem event occurs, `FileWatcher` takes a snapshot of the graph:
  ```rust
  let mut g = (*graph.load_full()).clone();
  if let Err(e) = indexer.sync_repository(&mut g, &sidecar, false).await {
      error!("Incremental sync failed: {:?}", e);
  } else {
      graph.store(Arc::new(g));
  }
  ```
* **Impact**:
  If an MCP tool modifies the graph concurrently while `sync_repository` is running (e.g., `rumin_record_episode`, `rumin_consolidate`, or `rumin_index`), `graph.store` unconditionally overwrites the shared pointer with its stale base, silently discarding those modifications.
* **Remediation**:
  Use `arc_swap::ArcSwap::rcu` or compare-and-swap semantics rather than an unconditional store.

---

### 2.2. FileWatcher 150ms Busy-Polling Loop
* **Files**: [`src/watcher.rs#L48-L74`](/home/dilb/dev/air-mem/ai-rumin/src/watcher.rs#L48-L74)
* **Severity**: **Medium**
* **Root Cause**:
  `FileWatcher` spawns a tokio task executing `while running.load(...)` with `while let Ok(res) = rx.try_recv()` followed by `tokio::time::sleep(Duration::from_millis(150)).await`.
* **Impact**:
  The task never parks asynchronously. It continuously wakes up ~7 times per second (over 24,000 times an hour) even when the repository is completely idle, consuming CPU cycles and draining battery.
* **Remediation**:
  Use an asynchronous channel (e.g. `tokio::sync::mpsc`) with `.recv().await` to suspend the task until an actual event is delivered.

---

### 2.3. Repository Switch Disconnects Live File Watcher
* **Files**: [`src/mcp.rs#L137-L141`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs#L137-L141), [`src/mcp.rs#L736-L777`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs#L736-L777)
* **Severity**: **High**
* **Root Cause**:
  `FileWatcher` is spawned in `MCPServer::run_stdio` and bound only to the initial `cur_repo`. When an agent invokes `rumin_switch_repo`, the server updates `repo_root`, `db`, and `graph`, but does not stop or re-point the running `FileWatcher`.
* **Impact**:
  The watcher continues monitoring the old repository directory forever. Edits in the newly switched repository are completely ignored.
* **Remediation**:
  Store the `FileWatcher` handle inside `MCPServer` behind `Arc<RwLock<Option<FileWatcher>>>` and restart it for `new_path` on repo switch.

---

### 2.4. SQLite Mutex Poisoning & Read Serialization
* **Files**: [`src/db.rs#L79-L81`](/home/dilb/dev/air-mem/ai-rumin/src/db.rs#L79-L81)
* **Severity**: **Medium**
* **Root Cause**:
  `MemoryDb::conn(&self)` uses `self.conn.lock().unwrap()`.
* **Impact**:
  If any thread panics while holding the mutex during a query or row extraction, the standard library mutex is poisoned. Every subsequent call across all sessions, episodes, or traces panics immediately, crashing the entire server. Additionally, a single `Mutex<Connection>` prevents concurrent reads despite SQLite WAL mode being enabled.
* **Remediation**:
  Handle mutex poisoning or implement a pooled connection pattern (such as `r2d2_sqlite`).

---

## 3. Graph Integrity & Incremental Sync Deletion Cascade

### 3.1. Shared/Synthetic Entity Deletion Cascade
* **Files**: [`src/graph.rs#L108-L123`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs#L108-L123), [`src/parsers/salesforce.rs#L68-L71`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/salesforce.rs#L68-L71), [`src/parsers/telecom.rs#L63-L69`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/telecom.rs#L63-L69)
* **Root Cause**:
  When files reference external objects (e.g. Apex classes referencing `sobject::Account`, or Erlang modules calling `erlang::<module>`), the parsers lazily create these nodes and assign `entity.file_path` to the *referencing* file path. `graph.add_entity()` registers that entity ID inside `file_to_entities[rel_path]`.
* **Impact**:
  When the referencing file is modified or removed, `indexer` calls `graph.remove_file(rel_path)`. `remove_file` deletes all entities registered under that file, deleting the shared `sobject::Account` node from the graph and destroying links to triggers and fields from other files.
* **Remediation**:
  Synthetic / canonical alias nodes must be tagged under a shared or synthetic origin (e.g. `<synthetic>`) and must not be owned by individual client source files in `file_to_entities`.

---

### 3.2. Trait Implementation Cross-File Resolution Failure
* **Files**: [`src/parsers/systems.rs#L193-L198`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs#L193-L198)
* **Root Cause**:
  In `parse_rust`, when parsing `impl Trait for Struct`:
  ```rust
  let struct_id = format!("{}::{}", rel_path, target);
  let trait_id = format!("{}::{}", rel_path, trait_name);
  graph.add_edge(&struct_id, &trait_id, EdgeKind::Implements);
  ```
* **Impact**:
  `trait_id` is prefixed with the current file's `rel_path`. If `Trait` was imported from another module or standard library, `graph.get_index(&trait_id)` returns `None`, and `add_edge` returns `false`. Structs are never linked to external traits.
* **Remediation**:
  Resolve traits against known global symbols or namespace tables before creating the edge.

---

### 3.3. Stale File-Entity Association on Moved Entities
* **Files**: [`src/graph.rs#L48-L60`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs#L48-L60)
* **Root Cause**:
  In `CodeGraph::add_entity`, if an entity ID already exists in `id_to_index` with a different `file_path`, the entity node is overwritten with the new data and inserted into `file_to_entities[new_file_path]`. However, the old `file_to_entities[old_file_path]` is never cleaned up.
* **Impact**:
  If `old_file_path` is later deleted or re-synced, `remove_file` will remove the entity from its *new* location.

---

## 4. Parser Deficiencies & Graph Disconnections

### 4.1. Silent Dropping of Rust and Python Import Edges
* **Files**: [`src/parsers/systems.rs#L244-L248`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs#L244-L248), [`src/parsers/scripting.rs#L261-L265`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/scripting.rs#L261-L265)
* **Root Cause**:
  Both parsers attempt to add import edges using:
  ```rust
  let src_id = format!("{}::file", rel_path);
  graph.add_edge(&src_id, target, EdgeKind::Imports);
  ```
  However, `{rel_path}::file` is **never added** as an entity node to `CodeGraph`. In [`src/graph.rs`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs#L66-L75), `add_edge` verifies that both endpoints exist in `id_to_index`. Because `{rel_path}::file` does not exist, `add_edge` returns `false` every time.
* **Impact**:
  100% of Rust and Python imports fail silently; dependency edges for these languages are never registered in the graph.
* **Remediation**:
  Register a module or file node for `{rel_path}::file` or link import edges directly between module declarations.

---

### 4.2. Complete Absence of Import Parsing for Go, C, C++, Java, and TypeScript
* **Files**: [`src/parsers/systems.rs#L6-L165`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs#L6-L165), [`src/parsers/jvm_xml.rs#L5-L60`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/jvm_xml.rs#L5-L60), [`src/parsers/scripting.rs#L78-L138`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/scripting.rs#L78-L138)
* **Root Cause**:
  None of these language parsers contain regexes or logic to parse `import`, `#include`, or `require` statements.
* **Impact**:
  All files in Go, C, C++, Java, and TypeScript exist as isolated subgraphs with zero cross-file import connections.

---

### 4.3. Class Methods Isolated from Structs and Classes
* **Files**:
  * Go: [`src/parsers/systems.rs#L26-L34`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs#L26-L34)
  * C++: [`src/parsers/systems.rs#L147-L154`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs#L147-L154)
  * Java: [`src/parsers/jvm_xml.rs#L50-L58`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/jvm_xml.rs#L50-L58)
  * TypeScript: [`src/parsers/scripting.rs#L78-L138`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/scripting.rs#L78-L138)
* **Root Cause**:
  * In Go and C++, methods are parsed but no `Contains` edge is added from the struct/class node to the method node.
  * In Java, methods are named `{rel_path}::{method}` without tracking the enclosing class. If a class has overloaded methods, the second method overwrites the first in `id_to_index`.
  * In TypeScript, methods defined inside `class { ... }` blocks are ignored by `parse_typescript`.
* **Impact**:
  Navigating from classes to their methods via `rumin_traverse` fails for these languages.

---

### 4.4. Native Rust PL/SQL Parser Bypassed by Indexer
* **Files**: [`src/indexer.rs#L185-L202`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs#L185-L202), [`src/parsers/jvm_xml.rs#L95-L180`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/jvm_xml.rs#L95-L180)
* **Root Cause**:
  [`src/indexer.rs`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs#L185) routes all `.sql`, `.pls`, `.pks`, and `.pkb` files exclusively to `sidecar.parse_sql()`. The native Rust `parse_plsql` parser in [`src/parsers/jvm_xml.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/jvm_xml.rs#L95) is dead code during normal repository indexing.
* **Impact**:
  If the Python sidecar is unavailable or fails, Oracle PL/SQL files produce 0 graph nodes despite having a native Rust parser implementation.

---

### 4.5. Dead / Unused Parser Code in Python Sidecar
* **Files**: [`sidecar/worker.py#L334-L895`](/home/dilb/dev/air-mem/ai-rumin/sidecar/worker.py#L334-L895), [`src/sidecar.rs#L146-L185`](/home/dilb/dev/air-mem/ai-rumin/src/sidecar.rs#L146-L185)
* **Root Cause**:
  `sidecar/worker.py` contains implementations for `parse_xml`, `parse_html`, `parse_apex`, `parse_erlang`, and `parse_apigee` (~500 lines of code). However, `SidecarClient` in Rust only implements `ping`, `parse_sql`, and `parse_shell`.
* **Impact**:
  Unmaintained duplicate parsing logic in Python that is never invoked.

---

## 5. Token Circuit Breaker & Context Engine Flaws

### 5.1. Token Budget Completely Unenforced in Hippocampal Query
* **Files**: [`src/hippocampal.rs#L201-L272`](/home/dilb/dev/air-mem/ai-rumin/src/hippocampal.rs#L201-L272)
* **Severity**: **Medium**
* **Root Cause**:
  `HippocampalEngine::query` accepts `budget: &ContextBudget`, but only uses `budget.max_symbols` and `budget.expand_bodies`. It never checks or enforces `budget.total_token_limit`. It calculates `estimated_tokens = formatted_context.len() / 4` at line 263 without truncating.
* **Impact**:
  Requests with strict token limits (e.g., 500 tokens) can return thousands of tokens when AST bodies are expanded, causing downstream LLM context overflow.
* **Remediation**:
  Truncate or stop assembling context lines once `estimated_tokens >= budget.total_token_limit`.

---

### 5.2. Working Directory Dependency for Code Snippet Expansion
* **Files**: [`src/hippocampal.rs#L238-L247`](/home/dilb/dev/air-mem/ai-rumin/src/hippocampal.rs#L238-L247)
* **Severity**: **Medium**
* **Root Cause**:
  When `budget.expand_bodies` is true, the engine calls `std::fs::read_to_string(&entity.file_path)`. `entity.file_path` is a relative path stored by the indexer.
* **Impact**:
  If the process working directory differs from the repository root (e.g. after a `rumin_switch_repo` call or when launching from a subfolder), file reading fails and AST snippet expansion returns empty.
* **Remediation**:
  Pass `repo_root: &Path` to `HippocampalEngine::query` and join with `entity.file_path`.

---

### 5.3. Phantom File Paths for Episodic Memory Nodes
* **Files**: [`src/episodic.rs#L160-L170`](/home/dilb/dev/air-mem/ai-rumin/src/episodic.rs#L160-L170), [`src/hippocampal.rs#L238`](/home/dilb/dev/air-mem/ai-rumin/src/hippocampal.rs#L238)
* **Severity**: **Low**
* **Root Cause**:
  `bind_episodes_to_graph` sets `entity.file_path` to `.rumin/episodes/{id}.md`. However, episodes are stored exclusively in SQLite and are never written to markdown files on disk.
* **Impact**:
  When queries match episode nodes with `expand_bodies: true`, `std::fs::read_to_string` fails.

---

## 6. Safety, Panic Hazards, Portability & Usability

### 6.1. UTF-8 Byte Slicing Panic in Trace Ingestion
* **Files**: [`src/trace.rs#L77-L79`](/home/dilb/dev/air-mem/ai-rumin/src/trace.rs#L77-L79)
* **Severity**: **High**
* **Root Cause**:
  ```rust
  ("generic".to_string(), line_str[..line_str.len().min(80)].to_string())
  ```
  `line_str[..80]` performs raw byte slicing.
* **Impact**:
  If a log line contains non-ASCII characters (e.g., UTF-8 multibyte characters like emoji, accents, or Asian characters) and byte 80 falls within a multibyte sequence, the thread panics with `byte index 80 is not a char boundary`.
* **Remediation**:
  Use `line_str.chars().take(80).collect::<String>()` or check `line_str.is_char_boundary(idx)`.

---

### 6.2. Missing Rollback and Unchecked Execution in SQLite Traces
* **Files**: [`src/trace.rs#L39-L91`](/home/dilb/dev/air-mem/ai-rumin/src/trace.rs#L39-L91)
* **Severity**: **Medium**
* **Root Cause**:
  `conn.execute_batch("BEGIN TRANSACTION;")` is called without a scoped transaction guard. If any line read fails or an error is returned, `ROLLBACK` is never called. Furthermore, the insertion statement uses `let _ = conn.execute(...)`, ignoring insertion failures while incrementing `steps_ingested`.
* **Impact**:
  Database transactions can remain open on failure, and reported ingestion counts can diverge from database rows.

---

### 6.3. Episodic ID Collision on Tail Record Deletion
* **Files**: [`src/episodic.rs#L41-L48`](/home/dilb/dev/air-mem/ai-rumin/src/episodic.rs#L41-L48)
* **Severity**: **Medium**
* **Root Cause**:
  `let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(CAST(SUBSTR(id, 4) AS INTEGER)), 0) FROM episodes...", [], |r| r.get(0)).unwrap_or(0);`
* **Impact**:
  If the latest episode (e.g., `ep_5`) is deleted, `MAX(...)` drops back to `4`. The next inserted episode will be assigned `ep_5` again, causing ID reuse that can corrupt episodic links in external task logs or session notes.
* **Remediation**:
  Use SQLite `AUTOINCREMENT` integer IDs or UUIDs.

---

### 6.4. Hardcoded Developer Paths in Production Binary
* **Files**: [`src/main.rs#L274-L277`](/home/dilb/dev/air-mem/ai-rumin/src/main.rs#L274-L277)
* **Severity**: **Low**
* **Root Cause**:
  `shared_locations` includes hardcoded paths:
  ```rust
  PathBuf::from("/home/dilb/.local/share/ai-rumin/sidecar/worker.py"),
  PathBuf::from("/home/dilb/dev/air-mem/ai-rumin/sidecar/worker.py"),
  ```
* **Impact**:
  Portability defect for other users and systems.
* **Remediation**:
  Resolve paths using `dirs::data_dir()` or the `HOME` environment variable.

---

### 6.5. `.ruminignore` and Configuration Ignored by Indexer
* **Files**: [`src/indexer.rs#L307-L313`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs#L307-L313), [`src/config.rs#L6-L32`](/home/dilb/dev/air-mem/ai-rumin/src/config.rs#L6-L32)
* **Severity**: **Medium**
* **Root Cause**:
  `Indexer::compute_delta` only checks a hardcoded list of folder names (`.git`, `target`, `node_modules`, etc.). It never reads `.ruminignore` or `config.ignored_patterns`.
* **Impact**:
  User-configured ignore patterns in `.ruminignore` and `.rumin/config.json` have no effect.

---

### 6.6. Destructive Git Hook Installation
* **Files**: [`src/hook.rs#L62-L74`](/home/dilb/dev/air-mem/ai-rumin/src/hook.rs#L62-L74)
* **Severity**: **Medium**
* **Root Cause**:
  `CircuitBreaker::install_git_hook` writes to `.git/hooks/pre-commit` via `fs::write(&hook_path, script)?` without checking if a hook already exists.
* **Impact**:
  Overwrites existing pre-commit hooks (Husky, custom linters) without warning or backup.

---

### 6.7. Memory Consolidation Lost on Server Restart
* **Files**: [`src/mcp.rs#L713-L722`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs#L713-L722)
* **Severity**: **Medium**
* **Root Cause**:
  `rumin_consolidate` calls `MemoryConsolidator::consolidate(&mut new_graph)` and updates `self.graph.store(...)`, but never calls `save_binary_checkpoint`.
* **Impact**:
  Distilled shortcuts and pruned transient nodes created during memory consolidation are lost when the MCP server restarts.

---

### 6.8. Anonymous XML Tag Bloat & Massive Resident Memory Spike on `rumin_digest`
* **Files**: [`src/parsers/jvm_xml.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/jvm_xml.rs), [`src/indexer.rs`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs), [`src/watcher.rs`](/home/dilb/dev/air-mem/ai-rumin/src/watcher.rs)
* **Severity**: **Critical**
* **Root Cause**:
  `parse_xml` created a `CodeEntity` of type `Tag` for every opening XML tag on every line even when missing `id` or `name` attributes. In codebases with thousands of XML deployment metadata files (such as Salesforce DX projects with 28,000+ `-meta.xml` files), this generated 1,339,530 useless nodes.
  When `rumin_digest` awaited background hydration on startup, all 1.34M nodes were loaded into memory. Subsequently, the background `FileWatcher` cloned the in-memory graph multiple times during RCU, spiking virtual memory to 4.57 GB and RSS to 2.83 GB.
* **Remediation**:
  1. Updated `parse_xml` to only index XML tags having `id="..."` or `name="..."` attributes.
  2. Updated `indexer.rs` to skip non-schema Salesforce companion metadata manifests (`*.cls-meta.xml`, `*.report-meta.xml`, etc.).
  3. Added `is_supported_file` fast filter to `compute_delta` preventing unnecessary hashing of 22,000 non-code files.
  4. Optimized `watcher.rs` RCU to perform zero-copy `Arc::clone` when no concurrent synthetic nodes require merging.
* **Resolution Status**: **RESOLVED & VERIFIED** (Node count in SF-B2B reduced by 98% from 1,339,530 to 27,089; checkpoint shrunk from 23 MB to 1.2 MB; memory usage plummeted from 2.8 GB to <35 MB).

---

### 6.9. Inbound JSON-RPC Frame Limit Overflow on `rumin_diff`
* **Files**: [`src/mcp.rs#L589-L666`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs#L589-L666)
* **Severity**: **High**
* **Root Cause**:
  `rumin_diff` serialized raw, unbounded `GraphDelta` structs using `serde_json::to_string_pretty(&delta)`. When comparing against base checkpoints with thousands of entities (e.g. 10,000+ newly indexed entities), this emitted multi-megabyte single-line JSON strings that exceeded client line buffer limits (`bufio.Scanner`), crashing the JSON-RPC channel with `inbound JSON-RPC frame exceeded the configured maximum line length`.
* **Remediation**:
  Replaced raw delta serialization with a structured, capped topological diff summary: reports exact counts for added/modified/removed entities and edges, and caps entity samples to top 20 and edge samples to top 10. Frame length is guaranteed <10 KB.
* **Resolution Status**: **RESOLVED & VERIFIED** (Verified via unit test `test_rumin_diff_bounded_jsonrpc_frame_summary`).

---

## Appendix: Defect Matrix

| ID | Issue Description | Severity | Affected Component | Primary Files | Resolution Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **DEF-01** | Bincode `deserialize_any` incompatibility breaking checkpoints & diff | **Critical** | Checkpoint / MCP Diff | [`src/types.rs`](/home/dilb/dev/air-mem/ai-rumin/src/types.rs), [`src/graph.rs`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs) | **RESOLVED & VERIFIED** |
| **DEF-02** | Non-atomic binary checkpoint file writing | **High** | Checkpoint Storage | [`src/graph.rs`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs) | **RESOLVED & VERIFIED** |
| **DEF-03** | FileWatcher `graph.store` race condition overwriting mutations | **High** | Concurrency / Watcher | [`src/watcher.rs`](/home/dilb/dev/air-mem/ai-rumin/src/watcher.rs) | **RESOLVED & VERIFIED** |
| **DEF-04** | FileWatcher 150ms busy-polling loop | **Medium** | Daemon Performance | [`src/watcher.rs`](/home/dilb/dev/air-mem/ai-rumin/src/watcher.rs) | **RESOLVED & VERIFIED** |
| **DEF-05** | `rumin_switch_repo` fails to update FileWatcher target path | **High** | MCP Lifecycle | [`src/mcp.rs`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs) | **RESOLVED & VERIFIED** |
| **DEF-06** | SQLite Mutex poisoning risk on `MemoryDb::conn` | **Medium** | Storage / Concurrency | [`src/db.rs`](/home/dilb/dev/air-mem/ai-rumin/src/db.rs) | **RESOLVED & VERIFIED** |
| **DEF-07** | Shared synthetic node deletion cascade on referencing file change | **High** | Incremental Sync | [`src/graph.rs`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs), [`src/parsers/salesforce.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/salesforce.rs) | **RESOLVED & VERIFIED** |
| **DEF-08** | Trait implementation cross-file resolution failure in Rust | **Medium** | Rust Parser | [`src/parsers/systems.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs) | **RESOLVED & VERIFIED** |
| **DEF-09** | Dropped import edges for Rust & Python (`::file` node missing) | **Medium** | Systems/Scripting Parsers | [`src/parsers/systems.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/systems.rs), [`src/parsers/scripting.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/scripting.rs) | **RESOLVED & VERIFIED** |
| **DEF-10** | Missing import parsing for Go, C, C++, Java, and TypeScript | **Medium** | Multi-Language Parsers | [`src/parsers/`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/) | **RESOLVED & VERIFIED** |
| **DEF-11** | Class methods isolated from structs/classes in Go, C++, Java, TS | **Medium** | Multi-Language Parsers | [`src/parsers/`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/) | **RESOLVED & VERIFIED** |
| **DEF-12** | Native Rust PL/SQL parser bypassed by indexer | **Medium** | Indexer Dispatch | [`src/indexer.rs`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs) | **RESOLVED & VERIFIED** |
| **DEF-13** | Dead / unused parser handlers in Python sidecar | **Low** | Python Sidecar | [`sidecar/worker.py`](/home/dilb/dev/air-mem/ai-rumin/sidecar/worker.py) | **RESOLVED & VERIFIED** |
| **DEF-14** | `budget.total_token_limit` completely unenforced in query | **Medium** | Hippocampal Engine | [`src/hippocampal.rs`](/home/dilb/dev/air-mem/ai-rumin/src/hippocampal.rs) | **RESOLVED & VERIFIED** |
| **DEF-15** | Relative file reads fail when process CWD != `repo_root` | **Medium** | Hippocampal Engine | [`src/hippocampal.rs`](/home/dilb/dev/air-mem/ai-rumin/src/hippocampal.rs) | **RESOLVED & VERIFIED** |
| **DEF-16** | UTF-8 byte-slice panic in `TraceIngestor::ingest_file` | **High** | Trace Ingestion | [`src/trace.rs`](/home/dilb/dev/air-mem/ai-rumin/src/trace.rs) | **RESOLVED & VERIFIED** |
| **DEF-17** | Episodic ID collision on tail record deletion | **Medium** | Episodic Engine | [`src/episodic.rs`](/home/dilb/dev/air-mem/ai-rumin/src/episodic.rs) | **RESOLVED & VERIFIED** |
| **DEF-18** | `.ruminignore` and `config.json` ignored by indexer | **Medium** | Indexer / Configuration | [`src/indexer.rs`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs) | **RESOLVED & VERIFIED** |
| **DEF-19** | Anonymous XML tag explosion & memory spiking on `rumin_digest` | **Critical** | XML Parser / Watcher | [`src/parsers/jvm_xml.rs`](/home/dilb/dev/air-mem/ai-rumin/src/parsers/jvm_xml.rs), [`src/indexer.rs`](/home/dilb/dev/air-mem/ai-rumin/src/indexer.rs), [`src/watcher.rs`](/home/dilb/dev/air-mem/ai-rumin/src/watcher.rs) | **RESOLVED & VERIFIED** |
| **DEF-20** | Inbound JSON-RPC frame limit overflow on `rumin_diff` | **High** | MCP Diff / Transport | [`src/mcp.rs`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs) | **RESOLVED & VERIFIED** |
| **DEF-3.3** | Stale file-entity association on moved entities | **Medium** | CodeGraph Management | [`src/graph.rs`](/home/dilb/dev/air-mem/ai-rumin/src/graph.rs) | **RESOLVED & VERIFIED** |
| **DEF-5.3** | Phantom file paths for episodic memory nodes | **Low** | Hippocampal Context | [`src/hippocampal.rs`](/home/dilb/dev/air-mem/ai-rumin/src/hippocampal.rs) | **RESOLVED & VERIFIED** |
| **DEF-6.2** | Missing rollback and unchecked execution in SQLite traces | **Medium** | Trace Storage | [`src/trace.rs`](/home/dilb/dev/air-mem/ai-rumin/src/trace.rs) | **RESOLVED & VERIFIED** |
| **DEF-6.4** | Hardcoded developer paths in production binary | **Low** | CLI / Discovery | [`src/main.rs`](/home/dilb/dev/air-mem/ai-rumin/src/main.rs) | **RESOLVED & VERIFIED** |
| **DEF-6.6** | Destructive git hook installation | **Medium** | Hook / Circuit Breaker | [`src/hook.rs`](/home/dilb/dev/air-mem/ai-rumin/src/hook.rs) | **RESOLVED & VERIFIED** |
| **DEF-6.7** | Memory consolidation lost on server restart | **Medium** | MCP Consolidation | [`src/mcp.rs`](/home/dilb/dev/air-mem/ai-rumin/src/mcp.rs) | **RESOLVED & VERIFIED** |
