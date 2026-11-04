#!/usr/bin/env python3
"""Pre-tool hook to forcefully interrupt unnecessary and token-wasteful read operations.

Intercepts unbounded read_file/view_file calls and whole-file shell dumps (cat, less, etc.).
Directs coding agents to RUMIN associative queries and aiedit windowed slices.
"""

from __future__ import annotations

import ast
import json
import re
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

SOURCE_EXTENSIONS = {
    ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".rs",
    ".c", ".cpp", ".cc", ".h", ".hpp", ".java", ".cs",
    ".rb", ".php", ".swift", ".kt", ".scala", ".lua",
    ".vb", ".vbs", ".bas", ".cls", ".sql",
}


def _get_python_outline(file_path: Path) -> str:
    """Extract class and function symbols using Python's standard ast."""
    try:
        content = file_path.read_text(encoding="utf-8", errors="replace")
        tree = ast.parse(content)
        outline = []
        for node in tree.body:
            if isinstance(node, ast.ClassDef):
                end_line = getattr(node, "end_lineno", node.lineno)
                doc = ast.get_docstring(node)
                doc_str = f" -- {doc.splitlines()[0]}" if doc else ""
                outline.append(f"[Lines {node.lineno}-{end_line}] class {node.name}{doc_str}")
                for sub in node.body:
                    if isinstance(sub, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        sub_end = getattr(sub, "end_lineno", sub.lineno)
                        sdoc = ast.get_docstring(sub)
                        sdoc_str = f" -- {sdoc.splitlines()[0]}" if sdoc else ""
                        outline.append(f"  [Lines {sub.lineno}-{sub_end}] def {sub.name}(...){sdoc_str}")
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                end_line = getattr(node, "end_lineno", node.lineno)
                doc = ast.get_docstring(node)
                doc_str = f" -- {doc.splitlines()[0]}" if doc else ""
                outline.append(f"[Lines {node.lineno}-{end_line}] def {node.name}(...){doc_str}")
        return "\n".join(outline)
    except Exception:
        return ""


def get_outline(file_path: Path) -> str:
    """Extract structural outline for a source file."""
    if not file_path.exists():
        return f"[File does not exist: {file_path}]"

    # 1. Native Python AST (zero external dependency, instant)
    if file_path.suffix.lower() == ".py":
        py_outline = _get_python_outline(file_path)
        if py_outline:
            return py_outline

    # 2. Try tree-sitter outline_extractor if available in lsp venv
    for venv_path in [
        "/home/dilb/dev/lsp/.venv/lib/python3.12/site-packages",
        "/home/dilb/dev/lsp",
    ]:
        if venv_path not in sys.path:
            sys.path.insert(0, venv_path)
    try:
        from outline_extractor import extract_outline
        code = file_path.read_bytes()
        outline = extract_outline(str(file_path), code, file_path.suffix)
        if outline and outline.strip():
            return outline.strip()
    except Exception:
        pass

    # 3. Simple regex fallback for classes, functions, and VB subs
    try:
        lines = file_path.read_text(encoding="utf-8", errors="replace").splitlines()
        found = []
        sym_pattern = re.compile(
            r"^\s*(class|def|function|func|fn|pub\s+fn|pub\s+async\s+fn|struct|interface|type|"
            r"public\s+sub|private\s+sub|friend\s+sub|sub|"
            r"public\s+function|private\s+function|friend\s+function|"
            r"public\s+class|private\s+class|friend\s+class|module)\s+([A-Za-z0-9_]+)",
            re.IGNORECASE,
        )
        for idx, line in enumerate(lines, 1):
            m = sym_pattern.match(line)
            if m:
                found.append(f"[Line {idx}] {m.group(1)} {m.group(2)}")
        if found:
            return "\n".join(found[:50])
    except Exception:
        pass

    return f"[Outline generator unavailable for {file_path.name}]"


def inspect_command_line(cmd: str, cwd: str = ".") -> str | None:
    """Check if a shell command attempts to dump whole source code files."""
    if "<<" in cmd:
        return None  # Heredocs permitted

    try:
        parts = re.split(r"(\&\&|\|\||;|\|)", cmd)
    except Exception:
        parts = [cmd]

    for part in parts:
        part = part.strip()
        if part in ("&&", "||", ";", "|"):
            continue
        try:
            tokens = shlex.split(part)
        except Exception:
            tokens = part.split()
        if not tokens:
            continue

        cmd_name = Path(tokens[0]).name.lower()
        if cmd_name in ("cat", "bat", "less", "more", "tac"):
            for arg in tokens[1:]:
                if arg.startswith("-") or arg.startswith(">"):
                    continue
                ext = Path(arg).suffix.lower()
                if ext in SOURCE_EXTENSIONS:
                    target_path = Path(arg) if Path(arg).is_absolute() else (Path(cwd) / arg).resolve()
                    outline = get_outline(target_path)
                    return (
                        f"[Token Circuit Breaker]: Dumping source file '{arg}' with '{cmd_name}' is blocked to preserve context window.\n"
                        f"Recommended RUMIN & Token-Efficient Alternatives:\n"
                        f"1. Query Rumin: `rumin_query(query=\"<symbol_or_concept>\")` or `ai-rumin query \"<query>\"`\n"
                        f"2. Inspect Symbol: `rumin_describe(entity_name=\"<symbol>\")`\n"
                        f"3. Trace Callers/Callees: `rumin_traverse(symbol_id=\"<symbol>\")`\n"
                        f"4. Sliced Read: `read_file(Path='{arg}', StartLine=X, EndLine=Y)` (<= 50 lines) or `aiedit slice {arg} --start X --end Y`\n"
                        f"5. Surgical Edit: `edit_file` or MCP `aiedit_replace_bounded`\n\n"
                        f"--- STRUCTURAL OUTLINE FOR {target_path.name} ---\n"
                        f"{outline}"
                    )
        elif cmd_name in ("head", "tail"):
            n_lines = 10
            for i, arg in enumerate(tokens[1:]):
                if arg.startswith("-n"):
                    if arg == "-n" and i + 1 < len(tokens[1:]):
                        try:
                            n_lines = int(tokens[1:][i + 1])
                        except ValueError:
                            pass
                    else:
                        try:
                            n_lines = int(arg[2:])
                        except ValueError:
                            pass
                elif not arg.startswith("-") and not arg.startswith(">"):
                    ext = Path(arg).suffix.lower()
                    if ext in SOURCE_EXTENSIONS and n_lines > 60:
                        target_path = Path(arg) if Path(arg).is_absolute() else (Path(cwd) / arg).resolve()
                        outline = get_outline(target_path)
                        return (
                            f"[Token Circuit Breaker]: Dumping {n_lines} lines of '{arg}' with '{cmd_name}' is blocked.\n"
                            f"Recommended RUMIN Alternatives:\n"
                            f"1. Query Rumin: `rumin_query(query=\"<task_or_symbol>\")`\n"
                            f"2. Inspect Symbol: `rumin_describe(entity_name=\"<symbol>\")`\n"
                            f"3. Sliced Read: `read_file(Path='{arg}', StartLine=X, EndLine=Y)` (<= 50 lines)\n\n"
                            f"--- STRUCTURAL OUTLINE FOR {target_path.name} ---\n"
                            f"{outline}"
                        )
    return None


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        print(json.dumps({"decision": "allow"}))
        return

    tool_call = payload.get("toolCall", {})
    name = tool_call.get("name", "")
    args = tool_call.get("args", {})

    # 1. Intercept native read_file / view_file on source files
    if name in ("read_file", "view_file"):
        path_str = args.get("Path") or args.get("AbsolutePath") or ""
        start_line = args.get("StartLine")
        end_line = args.get("EndLine")

        ext = Path(path_str).suffix.lower()
        if ext in SOURCE_EXTENSIONS:
            target_path = Path(path_str).resolve()
            total_lines = 0
            if target_path.exists():
                try:
                    with open(target_path, "r", encoding="utf-8", errors="replace") as f:
                        total_lines = sum(1 for _ in f)
                except Exception:
                    pass

            # Only intercept if file is large (> 60 lines) and read is unbounded (> 60 lines or full file)
            if total_lines > 60 and (start_line is None or (end_line is not None and (end_line - start_line > 60))):
                outline = get_outline(target_path)
                reason = (
                    f"[Token Circuit Breaker]: Unbounded read on '{target_path.name}' ({total_lines} lines) is blocked to prevent token waste.\n"
                    f"Recommended RUMIN & Token-Efficient Navigation:\n"
                    f"1. Query Rumin: `rumin_query(query=\"<symbol_or_concept>\")` or `ai-rumin query \"<query>\"`\n"
                    f"2. Inspect Symbol: `rumin_describe(entity_name=\"<symbol>\")` for docstrings, methods, fields, and callers\n"
                    f"3. Trace Callers/Callees: `rumin_traverse(symbol_id=\"<symbol>\")`\n"
                    f"4. Sliced Read: `read_file(Path='{path_str}', StartLine=X, EndLine=Y)` with <= 50 lines, or `aiedit slice {path_str} --start X --end Y`\n"
                    f"5. Surgical Edit: `edit_file` or `aiedit_replace_bounded`\n\n"
                    f"--- STRUCTURAL OUTLINE FOR {target_path.name} ---\n"
                    f"{outline}"
                )
                print(json.dumps({"decision": "deny", "reason": reason}))
                return

    # 2. Intercept shell commands that dump source files (cat, bat, less, etc.)
    elif name == "run_command":
        cmd_line = args.get("CommandLine", "")
        cwd = args.get("Cwd", ".")
        violation = inspect_command_line(cmd_line, cwd)
        if violation:
            print(json.dumps({"decision": "deny", "reason": violation}))
            return

    print(json.dumps({"decision": "allow"}))


if __name__ == "__main__":
    main()
