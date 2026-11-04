# Rumin MCP Server Instructions & Operational Best Practices

Rumin is the primary codebase intelligence, AST knowledge graph, and architectural reasoning server.
Always prioritize using Rumin MCP tools over manual file search or brute-force text tools.

## Golden Navigation Flow

1. **Locate Symbols & Concepts**:
   - Call `rumin_query(query="<concept or symbol>", budget=1500)`.
   - To include direct AST slices in the response, set `expand_bodies: true`.
   - To constrain to specific types (e.g. `class`, `function`, `table`), use `types: ["class", "function"]`.

2. **Inspect Entities**:
   - Call `rumin_describe(entity_name="<symbol>")` to examine docstrings, methods, fields, and callers without dumping source files.

3. **Trace Call Hierarchies & Polyglot Paths**:
   - Call `rumin_traverse(symbol_id="<symbol>", direction="callers"|"callees", depth=1..3)`.
   - Call `rumin_find_path(source_symbol="<source>", target_symbol="<target>")` to trace cross-service or frontend-to-backend dependency routes.

4. **Audit Blast Radius Before Changing Code**:
   - ALWAYS run `rumin_impact(symbol="<symbol>")` before editing or deleting symbols.
   - Review upstream callers and affected unit tests.

5. **Maintain Graph Freshness**:
   - After creating or editing files, call `rumin_index_file(path="<path>")` or `rumin_sync()` to ensure the graph reflects the latest changes.
