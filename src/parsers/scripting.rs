use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};
use regex::Regex;

/// Detect scripting language from shebang line.
pub fn detect_shebang(header: &[u8]) -> Option<&'static str> {
    let first_line = header.split(|&b| b == b'\n').next().unwrap_or(header);
    if first_line.starts_with(b"#!") {
        let line_str = String::from_utf8_lossy(first_line);
        let lower = line_str.to_ascii_lowercase();
        if lower.contains("node") || lower.contains("bun") || lower.contains("deno") || lower.contains("ts-node") {
            return Some("typescript");
        }
        if lower.contains("bash") || lower.contains("/sh") || lower.ends_with(" sh") {
            return Some("bash");
        }
        if lower.contains("python") {
            return Some("python");
        }
        if lower.contains("escript") {
            return Some("erlang");
        }
    }
    None
}

/// Parse a Makefile into Targets and DependsOn edges natively in Rust.
pub fn parse_makefile(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let lines: Vec<&str> = content.lines().collect();
    let mut targets: Vec<(String, Vec<String>, usize)> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        if line.starts_with('#') || line.starts_with('\t') || line.trim().is_empty() {
            continue;
        }

        if let Some(colon_pos) = line.find(':') {
            if line[colon_pos..].starts_with(":=") {
                continue;
            }

            let target_part = line[..colon_pos].trim();
            let prereq_part = line[colon_pos + 1..].trim();

            let prereqs: Vec<String> = prereq_part
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();

            for target_name in target_part.split_whitespace() {
                targets.push((target_name.to_string(), prereqs.clone(), line_num));
            }
        }
    }

    for (target_name, prereqs, line_num) in targets {
        let target_id = format!("{}::target::{}", rel_path, target_name);
        let mut entity = CodeEntity::new(
            &target_id,
            &target_name,
            EntityType::Target,
            rel_path,
            line_num,
            line_num,
        );
        entity.signature = Some(format!("{}: {}", target_name, prereqs.join(" ")));
        graph.add_entity(entity);

        for prereq in prereqs {
            let prereq_id = format!("{}::target::{}", rel_path, prereq);
            graph.add_edge(&target_id, &prereq_id, EdgeKind::DependsOn);
        }
    }
}

/// Parse TypeScript and JavaScript files into functions, classes, interfaces, types, and enums.
pub fn parse_typescript(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let fn_decl_re = Regex::new(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(").unwrap();
    let fn_expr_re = Regex::new(r"^\s*(?:export\s+)?(?:const|let|var)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[a-zA-Z_$][a-zA-Z0-9_$]*)(?:\s*:\s*[^=>]+)?\s*=>").unwrap();
    let fn_assign_re = Regex::new(r"^\s*(?:export\s+)?(?:const|let|var)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=\s*(?:async\s+)?function\b").unwrap();
    let class_re = Regex::new(r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();
    let iface_re = Regex::new(r"^\s*(?:export\s+)?interface\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();
    let type_re = Regex::new(r"^\s*(?:export\s+)?type\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=").unwrap();
    let enum_re = Regex::new(r"^\s*(?:export\s+)?(?:const\s+)?enum\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = fn_decl_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = fn_expr_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = fn_assign_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = class_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = iface_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Interface, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = type_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = enum_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        }
    }
}

/// Parse HTML files into semantic tags and custom component nodes.
pub fn parse_html(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let tag_re = Regex::new(r"<([a-zA-Z0-9_:-]+)(\s+[^>]*)?>").unwrap();
    let id_re = Regex::new(r#"id=["']([^"']+)["']"#).unwrap();
    let class_re = Regex::new(r#"class=["']([^"']+)["']"#).unwrap();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("<!--") || trimmed.starts_with("<!DOCTYPE") || trimmed.is_empty() {
            continue;
        }

        for caps in tag_re.captures_iter(line) {
            let raw_tag = caps.get(1).unwrap().as_str();
            if raw_tag.starts_with('/') || raw_tag.starts_with('!') || raw_tag.starts_with('?') {
                continue;
            }
            let tag = raw_tag.to_ascii_lowercase();
            let attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let id_match = id_re.captures(attrs);
            let class_match = class_re.captures(attrs);

            let is_semantic = matches!(
                tag.as_str(),
                "title"
                    | "template"
                    | "form"
                    | "main"
                    | "header"
                    | "nav"
                    | "section"
                    | "article"
                    | "footer"
                    | "dialog"
                    | "canvas"
                    | "svg"
                    | "script"
                    | "style"
            );
            let is_custom_component = tag.contains('-');

            let name = if let Some(id_c) = id_match {
                Some(format!("{}#{}", tag, id_c.get(1).unwrap().as_str()))
            } else if is_custom_component || is_semantic {
                Some(tag.clone())
            } else if let Some(class_c) = class_match {
                let first_class = class_c.get(1).unwrap().as_str().split_whitespace().next().unwrap_or("");
                Some(format!("{}.{}", tag, first_class))
            } else {
                None
            };

            if let Some(entity_name) = name {
                let id = format!("{}::{}::{}", rel_path, entity_name, line_num);
                let mut entity = CodeEntity::new(&id, entity_name, EntityType::Tag, rel_path, line_num, line_num);
                entity.signature = Some(caps.get(0).unwrap().as_str().to_string());
                graph.add_entity(entity);
            }
        }
    }
}

/// Parse Python source files into classes, functions, methods, and imports.
pub fn parse_python(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let class_re = Regex::new(r"^\s*class\s+([a-zA-Z0-9_]+)(?:\(([^)]+)\))?\s*:").unwrap();
    let func_re = Regex::new(r"^(\s*)(?:async\s+)?def\s+([a-zA-Z0-9_]+)\s*\(").unwrap();
    let import_re = Regex::new(r"^\s*(?:from\s+([a-zA-Z0-9_\.]+)\s+import|import\s+([a-zA-Z0-9_\.]+))").unwrap();

    let mut current_class: Option<(String, usize)> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        let indent = line.len() - line.trim_start().len();
        if let Some((_, class_indent)) = current_class {
            if indent <= class_indent && !trimmed.starts_with('@') && !trimmed.is_empty() {
                current_class = None;
            }
        }

        if let Some(caps) = class_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            current_class = Some((name.to_string(), indent));

            if let Some(bases) = caps.get(2) {
                for base in bases.as_str().split(',') {
                    let base = base.trim();
                    if !base.is_empty() {
                        let base_id = format!("{}::{}", rel_path, base);
                        graph.add_edge(&id, &base_id, EdgeKind::DependsOn);
                    }
                }
            }
        } else if let Some(caps) = func_re.captures(line) {
            let name = caps.get(2).unwrap().as_str();
            let (full_name, id, ent_type) = if let Some((ref cls, _)) = current_class {
                let full = format!("{}.{}", cls, name);
                let id = format!("{}::{}", rel_path, full);
                (full, id, EntityType::Method)
            } else {
                let id = format!("{}::{}", rel_path, name);
                (name.to_string(), id, EntityType::Function)
            };

            let mut entity = CodeEntity::new(&id, &full_name, ent_type, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);

            if let Some((ref cls, _)) = current_class {
                let class_id = format!("{}::{}", rel_path, cls);
                graph.add_edge(&class_id, &id, EdgeKind::Contains);
            }
        } else if let Some(caps) = import_re.captures(trimmed) {
            let mod_name = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
            let src_id = format!("{}::file", rel_path);
            graph.add_edge(&src_id, mod_name, EdgeKind::Imports);
        }
    }
}
