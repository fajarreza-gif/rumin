pub mod systems;
pub mod scripting;
pub mod jvm_xml;
pub mod salesforce;
pub mod telecom;
pub mod vb;
pub mod json_manifests;
pub mod yaml_manifests;
pub mod omnistudio_json;

pub use systems::*;
pub use scripting::*;
pub use jvm_xml::*;
pub use salesforce::*;
pub use telecom::*;
pub use vb::*;
pub use json_manifests::*;
pub use yaml_manifests::*;
pub use omnistudio_json::*;

use std::fs::File;
use std::io::Read;
use std::path::Path;
use memchr::memchr;
use crate::graph::{CodeGraph, CodeEntity, EntityType};

/// Check if a file is binary by inspecting the first 512 bytes for a NULL byte.
pub fn is_binary_file(path: &Path) -> bool {
    let mut buffer = [0u8; 512];
    if let Ok(mut file) = File::open(path) {
        if let Ok(bytes_read) = file.read(&mut buffer) {
            return memchr(0, &buffer[..bytes_read]).is_some();
        }
    }
    false
}

/// Universal parser router for supported source code languages and markup formats.
pub fn parse_generic_source(rel_path: &str, content: &str, lang: &str, graph: &mut CodeGraph) {
    match lang {
        "rust" | "rs" => {
            parse_rust(rel_path, content, graph);
            return;
        }
        "python" | "py" => {
            parse_python(rel_path, content, graph);
            return;
        }
        "typescript" | "javascript" | "ts" | "js" => {
            parse_typescript(rel_path, content, graph);
            return;
        }
        "go" => {
            parse_go(rel_path, content, graph);
            return;
        }
        "c" => {
            parse_c(rel_path, content, graph);
            return;
        }
        "cpp" | "c++" => {
            parse_cpp(rel_path, content, graph);
            return;
        }
        "java" => {
            parse_java(rel_path, content, graph);
            return;
        }
        "xml" => {
            parse_xml(rel_path, content, graph);
            return;
        }
        "html" => {
            parse_html(rel_path, content, graph);
            return;
        }
        "sql" | "plsql" | "oracle" => {
            parse_plsql(rel_path, content, graph);
            return;
        }
        "apex" => {
            parse_apex_class(rel_path, content, graph);
            return;
        }
        "cls" => {
            if is_vb_cls(content) {
                parse_vb(rel_path, content, graph);
            } else {
                parse_apex_class(rel_path, content, graph);
            }
            return;
        }
        "vb" | "vba" | "bas" | "frm" | "ctl" | "vbs" | "visualbasic" => {
            parse_vb(rel_path, content, graph);
            return;
        }
        "trigger" => {
            parse_apex_trigger(rel_path, content, graph);
            return;
        }
        "soql" => {
            parse_soql(rel_path, content, graph);
            return;
        }
        "aura" => {
            parse_aura(rel_path, content, graph);
            return;
        }
        "erlang" | "erl" | "hrl" => {
            parse_erlang_module(rel_path, content, graph);
            return;
        }
        "apigee" => {
            parse_apigee(rel_path, content, graph);
            return;
        }
        "json" | "jsonc" => {
            let path = Path::new(rel_path);
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_omnistudio_json_path(rel_path, file_name) || is_omnistudio_json_content(content) {
                parse_omnistudio_json(rel_path, content, graph);
            } else {
                parse_json_manifest(rel_path, content, graph);
            }
            return;
        }
        "yaml" | "yml" => {
            parse_yaml_manifest(rel_path, content, graph);
            return;
        }
        _ => {}
    }

    // Generic fallback heuristics
    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        if lang == "python" {
            if let Some(rest) = trimmed.strip_prefix("def ") {
                if let Some(name) = rest.split('(').next() {
                    let name = name.trim();
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            } else if let Some(rest) = trimmed.strip_prefix("class ") {
                if let Some(name) = rest.split(&['(', ':'][..]).next() {
                    let name = name.trim();
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            }
        } else if lang == "rust" {
            if let Some(rest) = trimmed.strip_prefix("pub fn ") {
                if let Some(name) = rest.split('(').next() {
                    let name = name.trim();
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            } else if let Some(rest) = trimmed.strip_prefix("fn ") {
                if let Some(name) = rest.split('(').next() {
                    let name = name.trim();
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            } else if let Some(rest) = trimmed.strip_prefix("pub struct ") {
                if let Some(name) = rest.split_whitespace().next() {
                    let name = name.trim();
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            } else if let Some(rest) = trimmed.strip_prefix("struct ") {
                if let Some(name) = rest.split_whitespace().next() {
                    let name = name.trim();
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            }
        }
    }
}
