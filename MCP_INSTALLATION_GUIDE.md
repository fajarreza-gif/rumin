# ai-rumin: Model Context Protocol (MCP) Installation & Integration Guide

`ai-rumin` is a high-performance Rust code-memory and associative knowledge graph engine implementing the **Model Context Protocol (MCP)** over standard I/O (stdio JSON-RPC 2.0).

Since the binary `ai-rumin` is already installed in your system executable PATH (`/usr/local/bin/ai-rumin`), configuring it in any MCP-compatible AI client (Claude Desktop, Cursor, Antigravity CLI, Windsurf, Cline, Roo Code, Continue) requires only standard JSON configuration.

---

## 🚀 Quick Setup (Generic Configuration)

To enable `ai-rumin` for a target codebase, add it to your client's `mcpServers` configuration block:

```json
{
  "mcpServers": {
    "rumin": {
      "command": "ai-rumin",
      "args": [
        "--repo-path", "/absolute/path/to/your/project",
        "serve"
      ]
    }
  }
}
```

> [!NOTE]
> If `serve` is omitted, `ai-rumin` defaults to running as an MCP stdio server. All diagnostic tracing is automatically routed to `stderr` to ensure pure, uncorrupted JSON-RPC 2.0 communication over `stdout`/`stdin`.

---

## 🛠️ Client-Specific Configuration Guides

### 1. Claude Desktop

Depending on your operating system, open the Claude Desktop configuration file:

- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Linux**: `~/.config/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

Add the `rumin` server configuration:

```json
{
  "mcpServers": {
    "rumin": {
      "command": "ai-rumin",
      "args": [
        "--repo-path", "/home/username/dev/my-project",
        "serve"
      ],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

Restart Claude Desktop. The hammer icon (🛠️) will display the 18 active `rumin_*` tools.

---

### 2. Antigravity CLI / AGY Ecosystem

Add `ai-rumin` to your local MCP server configuration (`~/.gemini/antigravity-cli/mcp/rumin.json` or project-level `.mcp.json`):

```json
{
  "name": "rumin",
  "command": "ai-rumin",
  "args": [
    "--repo-path", "${workspaceFolder}",
    "serve"
  ],
  "transport": "stdio"
}
```

---

### 3. Cursor IDE

In Cursor IDE, navigate to:
**Settings** → **Features** → **MCP Servers** → **Add New MCP Server**

Fill in:
- **Name**: `rumin`
- **Type**: `command`
- **Command**: `ai-rumin --repo-path /absolute/path/to/project serve`

Alternatively, add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "rumin": {
      "command": "ai-rumin",
      "args": ["--repo-path", "${workspaceFolder}", "serve"]
    }
  }
}
```

---

### 4. Cline / Roo Code (VS Code Extensions)

In VS Code, open:
`~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` (or Roo Code equivalent `roo-cline`):

```json
{
  "mcpServers": {
    "rumin": {
      "command": "ai-rumin",
      "args": [
        "--repo-path", "/home/username/dev/my-project",
        "serve"
      ],
      "disabled": false,
      "autoApprove": [
        "rumin_query",
        "rumin_digest",
        "rumin_traverse",
        "rumin_hazards",
        "rumin_search_episodes"
      ]
    }
  }
}
```

---

### 5. Multi-Dialect Python Semantic Sidecar (Optional)

If your codebase contains complex Oracle/Snowflake/BigQuery SQL or dynamic Bash scripts that require Python AST reflection (`sqlglot` / `tree_sitter_python`), specify the path to `worker.py`:

```json
{
  "mcpServers": {
    "rumin": {
      "command": "ai-rumin",
      "args": [
        "--repo-path", "/path/to/project",
        "--sidecar-path", "/path/to/ai-rumin/sidecar/worker.py",
        "serve"
      ]
    }
  }
}
```

---

## 🧰 Complete Catalog of 18 MCP Tools

`ai-rumin` exposes 18 production-grade MCP tools organized into four cognitive layers:

### 1. Code Topology & Recall

| Tool Name | Parameters | Purpose |
| :--- | :--- | :--- |
| `rumin_query` | `query: string`<br>`budget?: number` | Runs Personalized PageRank (PPR) associative graph recall under strict token limits. Surfaces multi-hop causal paths. |
| `rumin_digest` | *(none)* | Returns a compact token (<1000 tokens) architectural digest of codebase subsystems and connectivity hotspots. |
| `rumin_index` | *(none)* | Triggers a full parallel re-indexing of the repository into the Petgraph code graph. |
| `rumin_traverse` | `symbol: string`<br>`direction?: "Incoming"\|"Outgoing"\|"Both"`<br>`depth?: number` | Traverses direct structural neighbors (callers, callees, references, definitions). |
| `rumin_sync` | `force?: boolean` | Performs high-speed incremental file synchronization based on SHA256 hashes and mtimes. |
| `rumin_checkpoint` | `action: "save"\|"load"`<br>`path?: string` | Instantly saves or restores a binary graph checkpoint (< 1ms). |
| `rumin_diff` | `checkpoint_path: string` | Detects structural additions, modifications, and deletions against a base checkpoint. |

### 2. Graph Intelligence & Safety

| Tool Name | Parameters | Purpose |
| :--- | :--- | :--- |
| `rumin_hazards` | `symbols: string[]` | Calculates multi-hop blast radius risk scoring (Hop 1 direct: 0.67, Hop 2 transitive: 0.50). |
| `rumin_verify` | *(none)* | Verifies graph integrity (dangling references, orphaned interfaces, unlinked triggers). |
| `rumin_audit` | *(none)* | Audits architectural boundary invariants and cross-layer isolation rules. |

### 3. Episodic Memory

| Tool Name | Parameters | Purpose |
| :--- | :--- | :--- |
| `rumin_record_episode` | `title: string`<br>`content: string`<br>`symbols?: string[]`<br>`files?: string[]`<br>`tags?: string[]`<br>`success?: boolean` | Records a debugging trial or architectural lesson and links it directly to code entities in the Petgraph. |
| `rumin_search_episodes` | `query: string`<br>`limit?: number` | Searches past debugging solutions and episodic bug fixes by keyword or semantic tag. |

### 4. Task Sessions & Memory Consolidation

| Tool Name | Parameters | Purpose |
| :--- | :--- | :--- |
| `rumin_session_create` | `name: string`<br>`parent_id?: string` | Creates a persistent task session for tracking conversational context across agent restarts. |
| `rumin_session_list` | *(none)* | Lists active and recent task sessions. |
| `rumin_session_inspect` | `session_id: string` | Retrieves session metadata and incremental working notes. |
| `rumin_add_working_note` | `session_id: string`<br>`content: string` | Appends a working note or scratchpad observation to an active session. |
| `rumin_consolidate` | *(none)* | Runs topological memory consolidation: prunes dead transient nodes and synthesizes 2-hop shortcuts. |
| `rumin_ingest_trace` | `trace_path: string`<br>`session_id?: string` | Ingests an agent execution JSONL transcript into memory for retrospective evaluation. |

---

## 🧪 Testing and Verifying Your Installation

You can test that the MCP stdio interface responds correctly by piping a standard JSON-RPC `initialize` handshake into the executable:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0"}}}' | ai-rumin serve
```

**Expected JSON-RPC Response**:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
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
}
```

To list all 18 available tools:
```bash
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | ai-rumin serve
```

---

## 🔧 Operational Tips & Best Practices

1. **Stdout/Stderr Separation**:
   All log lines (`INFO`, `WARN`, `ERROR`) are emitted to `stderr`. The JSON-RPC protocol communicates exclusively over `stdout`. Never pipe stderr into JSON-RPC parsers.
2. **Persistence Storage**:
   `ai-rumin` stores persistent checkpoints and SQLite data in `<repo_root>/.rumin/`. Add `.rumin/` to your `.gitignore` if you do not wish to commit working memory to version control.
3. **Workspace Isolation**:
   Always pass `--repo-path` pointing to the workspace root you are inspecting. Multiple AI sessions can point to different repositories independently with zero cross-tenant contamination.
