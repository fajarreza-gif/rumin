#!/usr/bin/env python3
"""
Python Semantic Sidecar Worker for ai-rumin.
Provides multi-dialect SQL parsing (via sqlglot) and dynamic Shell AST extraction over JSON-RPC.
"""

import sys
import json
import re
import xml.etree.ElementTree as ET
from typing import Dict, Any, List

# Try importing sqlglot for advanced multi-dialect SQL parsing
try:
    import sqlglot
    from sqlglot import exp
    HAS_SQLGLOT = True
except ImportError:
    HAS_SQLGLOT = False


def extract_sql_entities(file_path: str, content: str, dialect: str = None) -> List[Dict[str, Any]]:
    """Extract tables, views, CTEs, and PL/SQL packages, procedures, functions, triggers, and types."""
    entities = []
    seen_ids = set()

    # 1. Multi-dialect SQL AST via sqlglot if available
    if HAS_SQLGLOT:
        try:
            parsed = sqlglot.parse(content, read=dialect)
            for expression in parsed:
                if not expression:
                    continue

                # CREATE TABLE statements
                if isinstance(expression, exp.Create):
                    table_name = expression.this.name if expression.this else "unknown_table"
                    kind = "view" if expression.kind == "VIEW" else "table"
                    columns = [col.name for col in expression.find_all(exp.ColumnDef)]
                    refs = [t.name for t in expression.find_all(exp.Table) if t.name != table_name]

                    ent_id = f"{file_path}::{table_name}"
                    seen_ids.add(ent_id)
                    entities.append({
                        "id": ent_id,
                        "name": table_name,
                        "entity_type": kind,
                        "file_path": file_path,
                        "line_start": expression.line or 1,
                        "line_end": expression.line or 1,
                        "signature": f"{expression.kind or 'TABLE'} {table_name}",
                        "docstring": None,
                        "metadata": {
                            "dialect": dialect or "generic",
                            "columns": columns,
                            "referenced_tables": refs
                        }
                    })
        except Exception:
            pass

    # 2. Resilient Regex extraction for SQL & PL/SQL (tables, views, packages, procedures, functions, triggers, types)
    lines = content.splitlines()

    table_view_re = re.compile(
        r"CREATE\s+(?:OR\s+REPLACE\s+)?(?:FORCE\s+)?(?:EDITIONABLE\s+)?(TABLE|VIEW|MATERIALIZED\s+VIEW)\s+(?:IF\s+NOT\s+EXISTS\s+)?([`\"']?(\w+)[`\"']?)",
        re.I
    )
    package_re = re.compile(r"CREATE\s+(?:OR\s+REPLACE\s+)?PACKAGE\s+(?:BODY\s+)?([`\"']?(\w+)[`\"']?)", re.I)
    proc_re = re.compile(r"CREATE\s+(?:OR\s+REPLACE\s+)?PROCEDURE\s+([`\"']?(\w+)[`\"']?)", re.I)
    func_re = re.compile(r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([`\"']?(\w+)[`\"']?)", re.I)
    trigger_re = re.compile(r"CREATE\s+(?:OR\s+REPLACE\s+)?TRIGGER\s+([`\"']?(\w+)[`\"']?)", re.I)
    type_re = re.compile(r"CREATE\s+(?:OR\s+REPLACE\s+)?TYPE\s+(?:BODY\s+)?([`\"']?(\w+)[`\"']?)", re.I)
    seq_re = re.compile(r"CREATE\s+SEQUENCE\s+([`\"']?(\w+)[`\"']?)", re.I)

    member_proc_re = re.compile(r"^\s*PROCEDURE\s+([a-zA-Z0-9_#$]+)\s*(?:\(|$)", re.I)
    member_func_re = re.compile(r"^\s*FUNCTION\s+([a-zA-Z0-9_#$]+)\s*(?:\(|$)", re.I)

    current_package = None

    for line_idx, line in enumerate(lines, start=1):
        trimmed = line.trim() if hasattr(line, "trim") else line.strip()
        if trimmed.startswith("--") or trimmed.startswith("/*") or not trimmed:
            continue

        # Package detection (PL/SQL)
        pkg_match = package_re.search(line)
        if pkg_match:
            pkg_name = pkg_match.group(1)
            current_package = pkg_name
            ent_id = f"{file_path}::pkg::{pkg_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": pkg_name,
                    "entity_type": "module",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "type": "package"}
                })
            continue

        # Standalone Procedure (PL/SQL)
        proc_match = proc_re.search(line)
        if proc_match:
            proc_name = proc_match.group(1)
            ent_id = f"{file_path}::{proc_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": proc_name,
                    "entity_type": "function",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "type": "procedure"}
                })
            continue

        # Standalone Function (PL/SQL)
        func_match = func_re.search(line)
        if func_match:
            func_name = func_match.group(1)
            ent_id = f"{file_path}::{func_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": func_name,
                    "entity_type": "function",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "type": "function"}
                })
            continue

        # Trigger (PL/SQL)
        trig_match = trigger_re.search(line)
        if trig_match:
            trig_name = trig_match.group(1)
            ent_id = f"{file_path}::{trig_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": trig_name,
                    "entity_type": "rule",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "type": "trigger"}
                })
            continue

        # Object Type (PL/SQL)
        type_match = type_re.search(line)
        if type_match:
            type_name = type_match.group(1)
            ent_id = f"{file_path}::{type_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": type_name,
                    "entity_type": "struct",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "type": "type"}
                })
            continue

        # Sequence
        seq_match = seq_re.search(line)
        if seq_match:
            seq_name = seq_match.group(1)
            ent_id = f"{file_path}::{seq_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": seq_name,
                    "entity_type": "variable",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "type": "sequence"}
                })
            continue

        # Table or View
        tv_match = table_view_re.search(line)
        if tv_match:
            kind_str = tv_match.group(1).lower()
            table_name = tv_match.group(2)
            ent_id = f"{file_path}::{table_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": table_name,
                    "entity_type": "view" if "view" in kind_str else "table",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": dialect or "generic"}
                })
            continue

        # Member procedure inside a package
        m_proc = member_proc_re.match(line)
        if m_proc and current_package:
            m_name = m_proc.group(1)
            full_name = f"{current_package}.{m_name}"
            ent_id = f"{file_path}::{full_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": full_name,
                    "entity_type": "method",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "package": current_package}
                })
            continue

        # Member function inside a package
        m_func = member_func_re.match(line)
        if m_func and current_package:
            m_name = m_func.group(1)
            full_name = f"{current_package}.{m_name}"
            ent_id = f"{file_path}::{full_name}"
            if ent_id not in seen_ids:
                seen_ids.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": full_name,
                    "entity_type": "method",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": trimmed,
                    "docstring": None,
                    "metadata": {"dialect": "oracle", "package": current_package}
                })
            continue

    return entities


def extract_shell_entities(file_path: str, content: str) -> List[Dict[str, Any]]:
    """Extract functions, exported variables, and source dependencies from Shell scripts."""
    entities = []
    lines = content.splitlines()

    func_regex = re.compile(r"^\s*(?:function\s+)?([a-zA-Z_0-9\-]+)\s*\(\s*\)\s*\{|^\s*function\s+([a-zA-Z_0-9\-]+)\s*\{")
    export_regex = re.compile(r"^\s*export\s+([a-zA-Z_0-9]+)=(.*)")
    source_regex = re.compile(r"^\s*(?:source|\.)\s+([^\s#]+)")

    for line_idx, line in enumerate(lines, start=1):
        # 1. Functions
        func_match = func_regex.match(line)
        if func_match:
            func_name = func_match.group(1) or func_match.group(2)
            entities.append({
                "id": f"{file_path}::{func_name}",
                "name": func_name,
                "entity_type": "function",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": f"{func_name}()",
                "docstring": None,
                "metadata": {"language": "bash"}
            })

        # 2. Exported environment variables
        export_match = export_regex.match(line)
        if export_match:
            var_name = export_match.group(1)
            entities.append({
                "id": f"{file_path}::env::{var_name}",
                "name": var_name,
                "entity_type": "variable",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": f"export {var_name}",
                "docstring": None,
                "metadata": {"language": "bash"}
            })

        # 3. Sourced scripts
        source_match = source_regex.match(line)
        if source_match:
            target_path = source_match.group(1).strip("\"'")
            entities.append({
                "id": f"{file_path}::source::{target_path}",
                "name": target_path,
                "entity_type": "module",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": f"source {target_path}",
                "docstring": None,
                "metadata": {"dependency": target_path, "language": "bash"}
            })

    return entities


def extract_xml_entities(file_path: str, content: str) -> List[Dict[str, Any]]:
    """Extract XML tags, configurations, and elements with IDs or names."""
    entities = []
    lines = content.splitlines()

    # Fast resilient regex extraction
    xml_tag_regex = re.compile(r"<([a-zA-Z0-9_:-]+)(\s+[^>]*)?>")
    id_attr_regex = re.compile(r"""(?:id|name)=["']([^"']+)["']""")

    for line_idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped.startswith("<!--") or stripped.startswith("<?"):
            continue
        for match in xml_tag_regex.finditer(line):
            tag = match.group(1)
            if tag.startswith("/") or tag.startswith("!") or tag.startswith("?"):
                continue
            attrs = match.group(2) or ""
            id_match = id_attr_regex.search(attrs)
            name = f"{tag}#{id_match.group(1)}" if id_match else tag
            entities.append({
                "id": f"{file_path}::{name}::{line_idx}",
                "name": name,
                "entity_type": "tag",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": match.group(0),
                "docstring": None,
                "metadata": {"tag": tag, "language": "xml"}
            })

    return entities


def extract_html_entities(file_path: str, content: str) -> List[Dict[str, Any]]:
    """Extract HTML semantic tags, IDs, classes, and web components."""
    entities = []
    lines = content.splitlines()
    html_tag_regex = re.compile(r"<([a-zA-Z0-9_:-]+)(\s+[^>]*)?>")
    id_attr_regex = re.compile(r"""id=["']([^"']+)["']""")
    class_attr_regex = re.compile(r"""class=["']([^"']+)["']""")

    semantic_tags = {
        "title", "template", "form", "main", "header", "nav",
        "section", "article", "footer", "dialog", "canvas", "svg", "script", "style"
    }

    for line_idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped.startswith("<!--") or stripped.startswith("<!DOCTYPE"):
            continue
        for match in html_tag_regex.finditer(line):
            raw_tag = match.group(1)
            if raw_tag.startswith("/") or raw_tag.startswith("!"):
                continue
            tag = raw_tag.lower()
            attrs = match.group(2) or ""
            id_match = id_attr_regex.search(attrs)
            class_match = class_attr_regex.search(attrs)

            name = None
            if id_match:
                name = f"{tag}#{id_match.group(1)}"
            elif "-" in tag:
                name = tag
            elif tag in semantic_tags:
                name = tag
            elif class_match:
                first_class = class_match.group(1).split()[0]
                name = f"{tag}.{first_class}"

            if name:
                entities.append({
                    "id": f"{file_path}::{name}::{line_idx}",
                    "name": name,
                    "entity_type": "tag",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": match.group(0),
                    "docstring": None,
                    "metadata": {"tag": tag, "language": "html"}
                })

    return entities


def extract_apex_entities(file_path: str, content: str) -> List[Dict[str, Any]]:
    """Extract Apex classes, interfaces, enums, triggers, methods, and embedded SOQL queries."""
    entities = []
    lines = content.splitlines()

    class_re = re.compile(r"^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:public|global|private)\s+(?:(?:with|without|inherited)\s+sharing\s+)?(?:virtual|abstract)?\s*class\s+([a-zA-Z0-9_]+)", re.IGNORECASE)
    iface_re = re.compile(r"^\s*(?:public|global)\s+interface\s+([a-zA-Z0-9_]+)", re.IGNORECASE)
    enum_re = re.compile(r"^\s*(?:public|global|private)\s+enum\s+([a-zA-Z0-9_]+)", re.IGNORECASE)
    trigger_re = re.compile(r"^\s*trigger\s+([a-zA-Z0-9_]+)\s+on\s+([a-zA-Z0-9_]+)\s*\(([^)]+)\)", re.IGNORECASE)
    method_re = re.compile(r"^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:public|global|private|protected)\s+(?:static\s+)?[a-zA-Z0-9_<>\[\],\s]+\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*\{", re.IGNORECASE)
    soql_re = re.compile(r"\[\s*SELECT\s+.*?\s+FROM\s+([a-zA-Z0-9_]+)", re.IGNORECASE)

    current_class = None

    for line_idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped.startswith("//") or stripped.startswith("/*") or not stripped:
            continue

        # 1. Trigger
        trig_match = trigger_re.match(stripped)
        if trig_match:
            trig_name, sobj_name, events = trig_match.group(1), trig_match.group(2), trig_match.group(3)
            entities.append({
                "id": f"{file_path}::trigger::{trig_name}",
                "name": trig_name,
                "entity_type": "rule",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": f"trigger {trig_name} on {sobj_name} ({events.strip()})",
                "docstring": None,
                "metadata": {"sobject": sobj_name, "events": events.strip(), "language": "apex"}
            })
            continue

        # 2. Class
        cls_match = class_re.match(stripped)
        if cls_match:
            cls_name = cls_match.group(1)
            current_class = cls_name
            entities.append({
                "id": f"{file_path}::{cls_name}",
                "name": cls_name,
                "entity_type": "class",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"language": "apex"}
            })
            continue

        # 3. Interface
        iface_match = iface_re.match(stripped)
        if iface_match:
            iface_name = iface_match.group(1)
            entities.append({
                "id": f"{file_path}::{iface_name}",
                "name": iface_name,
                "entity_type": "interface",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"language": "apex"}
            })
            continue

        # 4. Enum
        enum_match = enum_re.match(stripped)
        if enum_match:
            enum_name = enum_match.group(1)
            entities.append({
                "id": f"{file_path}::{enum_name}",
                "name": enum_name,
                "entity_type": "struct",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"language": "apex"}
            })
            continue

        # 5. Method
        method_match = method_re.match(stripped)
        if method_match:
            m_name = method_match.group(1)
            if m_name not in ("if", "while", "for", "switch", "catch", "return"):
                full_name = f"{current_class}.{m_name}" if current_class else m_name
                entities.append({
                    "id": f"{file_path}::{full_name}",
                    "name": full_name,
                    "entity_type": "method",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": stripped,
                    "docstring": None,
                    "metadata": {"class": current_class, "language": "apex"}
                })

        # 6. Embedded SOQL query
        soql_match = soql_re.search(stripped)
        if soql_match:
            sobj = soql_match.group(1)
            entities.append({
                "id": f"{file_path}::soql::{sobj}::{line_idx}",
                "name": f"SOQL::{sobj}",
                "entity_type": "view",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"sobject": sobj, "language": "soql"}
            })

    return entities


def extract_erlang_entities(file_path: str, content: str) -> List[Dict[str, Any]]:
    """Extract Erlang modules, behaviors, records, macros, types, functions, and application specs."""
    entities = []
    lines = content.splitlines()

    mod_re = re.compile(r"^\s*-\s*module\s*\(\s*([a-zA-Z0-9_]+)\s*\)\s*\.")
    beh_re = re.compile(r"^\s*-\s*behaviou?r\s*\(\s*([a-zA-Z0-9_]+)\s*\)\s*\.")
    rec_re = re.compile(r"^\s*-\s*record\s*\(\s*([a-zA-Z0-9_]+)\s*,")
    def_re = re.compile(r"^\s*-\s*define\s*\(\s*([a-zA-Z0-9_]+)")
    type_re = re.compile(r"^\s*-\s*(?:type|opaque)\s+([a-zA-Z0-9_]+)\s*\(")
    func_re = re.compile(r"^([a-z][a-zA-Z0-9_]*)\s*\(([^)]*)\)\s*(?:when\s+[^->]+)?\s*->")
    app_re = re.compile(r"\{\s*application\s*,\s*([a-zA-Z0-9_]+)")

    default_mod = file_path.rsplit("/", 1)[-1].split(".", 1)[0]
    current_module = default_mod
    seen_funcs = set()

    # Check if application specification
    app_match = app_re.search(content)
    if app_match:
        app_name = app_match.group(1)
        entities.append({
            "id": f"{file_path}::application::{app_name}",
            "name": app_name,
            "entity_type": "module",
            "file_path": file_path,
            "line_start": 1,
            "line_end": 1,
            "signature": f"{{application, {app_name}, [...]}}",
            "docstring": None,
            "metadata": {"type": "application", "language": "erlang"}
        })

    for line_idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped.startswith("%") or not stripped:
            continue

        # 1. Module
        m_match = mod_re.match(stripped)
        if m_match:
            current_module = m_match.group(1)
            entities.append({
                "id": f"{file_path}::{current_module}",
                "name": current_module,
                "entity_type": "module",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"language": "erlang"}
            })
            continue

        # 2. Behavior
        b_match = beh_re.match(stripped)
        if b_match:
            beh_name = b_match.group(1)
            entities.append({
                "id": f"{file_path}::behaviour::{beh_name}",
                "name": beh_name,
                "entity_type": "interface",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"behaviour": beh_name, "language": "erlang"}
            })
            continue

        # 3. Record
        r_match = rec_re.match(stripped)
        if r_match:
            rec_name = r_match.group(1)
            entities.append({
                "id": f"{file_path}::record::{rec_name}",
                "name": rec_name,
                "entity_type": "struct",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"record": rec_name, "language": "erlang"}
            })
            continue

        # 4. Macro
        d_match = def_re.match(stripped)
        if d_match:
            macro_name = d_match.group(1)
            entities.append({
                "id": f"{file_path}::macro::{macro_name}",
                "name": macro_name,
                "entity_type": "variable",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"macro": macro_name, "language": "erlang"}
            })
            continue

        # 5. Type
        t_match = type_re.match(stripped)
        if t_match:
            type_name = t_match.group(1)
            entities.append({
                "id": f"{file_path}::type::{type_name}",
                "name": type_name,
                "entity_type": "struct",
                "file_path": file_path,
                "line_start": line_idx,
                "line_end": line_idx,
                "signature": stripped,
                "docstring": None,
                "metadata": {"type": type_name, "language": "erlang"}
            })
            continue

        # 6. Function
        f_match = func_re.match(stripped)
        if f_match:
            f_name = f_match.group(1)
            args_str = f_match.group(2).strip()
            arity = 0 if not args_str else len([a for a in args_str.split(",") if a.strip()])
            full_name = f"{f_name}/{arity}"
            ent_id = f"{file_path}::{full_name}"
            if ent_id not in seen_funcs:
                seen_funcs.add(ent_id)
                entities.append({
                    "id": ent_id,
                    "name": full_name,
                    "entity_type": "function",
                    "file_path": file_path,
                    "line_start": line_idx,
                    "line_end": line_idx,
                    "signature": stripped,
                    "docstring": None,
                    "metadata": {"module": current_module, "arity": arity, "language": "erlang"}
                })

    return entities


def extract_apigee_entities(file_path: str, content: str) -> List[Dict[str, Any]]:
    """Extract Apigee API proxy manifests, proxy endpoints, target endpoints, policies, and script references."""
    entities = []

    manifest_re = re.compile(r"""<APIProxy(?:\s+[^>]*?)?\s+name=["']([^"']+)["']|<SharedFlow(?:\s+[^>]*?)?\s+name=["']([^"']+)["']""")
    proxy_re = re.compile(r"""<ProxyEndpoint(?:\s+[^>]*?)?\s+name=["']([^"']+)["']""")
    target_re = re.compile(r"""<TargetEndpoint(?:\s+[^>]*?)?\s+name=["']([^"']+)["']""")
    basepath_re = re.compile(r"""<BasePath>([^<]+)</BasePath>""")
    url_re = re.compile(r"""<URL>([^<]+)</URL>""")
    step_re = re.compile(r"""<Step>\s*<Name>([^<]+)</Name>""")
    route_target_re = re.compile(r"""<RouteRule[^>]*>[\s\S]*?<TargetEndpoint>([^<]+)</TargetEndpoint>""")
    resource_url_re = re.compile(r"""<ResourceURL>jsc://([^<]+)</ResourceURL>""")
    java_class_re = re.compile(r"""<ClassName>([^<]+)</ClassName>""")
    policy_tag_re = re.compile(r"""<([A-Za-z0-9]+)(?:\s+[^>]*?)?\s+name=["']([^"']+)["']""")

    default_name = file_path.rsplit("/", 1)[-1].split(".", 1)[0]

    # 1. Manifest
    m_match = manifest_re.search(content)
    if m_match:
        proxy_name = m_match.group(1) or m_match.group(2) or default_name
        bp_match = basepath_re.search(content)
        bp = bp_match.group(1) if bp_match else ""
        sig = f"APIProxy {proxy_name} (basepath: {bp})" if bp else f"APIProxy {proxy_name}"
        entities.append({
            "id": f"apiproxy::{proxy_name}",
            "name": proxy_name,
            "entity_type": "module",
            "file_path": file_path,
            "line_start": 1,
            "line_end": 1,
            "signature": sig,
            "docstring": None,
            "metadata": {"type": "apiproxy", "basepath": bp}
        })

        pe_sec = re.search(r"<ProxyEndpoints>([\s\S]*?)</ProxyEndpoints>", content)
        if pe_sec:
            for pe in re.findall(r"<ProxyEndpoint>([^<]+)</ProxyEndpoint>", pe_sec.group(1)):
                pe_name = pe.strip()
                if pe_name:
                    entities.append({
                        "id": f"apiproxy::proxy::{pe_name}",
                        "name": pe_name,
                        "entity_type": "interface",
                        "file_path": file_path,
                        "line_start": 1,
                        "line_end": 1,
                        "signature": f"ProxyEndpoint {pe_name}",
                        "docstring": None,
                        "metadata": {"type": "proxy_endpoint"}
                    })

        te_sec = re.search(r"<TargetEndpoints>([\s\S]*?)</TargetEndpoints>", content)
        if te_sec:
            for te in re.findall(r"<TargetEndpoint>([^<]+)</TargetEndpoint>", te_sec.group(1)):
                te_name = te.strip()
                if te_name:
                    entities.append({
                        "id": f"apiproxy::target::{te_name}",
                        "name": te_name,
                        "entity_type": "interface",
                        "file_path": file_path,
                        "line_start": 1,
                        "line_end": 1,
                        "signature": f"TargetEndpoint {te_name}",
                        "docstring": None,
                        "metadata": {"type": "target_endpoint"}
                    })

        pol_sec = re.search(r"<Policies>([\s\S]*?)</Policies>", content)
        if pol_sec:
            for pol in re.findall(r"<Policy>([^<]+)</Policy>", pol_sec.group(1)):
                pol_name = pol.strip()
                if pol_name:
                    entities.append({
                        "id": f"apiproxy::policy::{pol_name}",
                        "name": pol_name,
                        "entity_type": "rule",
                        "file_path": file_path,
                        "line_start": 1,
                        "line_end": 1,
                        "signature": f"Policy {pol_name}",
                        "docstring": None,
                        "metadata": {"type": "policy"}
                    })

        return entities

    # 2. ProxyEndpoint
    if "<ProxyEndpoint" in content:
        p_match = proxy_re.search(content)
        proxy_name = p_match.group(1) if p_match else default_name
        bp_match = basepath_re.search(content)
        bp = bp_match.group(1) if bp_match else ""
        sig = f"ProxyEndpoint {proxy_name} (basepath: {bp})" if bp else f"ProxyEndpoint {proxy_name}"
        entities.append({
            "id": f"apiproxy::proxy::{proxy_name}",
            "name": proxy_name,
            "entity_type": "interface",
            "file_path": file_path,
            "line_start": 1,
            "line_end": 1,
            "signature": sig,
            "docstring": None,
            "metadata": {"type": "proxy_endpoint", "basepath": bp}
        })
        for step in step_re.findall(content):
            entities.append({
                "id": f"apiproxy::policy::{step}",
                "name": step,
                "entity_type": "rule",
                "file_path": file_path,
                "line_start": 1,
                "line_end": 1,
                "signature": f"Step: {step}",
                "docstring": None,
                "metadata": {"type": "step", "policy": step}
            })
        for rt in route_target_re.findall(content):
            entities.append({
                "id": f"apiproxy::target::{rt}",
                "name": rt,
                "entity_type": "interface",
                "file_path": file_path,
                "line_start": 1,
                "line_end": 1,
                "signature": f"RouteTarget: {rt}",
                "docstring": None,
                "metadata": {"type": "route_target", "target": rt}
            })
        return entities

    # 3. TargetEndpoint
    if "<TargetEndpoint" in content:
        t_match = target_re.search(content)
        target_name = t_match.group(1) if t_match else default_name
        u_match = url_re.search(content)
        url = u_match.group(1) if u_match else ""
        sig = f"TargetEndpoint {target_name} (url: {url})" if url else f"TargetEndpoint {target_name}"
        entities.append({
            "id": f"apiproxy::target::{target_name}",
            "name": target_name,
            "entity_type": "interface",
            "file_path": file_path,
            "line_start": 1,
            "line_end": 1,
            "signature": sig,
            "docstring": None,
            "metadata": {"type": "target_endpoint", "url": url}
        })
        return entities

    # 4. Policy definition
    pol_match = policy_tag_re.search(content)
    if pol_match:
        pol_type = pol_match.group(1)
        pol_name = pol_match.group(2)
        entities.append({
            "id": f"apiproxy::policy::{pol_name}",
            "name": pol_name,
            "entity_type": "rule",
            "file_path": file_path,
            "line_start": 1,
            "line_end": 1,
            "signature": f"Policy {pol_name} ({pol_type})",
            "docstring": None,
            "metadata": {"policy_type": pol_type}
        })
        res_match = resource_url_re.search(content)
        if res_match:
            script_name = res_match.group(1)
            entities.append({
                "id": f"apiproxy::resource::{script_name}",
                "name": script_name,
                "entity_type": "function",
                "file_path": file_path,
                "line_start": 1,
                "line_end": 1,
                "signature": f"Resource: {script_name}",
                "docstring": None,
                "metadata": {"resource_url": f"jsc://{script_name}"}
            })
        cls_match = java_class_re.search(content)
        if cls_match:
            class_name = cls_match.group(1)
            entities.append({
                "id": f"java::{class_name}",
                "name": class_name,
                "entity_type": "class",
                "file_path": file_path,
                "line_start": 1,
                "line_end": 1,
                "signature": f"JavaClass: {class_name}",
                "docstring": None,
                "metadata": {"java_class": class_name}
            })

    return entities


def handle_request(req: Dict[str, Any]) -> Dict[str, Any]:
    method = req.get("method")
    params = req.get("params", {})
    req_id = req.get("id")

    if method == "ping":
        return {"id": req_id, "result": {"status": "pong", "has_sqlglot": HAS_SQLGLOT}, "error": None}

    elif method == "parse_sql":
        file_path = params.get("file_path", "unknown.sql")
        content = params.get("content", "")
        dialect = params.get("dialect")
        entities = extract_sql_entities(file_path, content, dialect)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    elif method == "parse_shell":
        file_path = params.get("file_path", "unknown.sh")
        content = params.get("content", "")
        entities = extract_shell_entities(file_path, content)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    elif method == "parse_xml":
        file_path = params.get("file_path", "unknown.xml")
        content = params.get("content", "")
        entities = extract_xml_entities(file_path, content)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    elif method == "parse_html":
        file_path = params.get("file_path", "unknown.html")
        content = params.get("content", "")
        entities = extract_html_entities(file_path, content)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    elif method == "parse_apex":
        file_path = params.get("file_path", "unknown.cls")
        content = params.get("content", "")
        entities = extract_apex_entities(file_path, content)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    elif method == "parse_erlang":
        file_path = params.get("file_path", "unknown.erl")
        content = params.get("content", "")
        entities = extract_erlang_entities(file_path, content)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    elif method == "parse_apigee":
        file_path = params.get("file_path", "unknown.xml")
        content = params.get("content", "")
        entities = extract_apigee_entities(file_path, content)
        return {"id": req_id, "result": {"entities": entities}, "error": None}

    else:
        return {"id": req_id, "result": None, "error": f"Unknown method: {method}"}


def main():
    """Stdio JSON-RPC loop."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            resp = handle_request(req)
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()
        except Exception as e:
            err_resp = {"id": None, "result": None, "error": str(e)}
            sys.stdout.write(json.dumps(err_resp) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
