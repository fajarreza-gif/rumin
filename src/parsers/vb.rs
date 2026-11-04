use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

/// Check if a line is a remark or comment.
#[inline]
pub fn is_vb_comment_or_empty(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('\'') {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("rem ") || lower == "rem" {
        return true;
    }
    false
}

/// Strip VB/VBA comments (' and Rem) taking string literals into account.
pub fn strip_vb_comment(line: &str) -> &str {
    let mut in_string = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_string = !in_string;
        } else if !in_string {
            if bytes[i] == b'\'' {
                return &line[..i];
            }
            if (bytes[i] == b'R' || bytes[i] == b'r')
                && i + 3 <= bytes.len()
                && (bytes[i + 1] == b'E' || bytes[i + 1] == b'e')
                && (bytes[i + 2] == b'M' || bytes[i + 2] == b'm')
                && (i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b':')
                && (i + 3 == bytes.len() || bytes[i + 3].is_ascii_whitespace())
            {
                return &line[..i];
            }
        }
        i += 1;
    }
    line
}

/// Check if a `.cls` file is Visual Basic (VB6 / VBA Class Module) rather than Salesforce Apex.
pub fn is_vb_cls(content: &str) -> bool {
    // 1. Explicit VB headers or attribute statements
    if content.contains("VERSION 1.0 CLASS")
        || content.contains("Attribute VB_Name")
        || content.contains("Attribute VB_GlobalNameSpace")
        || content.contains("Attribute VB_Creatable")
        || content.contains("Attribute VB_PredeclaredId")
        || content.contains("Attribute VB_Exposed")
    {
        return true;
    }

    // 2. Starts with VERSION or contains VB Begin ... End blocks
    let trimmed = content.trim_start();
    if trimmed.starts_with("VERSION ") {
        return true;
    }

    // 3. Salesforce Apex exclusive markers
    let has_apex_markers = content.contains("@isTest")
        || content.contains("with sharing")
        || content.contains("without sharing")
        || content.contains("inherited sharing");
    if has_apex_markers {
        return false;
    }

    // 4. VB Keywords check (Option Explicit, End Sub, End Function, Property Get/Let/Set)
    let lower = content.to_ascii_lowercase();
    let has_vb_markers = lower.contains("option explicit")
        || lower.contains("end sub")
        || lower.contains("end function")
        || lower.contains("property get")
        || lower.contains("property let")
        || lower.contains("property set");

    let has_apex_class_decl = Regex::new(r"(?i)\b(?:public|global|private)\s+(?:virtual|abstract)?\s*class\b")
        .unwrap()
        .is_match(content);

    has_vb_markers && !has_apex_class_decl
}

/// Logical line representation after resolving line continuations ('_').
#[derive(Debug, Clone)]
struct LogicalLine {
    text: String,
    start_line: usize,
    end_line: usize,
}

/// Preprocess raw content lines into logical lines, joining lines ending with underscore.
fn preprocess_logical_lines(content: &str) -> Vec<LogicalLine> {
    let mut logical_lines = Vec::new();
    let mut current_buf = String::new();
    let mut start_line = 1;
    let mut in_continuation = false;

    for (idx, raw_line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let clean = strip_vb_comment(raw_line).trim_end();

        if clean.trim().is_empty() {
            if in_continuation {
                continue;
            }
            continue;
        }

        // Check for line continuation: space followed by underscore at end of line
        let trimmed = clean.trim_end();
        if trimmed.ends_with(" _") || trimmed == "_" {
            let without_underscore = if trimmed == "_" {
                ""
            } else {
                &trimmed[..trimmed.len() - 1].trim_end()
            };

            if !in_continuation {
                start_line = line_num;
                current_buf.clear();
                current_buf.push_str(without_underscore);
                in_continuation = true;
            } else {
                current_buf.push(' ');
                current_buf.push_str(without_underscore);
            }
        } else if in_continuation {
            current_buf.push(' ');
            current_buf.push_str(clean.trim());
            logical_lines.push(LogicalLine {
                text: current_buf.clone(),
                start_line,
                end_line: line_num,
            });
            current_buf.clear();
            in_continuation = false;
        } else {
            logical_lines.push(LogicalLine {
                text: clean.to_string(),
                start_line: line_num,
                end_line: line_num,
            });
        }
    }

    if in_continuation && !current_buf.is_empty() {
        let end_line = start_line;
        logical_lines.push(LogicalLine {
            text: current_buf,
            start_line,
            end_line,
        });
    }

    logical_lines
}

/// Scope context tracking for nested blocks (Namespace, Class, Module, Interface, Structure).
#[derive(Debug, Clone)]
struct ScopeContext {
    id: String,
    name: String,
    entity_type: EntityType,
    kind: ScopeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScopeKind {
    Namespace,
    Class,
    Module,
    Interface,
    Structure,
    Enum,
    Routine,
}

/// Parse Visual Basic (.vb, .vba, .bas, .frm, .ctl, .vbs, and VB .cls) source code into AST entities and edges.
pub fn parse_vb(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let path = Path::new(rel_path);
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("UnknownModule");

    // Regular expressions for Visual Basic constructs (case-insensitive)
    let vb_name_attr_re = Regex::new(r#"(?i)^\s*Attribute\s+VB_Name\s*=\s*"([^"]+)""#).unwrap();
    let namespace_re = Regex::new(r"(?i)^\s*Namespace\s+([a-zA-Z0-9_\.]+)").unwrap();
    let imports_re = Regex::new(r"(?i)^\s*Imports\s+(?:([a-zA-Z0-9_]+)\s*=\s*)?([a-zA-Z0-9_\.]+)").unwrap();
    let module_re = Regex::new(r"(?i)^\s*(?:(?:Public|Friend|Private)\s+)?Module\s+([a-zA-Z0-9_]+)").unwrap();
    let class_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Protected|Partial|MustInherit|NotInheritable|Static)\s+)*Class\s+([a-zA-Z0-9_]+)(?:\s*\(Of\s+[^)]+\))?").unwrap();
    let iface_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Protected)\s+)*Interface\s+([a-zA-Z0-9_]+)").unwrap();
    let struct_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Protected)\s+)*Structure\s+([a-zA-Z0-9_]+)").unwrap();
    let type_udt_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Global)\s+)*Type\s+([a-zA-Z0-9_]+)").unwrap();
    let enum_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend)\s+)*Enum\s+([a-zA-Z0-9_]+)").unwrap();
    
    // Inheritance and interface implementation
    let inherits_re = Regex::new(r"(?i)^\s*Inherits\s+([a-zA-Z0-9_\.]+)").unwrap();
    let implements_re = Regex::new(r"(?i)^\s*Implements\s+([a-zA-Z0-9_\.,\s]+)").unwrap();

    // Procedures and methods
    let sub_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Protected|Shared|Static|Overrides|Overridable|MustOverride|Async|Default)\s+)*Sub\s+([a-zA-Z0-9_]+)\s*(?:\((.*?)\))?(?:\s+Implements\s+([a-zA-Z0-9_\.]+))?(?:\s+Handles\s+([a-zA-Z0-9_\.,\s]+))?").unwrap();
    let func_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Protected|Shared|Static|Overrides|Overridable|MustOverride|Async)\s+)*Function\s+([a-zA-Z0-9_]+)\s*(?:\((.*?)\))?(?:\s+As\s+([a-zA-Z0-9_\.\(\)]+))?(?:\s+Implements\s+([a-zA-Z0-9_\.]+))?").unwrap();
    let prop_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend|Protected|Shared|Overrides|Overridable|ReadOnly|WriteOnly|Default)\s+)*Property\s+(?:(Get|Let|Set)\s+)?([a-zA-Z0-9_]+)\s*(?:\((.*?)\))?(?:\s+As\s+([a-zA-Z0-9_\.\(\)]+))?").unwrap();
    
    // Win32 API DLL P/Invoke Declare
    let declare_re = Regex::new(r#"(?i)^\s*(?:(?:Public|Private)\s+)?Declare\s+(?:PtrSafe\s+)?(Sub|Function)\s+([a-zA-Z0-9_]+)\s+Lib\s+"([^"]+)"(?:\s+Alias\s+"([^"]+)")?\s*(?:\((.*?)\))?(?:\s+As\s+([a-zA-Z0-9_\.\(\)]+))?"#).unwrap();

    // Events and Fields
    let event_re = Regex::new(r"(?i)^\s*(?:(?:Public|Private|Friend)\s+)?Event\s+([a-zA-Z0-9_]+)\s*(?:\((.*?)\))?").unwrap();
    let field_re = Regex::new(r"(?i)^\s*(?:(Public|Private|Friend|Dim|Const|Global)\s+)(?:(Shared|ReadOnly|Static)\s+)?([a-zA-Z0-9_]+)\s+As\s+(?:New\s+)?([a-zA-Z0-9_\.]+)").unwrap();

    // COM / ActiveX Instantiations & References
    let com_create_re = Regex::new(r#"(?i)(?:CreateObject|GetObject|WScript\.CreateObject)\s*\(\s*"([a-zA-Z0-9_\.]+)"\s*\)"#).unwrap();
    let new_com_re = Regex::new(r"(?i)\b(?:Set\s+[a-zA-Z0-9_]+\s*=\s*)?New\s+([a-zA-Z0-9_]+\.[a-zA-Z0-9_]+)").unwrap();

    // Procedure Calls
    let call_stmt_re = Regex::new(r"(?i)\bCall\s+([a-zA-Z0-9_]+)").unwrap();
    let method_call_re = Regex::new(r"(?i)\.([a-zA-Z0-9_]+)\s*\(").unwrap();

    // VB6 GUI Form Controls (e.g. Begin VB.CommandButton btnSubmit)
    let gui_control_re = Regex::new(r"(?i)^\s*Begin\s+([a-zA-Z0-9_\.]+)\s+([a-zA-Z0-9_]+)").unwrap();

    let total_lines = content.lines().count().max(1);
    let file_id = format!("{}::file", rel_path);
    let mut file_entity = CodeEntity::new(&file_id, rel_path, EntityType::Module, rel_path, 1, total_lines);
    file_entity.signature = Some(format!("file {}", rel_path));
    
    // Determine primary dialect
    let dialect = if extension == "vb" {
        "vb.net"
    } else if extension == "vbs" {
        "vbscript"
    } else if extension == "frm" {
        "vb6/form"
    } else if extension == "ctl" {
        "vb6/usercontrol"
    } else if extension == "cls" {
        "vba/class"
    } else {
        "vba"
    };
    file_entity.metadata.insert("dialect".to_string(), dialect.to_string());
    graph.add_entity(file_entity);

    // Extract Attribute VB_Name if present in VB6/VBA files
    let mut vb_attribute_name = None;
    for line in content.lines().take(50) {
        if let Some(caps) = vb_name_attr_re.captures(line) {
            vb_attribute_name = Some(caps.get(1).unwrap().as_str().to_string());
            break;
        }
    }

    let default_module_name = vb_attribute_name.unwrap_or_else(|| file_stem.to_string());

    // Preprocess logical lines (handles multi-line signatures with ' _')
    let logical_lines = preprocess_logical_lines(content);

    let mut scope_stack: Vec<ScopeContext> = Vec::new();
    let mut gui_depth: usize = 0;
    let mut pending_docstring: Vec<String> = Vec::new();
    let mut defined_routines: HashSet<String> = HashSet::new();
    let mut calls_to_resolve: Vec<(String, String)> = Vec::new();

    // Helper to resolve current container (Class, Module, Interface, Structure, or fallback default module)
    let get_current_container = |stack: &[ScopeContext]| -> (String, String, EntityType) {
        for ctx in stack.iter().rev() {
            if ctx.kind == ScopeKind::Class
                || ctx.kind == ScopeKind::Module
                || ctx.kind == ScopeKind::Interface
                || ctx.kind == ScopeKind::Structure
            {
                return (ctx.id.clone(), ctx.name.clone(), ctx.entity_type.clone());
            }
        }
        let fallback_id = format!("{}::module::{}", rel_path, default_module_name);
        (fallback_id, default_module_name.clone(), EntityType::Module)
    };

    // Helper to resolve current routine (Sub, Function, Property) or fallback to container
    let get_current_routine_or_container = |stack: &[ScopeContext]| -> String {
        if let Some(routine) = stack.iter().rev().find(|s| s.kind == ScopeKind::Routine) {
            return routine.id.clone();
        }
        for ctx in stack.iter().rev() {
            if ctx.kind == ScopeKind::Class
                || ctx.kind == ScopeKind::Module
                || ctx.kind == ScopeKind::Interface
                || ctx.kind == ScopeKind::Structure
            {
                return ctx.id.clone();
            }
        }
        format!("{}::module::{}", rel_path, default_module_name)
    };

    // If VB6/VBA file (.bas, .cls, .frm, .ctl), create module/class node if not VB.NET
    let is_vb_net = extension == "vb";
    if !is_vb_net {
        let (root_ent_type, root_kind) = if extension == "cls" {
            (EntityType::Class, ScopeKind::Class)
        } else {
            (EntityType::Module, ScopeKind::Module)
        };
        let root_id = format!("{}::{}", rel_path, default_module_name);
        let mut root_ent = CodeEntity::new(&root_id, &default_module_name, root_ent_type.clone(), rel_path, 1, total_lines);
        root_ent.signature = Some(format!("{} {}", if root_ent_type == EntityType::Class { "Class" } else { "Module" }, default_module_name));
        root_ent.metadata.insert("dialect".to_string(), dialect.to_string());
        graph.add_entity(root_ent);
        graph.add_edge(&file_id, &root_id, EdgeKind::Contains);

        scope_stack.push(ScopeContext {
            id: root_id,
            name: default_module_name.clone(),
            entity_type: root_ent_type,
            kind: root_kind,
        });
    }

    // Process raw lines for comments/docstrings in parallel with logical lines
    let mut raw_line_idx = 0;
    let raw_lines: Vec<&str> = content.lines().collect();

    for lline in &logical_lines {
        let trimmed = lline.text.trim();
        if trimmed.is_empty() {
            pending_docstring.clear();
            continue;
        }

        // Advance raw lines to accumulate docstring comments preceding this logical line
        while raw_line_idx < lline.start_line - 1 && raw_line_idx < raw_lines.len() {
            let raw_trimmed = raw_lines[raw_line_idx].trim();
            if raw_trimmed.starts_with('\'') {
                pending_docstring.push(raw_trimmed.trim_start_matches('\'').trim().to_string());
            } else if raw_trimmed.to_ascii_lowercase().starts_with("rem ") {
                pending_docstring.push(raw_trimmed[4..].trim().to_string());
            } else if raw_trimmed.is_empty() {
                pending_docstring.clear();
            }
            raw_line_idx += 1;
        }
        raw_line_idx = lline.end_line;

        let docstring_opt = if !pending_docstring.is_empty() {
            let doc = pending_docstring.join("\n");
            pending_docstring.clear();
            Some(doc)
        } else {
            None
        };

        // 1. VB6 / UserForm GUI Definition Blocks (Begin VB.Form ... End)
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("begin ") || lower == "begin" {
            gui_depth += 1;
            if let Some(caps) = gui_control_re.captures(trimmed) {
                let control_type = caps.get(1).unwrap().as_str();
                let control_name = caps.get(2).unwrap().as_str();
                let ctrl_id = format!("{}::control::{}", rel_path, control_name);
                let mut ctrl_ent = CodeEntity::new(&ctrl_id, control_name, EntityType::Tag, rel_path, lline.start_line, lline.end_line);
                ctrl_ent.signature = Some(trimmed.to_string());
                ctrl_ent.metadata.insert("control_type".to_string(), control_type.to_string());
                graph.add_entity(ctrl_ent);
                let (container_id, _, _) = get_current_container(&scope_stack);
                graph.add_edge(&container_id, &ctrl_id, EdgeKind::Contains);
            }
            continue;
        }

        if gui_depth > 0 && lower == "end" {
            gui_depth -= 1;
            continue;
        }

        if gui_depth > 0 {
            // Inside GUI layout block - skip layout properties
            continue;
        }

        // 2. Block Closures
        if lower.starts_with("end ") || lower == "end" {
            let tokens: Vec<&str> = lower.split_whitespace().collect();
            if tokens.len() >= 2 {
                match tokens[1] {
                    "sub" | "function" | "property" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Routine) {
                            scope_stack.truncate(pos);
                        }
                    }
                    "class" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Class) {
                            scope_stack.truncate(pos);
                        }
                    }
                    "module" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Module) {
                            scope_stack.truncate(pos);
                        }
                    }
                    "interface" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Interface) {
                            scope_stack.truncate(pos);
                        }
                    }
                    "structure" | "type" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Structure) {
                            scope_stack.truncate(pos);
                        }
                    }
                    "enum" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Enum) {
                            scope_stack.truncate(pos);
                        }
                    }
                    "namespace" => {
                        if let Some(pos) = scope_stack.iter().rposition(|s| s.kind == ScopeKind::Namespace) {
                            scope_stack.truncate(pos);
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        // 3. Namespaces (VB.NET)
        if let Some(caps) = namespace_re.captures(trimmed) {
            let ns_name = caps.get(1).unwrap().as_str();
            let ns_id = format!("{}::ns::{}", rel_path, ns_name);
            let mut ns_ent = CodeEntity::new(&ns_id, ns_name, EntityType::Module, rel_path, lline.start_line, lline.end_line);
            ns_ent.signature = Some(trimmed.to_string());
            ns_ent.docstring = docstring_opt;
            graph.add_entity(ns_ent);
            graph.add_edge(&file_id, &ns_id, EdgeKind::Contains);

            scope_stack.push(ScopeContext {
                id: ns_id,
                name: ns_name.to_string(),
                entity_type: EntityType::Module,
                kind: ScopeKind::Namespace,
            });
            continue;
        }

        // 4. Imports (VB.NET)
        if let Some(caps) = imports_re.captures(trimmed) {
            let target_ns = caps.get(2).unwrap().as_str();
            let alias = caps.get(1).map(|m| m.as_str());
            let ext_id = format!("dotnet::{}", target_ns);
            if graph.get_entity(&ext_id).is_none() {
                let mut ext = CodeEntity::new(&ext_id, target_ns, EntityType::Module, "<external>", lline.start_line, lline.end_line);
                ext.signature = Some(format!("Imports {}", target_ns));
                graph.add_entity(ext);
            }
            graph.add_edge(&file_id, &ext_id, EdgeKind::Imports);
            if let Some(al) = alias {
                let alias_id = format!("{}::alias::{}", rel_path, al);
                let mut al_ent = CodeEntity::new(&alias_id, al, EntityType::Module, rel_path, lline.start_line, lline.end_line);
                al_ent.signature = Some(trimmed.to_string());
                graph.add_entity(al_ent);
                graph.add_edge(&alias_id, &ext_id, EdgeKind::References);
            }
            continue;
        }

        // 5. Modules (VB.NET)
        if let Some(caps) = module_re.captures(trimmed) {
            let mod_name = caps.get(1).unwrap().as_str();
            let mod_id = format!("{}::module::{}", rel_path, mod_name);
            let mut mod_ent = CodeEntity::new(&mod_id, mod_name, EntityType::Module, rel_path, lline.start_line, lline.end_line);
            mod_ent.signature = Some(trimmed.to_string());
            mod_ent.docstring = docstring_opt;
            graph.add_entity(mod_ent);

            let (parent_id, _, _) = get_current_container(&scope_stack);
            graph.add_edge(&parent_id, &mod_id, EdgeKind::Contains);

            scope_stack.push(ScopeContext {
                id: mod_id,
                name: mod_name.to_string(),
                entity_type: EntityType::Module,
                kind: ScopeKind::Module,
            });
            continue;
        }

        // 6. Classes
        if let Some(caps) = class_re.captures(trimmed) {
            let cls_name = caps.get(1).unwrap().as_str();
            let cls_id = format!("{}::class::{}", rel_path, cls_name);
            let mut cls_ent = CodeEntity::new(&cls_id, cls_name, EntityType::Class, rel_path, lline.start_line, lline.end_line);
            cls_ent.signature = Some(trimmed.to_string());
            cls_ent.docstring = docstring_opt;
            graph.add_entity(cls_ent);

            let (parent_id, _, _) = get_current_container(&scope_stack);
            graph.add_edge(&parent_id, &cls_id, EdgeKind::Contains);

            // Check for inline Inherits or Implements
            if let Some(inh_idx) = lower.find(" inherits ") {
                let rest = &trimmed[inh_idx + 10..].trim();
                let base_name = rest.split_whitespace().next().unwrap_or("").trim_end_matches(',');
                if !base_name.is_empty() {
                    let base_id = format!("{}::class::{}", rel_path, base_name);
                    graph.add_edge(&cls_id, &base_id, EdgeKind::DependsOn);
                }
            }
            if let Some(imp_idx) = lower.find(" implements ") {
                let rest = &trimmed[imp_idx + 12..].trim();
                for iface_part in rest.split(',') {
                    let iface_name = iface_part.trim().split_whitespace().next().unwrap_or("");
                    if !iface_name.is_empty() {
                        let iface_id = format!("{}::interface::{}", rel_path, iface_name);
                        graph.add_edge(&cls_id, &iface_id, EdgeKind::Implements);
                    }
                }
            }

            scope_stack.push(ScopeContext {
                id: cls_id,
                name: cls_name.to_string(),
                entity_type: EntityType::Class,
                kind: ScopeKind::Class,
            });
            continue;
        }

        // 7. Interfaces
        if let Some(caps) = iface_re.captures(trimmed) {
            let iface_name = caps.get(1).unwrap().as_str();
            let iface_id = format!("{}::interface::{}", rel_path, iface_name);
            let mut iface_ent = CodeEntity::new(&iface_id, iface_name, EntityType::Interface, rel_path, lline.start_line, lline.end_line);
            iface_ent.signature = Some(trimmed.to_string());
            iface_ent.docstring = docstring_opt;
            graph.add_entity(iface_ent);

            let (parent_id, _, _) = get_current_container(&scope_stack);
            graph.add_edge(&parent_id, &iface_id, EdgeKind::Contains);

            scope_stack.push(ScopeContext {
                id: iface_id,
                name: iface_name.to_string(),
                entity_type: EntityType::Interface,
                kind: ScopeKind::Interface,
            });
            continue;
        }

        // 8. Structures (VB.NET) and Types (VB6/VBA UDT)
        if let Some(caps) = struct_re.captures(trimmed).or_else(|| type_udt_re.captures(trimmed)) {
            let struct_name = caps.get(1).unwrap().as_str();
            let struct_id = format!("{}::struct::{}", rel_path, struct_name);
            let mut struct_ent = CodeEntity::new(&struct_id, struct_name, EntityType::Struct, rel_path, lline.start_line, lline.end_line);
            struct_ent.signature = Some(trimmed.to_string());
            struct_ent.docstring = docstring_opt;
            graph.add_entity(struct_ent);

            let (parent_id, _, _) = get_current_container(&scope_stack);
            graph.add_edge(&parent_id, &struct_id, EdgeKind::Contains);

            scope_stack.push(ScopeContext {
                id: struct_id,
                name: struct_name.to_string(),
                entity_type: EntityType::Struct,
                kind: ScopeKind::Structure,
            });
            continue;
        }

        // 9. Enums
        if let Some(caps) = enum_re.captures(trimmed) {
            let enum_name = caps.get(1).unwrap().as_str();
            let enum_id = format!("{}::enum::{}", rel_path, enum_name);
            let mut enum_ent = CodeEntity::new(&enum_id, enum_name, EntityType::Struct, rel_path, lline.start_line, lline.end_line);
            enum_ent.signature = Some(trimmed.to_string());
            enum_ent.metadata.insert("kind".to_string(), "enum".to_string());
            enum_ent.docstring = docstring_opt;
            graph.add_entity(enum_ent);

            let (parent_id, _, _) = get_current_container(&scope_stack);
            graph.add_edge(&parent_id, &enum_id, EdgeKind::Contains);

            scope_stack.push(ScopeContext {
                id: enum_id,
                name: enum_name.to_string(),
                entity_type: EntityType::Struct,
                kind: ScopeKind::Enum,
            });
            continue;
        }

        // 10. Standalone Inherits statement inside class body
        if let Some(caps) = inherits_re.captures(trimmed) {
            let base_name = caps.get(1).unwrap().as_str();
            let (container_id, _, _) = get_current_container(&scope_stack);
            let base_id = format!("{}::class::{}", rel_path, base_name);
            graph.add_edge(&container_id, &base_id, EdgeKind::DependsOn);
            continue;
        }

        // 11. Standalone Implements statement inside class body
        if let Some(caps) = implements_re.captures(trimmed) {
            let ifaces_str = caps.get(1).unwrap().as_str();
            let (container_id, _, _) = get_current_container(&scope_stack);
            for part in ifaces_str.split(',') {
                let iface_name = part.trim();
                if !iface_name.is_empty() {
                    let iface_id = format!("{}::interface::{}", rel_path, iface_name);
                    graph.add_edge(&container_id, &iface_id, EdgeKind::Implements);
                }
            }
            continue;
        }

        // 12. Win32 DLL Declare (P/Invoke) in VB6/VBA/VB.NET
        if let Some(caps) = declare_re.captures(trimmed) {
            let func_name = caps.get(2).unwrap().as_str();
            let lib_name = caps.get(3).unwrap().as_str();
            let alias = caps.get(4).map(|m| m.as_str());
            let (container_id, container_name, _) = get_current_container(&scope_stack);

            let decl_id = format!("{}::{}::{}", rel_path, container_name, func_name);
            let mut decl_ent = CodeEntity::new(&decl_id, func_name, EntityType::Function, rel_path, lline.start_line, lline.end_line);
            decl_ent.signature = Some(trimmed.to_string());
            decl_ent.docstring = docstring_opt;
            decl_ent.metadata.insert("is_declare".to_string(), "true".to_string());
            decl_ent.metadata.insert("dll".to_string(), lib_name.to_string());
            if let Some(al) = alias {
                decl_ent.metadata.insert("alias".to_string(), al.to_string());
            }
            graph.add_entity(decl_ent);
            graph.add_edge(&container_id, &decl_id, EdgeKind::Contains);

            // Connect external DLL node
            let dll_node_id = format!("dll::{}", lib_name.to_ascii_lowercase());
            if graph.get_entity(&dll_node_id).is_none() {
                let mut dll_ent = CodeEntity::new(&dll_node_id, lib_name, EntityType::Module, "<external-dll>", lline.start_line, lline.end_line);
                dll_ent.signature = Some(format!("Declare Lib \"{}\"", lib_name));
                graph.add_entity(dll_ent);
            }
            graph.add_edge(&decl_id, &dll_node_id, EdgeKind::DependsOn);
            defined_routines.insert(func_name.to_string());
            continue;
        }

        // 13. Subroutines (Sub)
        if let Some(caps) = sub_re.captures(trimmed) {
            let sub_name = caps.get(1).unwrap().as_str();
            let params = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let implements_clause = caps.get(3).map(|m| m.as_str());
            let handles_clause = caps.get(4).map(|m| m.as_str());

            let (container_id, container_name, _) = get_current_container(&scope_stack);
            let sub_id = format!("{}::{}::{}", rel_path, container_name, sub_name);

            let mut sub_ent = CodeEntity::new(&sub_id, sub_name, EntityType::Method, rel_path, lline.start_line, lline.end_line);
            sub_ent.signature = Some(trimmed.to_string());
            sub_ent.docstring = docstring_opt;
            sub_ent.metadata.insert("parameters".to_string(), params.to_string());

            if let Some(imp) = implements_clause {
                sub_ent.metadata.insert("implements".to_string(), imp.to_string());
                let imp_target_id = format!("{}::{}", rel_path, imp);
                graph.add_edge(&sub_id, &imp_target_id, EdgeKind::Implements);
            }
            if let Some(hnd) = handles_clause {
                sub_ent.metadata.insert("handles".to_string(), hnd.to_string());
            }

            graph.add_entity(sub_ent);
            graph.add_edge(&container_id, &sub_id, EdgeKind::Contains);
            defined_routines.insert(sub_name.to_string());

            scope_stack.push(ScopeContext {
                id: sub_id,
                name: sub_name.to_string(),
                entity_type: EntityType::Method,
                kind: ScopeKind::Routine,
            });
            continue;
        }

        // 14. Functions (Function)
        if let Some(caps) = func_re.captures(trimmed) {
            let func_name = caps.get(1).unwrap().as_str();
            let params = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let ret_type = caps.get(3).map(|m| m.as_str()).unwrap_or("Variant");
            let implements_clause = caps.get(4).map(|m| m.as_str());

            let (container_id, container_name, _) = get_current_container(&scope_stack);
            let func_id = format!("{}::{}::{}", rel_path, container_name, func_name);

            let mut func_ent = CodeEntity::new(&func_id, func_name, EntityType::Function, rel_path, lline.start_line, lline.end_line);
            func_ent.signature = Some(trimmed.to_string());
            func_ent.docstring = docstring_opt;
            func_ent.metadata.insert("parameters".to_string(), params.to_string());
            func_ent.metadata.insert("return_type".to_string(), ret_type.to_string());

            if let Some(imp) = implements_clause {
                func_ent.metadata.insert("implements".to_string(), imp.to_string());
                let imp_target_id = format!("{}::{}", rel_path, imp);
                graph.add_edge(&func_id, &imp_target_id, EdgeKind::Implements);
            }

            graph.add_entity(func_ent);
            graph.add_edge(&container_id, &func_id, EdgeKind::Contains);
            defined_routines.insert(func_name.to_string());

            scope_stack.push(ScopeContext {
                id: func_id,
                name: func_name.to_string(),
                entity_type: EntityType::Function,
                kind: ScopeKind::Routine,
            });
            continue;
        }

        // 15. Properties (Property Get / Let / Set / Auto-Property)
        if let Some(caps) = prop_re.captures(trimmed) {
            let prop_kind = caps.get(1).map(|m| m.as_str()).unwrap_or("Standard");
            let prop_name = caps.get(2).unwrap().as_str();
            let params = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let prop_type = caps.get(4).map(|m| m.as_str()).unwrap_or("Variant");

            let (container_id, container_name, _) = get_current_container(&scope_stack);
            let prop_id = format!("{}::{}::{}::{}", rel_path, container_name, prop_kind, prop_name);

            let mut prop_ent = CodeEntity::new(&prop_id, prop_name, EntityType::Method, rel_path, lline.start_line, lline.end_line);
            prop_ent.signature = Some(trimmed.to_string());
            prop_ent.docstring = docstring_opt;
            prop_ent.metadata.insert("property_kind".to_string(), prop_kind.to_string());
            prop_ent.metadata.insert("parameters".to_string(), params.to_string());
            prop_ent.metadata.insert("property_type".to_string(), prop_type.to_string());

            graph.add_entity(prop_ent);
            graph.add_edge(&container_id, &prop_id, EdgeKind::Contains);
            defined_routines.insert(prop_name.to_string());

            scope_stack.push(ScopeContext {
                id: prop_id,
                name: prop_name.to_string(),
                entity_type: EntityType::Method,
                kind: ScopeKind::Routine,
            });
            continue;
        }

        // 16. Events
        if let Some(caps) = event_re.captures(trimmed) {
            let event_name = caps.get(1).unwrap().as_str();
            let (container_id, container_name, _) = get_current_container(&scope_stack);
            let event_id = format!("{}::{}::event::{}", rel_path, container_name, event_name);

            let mut event_ent = CodeEntity::new(&event_id, event_name, EntityType::Variable, rel_path, lline.start_line, lline.end_line);
            event_ent.signature = Some(trimmed.to_string());
            event_ent.docstring = docstring_opt;
            event_ent.metadata.insert("is_event".to_string(), "true".to_string());

            graph.add_entity(event_ent);
            graph.add_edge(&container_id, &event_id, EdgeKind::Contains);
            continue;
        }

        // 17. Fields & Variables (Module/Class Level)
        let is_in_routine = scope_stack.iter().any(|s| s.kind == ScopeKind::Routine);
        if !is_in_routine {
            if let Some(caps) = field_re.captures(trimmed) {
                let var_name = caps.get(3).unwrap().as_str();
                let type_name = caps.get(4).unwrap().as_str();
                let (container_id, container_name, _) = get_current_container(&scope_stack);
                let field_id = format!("{}::{}::field::{}", rel_path, container_name, var_name);

                let mut field_ent = CodeEntity::new(&field_id, var_name, EntityType::Variable, rel_path, lline.start_line, lline.end_line);
                field_ent.signature = Some(trimmed.to_string());
                field_ent.docstring = docstring_opt;
                field_ent.metadata.insert("type".to_string(), type_name.to_string());

                graph.add_entity(field_ent);
                graph.add_edge(&container_id, &field_id, EdgeKind::Contains);

                // If early-bound COM or custom class (e.g. ADODB.Connection)
                if type_name.contains('.') {
                    let com_node_id = format!("com::{}", type_name);
                    if graph.get_entity(&com_node_id).is_none() {
                        let mut com_ent = CodeEntity::new(&com_node_id, type_name, EntityType::Module, "<com-activex>", lline.start_line, lline.end_line);
                        com_ent.signature = Some(format!("COM Type {}", type_name));
                        graph.add_entity(com_ent);
                    }
                    graph.add_edge(&field_id, &com_node_id, EdgeKind::References);
                }
                continue;
            }
        }

        // 18. COM / ActiveX Instantiations (CreateObject("ADODB.Connection"), GetObject, WScript.CreateObject)
        if let Some(caps) = com_create_re.captures(trimmed).or_else(|| new_com_re.captures(trimmed)) {
            let prog_id = caps.get(1).unwrap().as_str();
            let com_node_id = format!("com::{}", prog_id);
            if graph.get_entity(&com_node_id).is_none() {
                let mut com_ent = CodeEntity::new(&com_node_id, prog_id, EntityType::Module, "<com-activex>", lline.start_line, lline.end_line);
                com_ent.signature = Some(format!("COM Component {}", prog_id));
                com_ent.metadata.insert("prog_id".to_string(), prog_id.to_string());
                graph.add_entity(com_ent);
            }
            let caller_id = get_current_routine_or_container(&scope_stack);
            graph.add_edge(&caller_id, &com_node_id, EdgeKind::References);
        }

        // 19. Explicit Call Statements: Call HelperSub(...)
        if let Some(caps) = call_stmt_re.captures(trimmed) {
            let target_sub = caps.get(1).unwrap().as_str();
            let caller_id = get_current_routine_or_container(&scope_stack);
            calls_to_resolve.push((caller_id, target_sub.to_string()));
        } else if let Some(caps) = method_call_re.captures(trimmed) {
            let target_method = caps.get(1).unwrap().as_str();
            let caller_id = get_current_routine_or_container(&scope_stack);
            calls_to_resolve.push((caller_id, target_method.to_string()));
        }
    }

    // Resolve inter-procedural calls within the module or across project
    for (caller_id, target_name) in calls_to_resolve {
        if defined_routines.iter().any(|r| r.eq_ignore_ascii_case(&target_name)) {
            let mut target_id = None;
            for idx in graph.graph.node_indices() {
                let ent = &graph.graph[idx];
                if ent.file_path == rel_path && ent.name.eq_ignore_ascii_case(&target_name) && ent.id != caller_id {
                    target_id = Some(ent.id.clone());
                    break;
                }
            }
            if let Some(tid) = target_id {
                graph.add_edge(&caller_id, &tid, EdgeKind::Calls);
            }
        }
    }
}
