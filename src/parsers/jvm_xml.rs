use regex::Regex;
use crate::graph::{CodeGraph, CodeEntity, EntityType};

/// Parse Java source files into classes, interfaces, enums, records, packages, and methods.
pub fn parse_java(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let pkg_re = Regex::new(r"^\s*package\s+([a-zA-Z0-9_.]+);").unwrap();
    let class_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+|abstract\s+|static\s+|final\s+)*class\s+([a-zA-Z0-9_]+)").unwrap();
    let iface_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+)*interface\s+([a-zA-Z0-9_]+)").unwrap();
    let enum_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+)*enum\s+([a-zA-Z0-9_]+)").unwrap();
    let record_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+)*record\s+([a-zA-Z0-9_]+)").unwrap();
    let method_re = Regex::new(r"^\s*(?:public|protected|private|static|final|synchronized|abstract|default|native)\s+(?:[\w<>\[\],\s?]+)\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*(?:throws\s+[\w,\s]+)?\s*\{").unwrap();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = pkg_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::pkg::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Module, rel_path, line_num, line_num);
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
        } else if let Some(caps) = enum_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = record_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = method_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            if !matches!(name, "if" | "while" | "for" | "switch" | "catch" | "return") {
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            }
        }
    }
}

/// Parse XML documents into structured elements and tags with IDs or names.
pub fn parse_xml(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let tag_re = Regex::new(r"<([a-zA-Z0-9_:-]+)(\s+[^>]*)?>").unwrap();
    let id_re = Regex::new(r#"(?:id|name)=["']([^"']+)["']"#).unwrap();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("<!--") || trimmed.starts_with("<?") || trimmed.is_empty() {
            continue;
        }

        for caps in tag_re.captures_iter(line) {
            let raw_tag = caps.get(1).unwrap().as_str();
            if raw_tag.starts_with('/') || raw_tag.starts_with('!') || raw_tag.starts_with('?') {
                continue;
            }
            let attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let name = if let Some(id_caps) = id_re.captures(attrs) {
                format!("{}#{}", raw_tag, id_caps.get(1).unwrap().as_str())
            } else {
                raw_tag.to_string()
            };

            let id = format!("{}::{}::{}", rel_path, name, line_num);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Tag, rel_path, line_num, line_num);
            entity.signature = Some(caps.get(0).unwrap().as_str().to_string());
            graph.add_entity(entity);
        }
    }
}

/// Parse Oracle PL/SQL files into packages, procedures, functions, triggers, types, sequences, and tables.
pub fn parse_plsql(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let pkg_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?PACKAGE\s+(?:BODY\s+)?([`\x22']?(\w+)[`\x22']?)").unwrap();
    let proc_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?PROCEDURE\s+([`\x22']?(\w+)[`\x22']?)").unwrap();
    let func_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([`\x22']?(\w+)[`\x22']?)").unwrap();
    let trig_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?TRIGGER\s+([`\x22']?(\w+)[`\x22']?)").unwrap();
    let type_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?TYPE\s+(?:BODY\s+)?([`\x22']?(\w+)[`\x22']?)").unwrap();
    let seq_re = Regex::new(r"(?i)CREATE\s+SEQUENCE\s+([`\x22']?(\w+)[`\x22']?)").unwrap();
    let table_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?(?:FORCE\s+)?(TABLE|VIEW)\s+(?:IF\s+NOT\s+EXISTS\s+)?([`\x22']?(\w+)[`\x22']?)").unwrap();
    let member_proc_re = Regex::new(r"(?i)^\s*PROCEDURE\s+([a-zA-Z0-9_#$]+)\s*(?:\(|$)").unwrap();
    let member_func_re = Regex::new(r"(?i)^\s*FUNCTION\s+([a-zA-Z0-9_#$]+)\s*(?:\(|$)").unwrap();

    let mut current_package: Option<String> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("--") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = pkg_re.captures(line) {
            let name = caps.get(1).unwrap().as_str();
            current_package = Some(name.to_string());
            let id = format!("{}::pkg::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Module, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = proc_re.captures(line) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = func_re.captures(line) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = trig_re.captures(line) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Rule, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = type_re.captures(line) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = seq_re.captures(line) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Variable, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = table_re.captures(line) {
            let kind_str = caps.get(1).unwrap().as_str().to_ascii_lowercase();
            let name = caps.get(2).unwrap().as_str();
            let ent_type = if kind_str.contains("view") { EntityType::View } else { EntityType::Table };
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, ent_type, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = member_proc_re.captures(line) {
            if let Some(pkg) = &current_package {
                let m_name = caps.get(1).unwrap().as_str();
                let full_name = format!("{}.{}", pkg, m_name);
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, &full_name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            }
        } else if let Some(caps) = member_func_re.captures(line) {
            if let Some(pkg) = &current_package {
                let m_name = caps.get(1).unwrap().as_str();
                let full_name = format!("{}.{}", pkg, m_name);
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, &full_name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            }
        }
    }
}
