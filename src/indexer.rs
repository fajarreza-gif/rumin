use crate::graph::CodeGraph;
use crate::sidecar::SidecarClient;
use crate::types::{CodeEntity, EdgeKind, EntityType};
use anyhow::Result;
use memchr::memchr;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, info};
use walkdir::WalkDir;

pub struct Indexer {
    repo_root: PathBuf,
}

impl Indexer {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

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

    /// Sniff the shebang or first line to detect scripts without file extensions.
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
            // Ignore comments and recipe lines (starting with tab)
            if line.starts_with('#') || line.starts_with('\t') || line.trim().is_empty() {
                continue;
            }

            if let Some(colon_pos) = line.find(':') {
                // Ensure it's not a variable assignment like ':= '
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

    /// Parse Go source files into packages, standalone functions, receiver methods, structs, and interfaces.
    pub fn parse_go(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let pkg_re = Regex::new(r"^\s*package\s+([a-zA-Z0-9_]+)").unwrap();
        let method_re = Regex::new(r"^\s*func\s*\(\s*(?:[a-zA-Z0-9_]+\s+)?\*?([a-zA-Z0-9_]+)\s*\)\s*([a-zA-Z0-9_]+)\s*\(").unwrap();
        let func_re = Regex::new(r"^\s*func\s+([a-zA-Z0-9_]+)\s*\(").unwrap();
        let struct_re = Regex::new(r"^\s*type\s+([a-zA-Z0-9_]+)\s+struct\b").unwrap();
        let iface_re = Regex::new(r"^\s*type\s+([a-zA-Z0-9_]+)\s+interface\b").unwrap();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = pkg_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::pkg::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Module, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = method_re.captures(trimmed) {
                let recv = caps.get(1).unwrap().as_str();
                let method = caps.get(2).unwrap().as_str();
                let full_name = format!("{}.{}", recv, method);
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, full_name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = func_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = struct_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = iface_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Interface, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            }
        }
    }

    /// Parse C source and header files into structs, unions, enums, macros, and functions.
    pub fn parse_c(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let struct_re = Regex::new(r"^\s*(?:typedef\s+)?struct\s+([a-zA-Z0-9_]+)\s*\{").unwrap();
        let typedef_end_re = Regex::new(r"^\s*\}\s*([a-zA-Z0-9_]+)\s*;").unwrap();
        let union_re = Regex::new(r"^\s*(?:typedef\s+)?union\s+([a-zA-Z0-9_]+)\s*\{").unwrap();
        let enum_re = Regex::new(r"^\s*(?:typedef\s+)?enum\s+([a-zA-Z0-9_]+)\s*\{").unwrap();
        let macro_re = Regex::new(r"^\s*#define\s+([a-zA-Z0-9_]+)\b").unwrap();
        let func_re = Regex::new(r"^\s*(?:static\s+|inline\s+|extern\s+)*[a-zA-Z0-9_]+(?:\s*\*+)?\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*\{").unwrap();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = struct_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::struct::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = typedef_end_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::type::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = union_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::union::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = enum_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::enum::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = macro_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::macro::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Variable, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = func_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                if !matches!(name, "if" | "while" | "for" | "switch" | "return" | "sizeof") {
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            }
        }
    }

    /// Parse C++ files into namespaces, classes, structs, methods, and functions.
    pub fn parse_cpp(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let ns_re = Regex::new(r"^\s*namespace\s+([a-zA-Z0-9_:]+)\s*\{").unwrap();
        let class_re = Regex::new(r"^\s*(?:template\s*<[^>]*>\s*)?class\s+([a-zA-Z0-9_]+)(?:\s+final)?(?:\s*:\s*[^\{]+)?\s*\{").unwrap();
        let struct_re = Regex::new(r"^\s*(?:template\s*<[^>]*>\s*)?struct\s+([a-zA-Z0-9_]+)(?:\s+final)?(?:\s*:\s*[^\{]+)?\s*\{").unwrap();
        let method_re = Regex::new(r"^\s*(?:[a-zA-Z0-9_<>:*&]+\s+)?([a-zA-Z0-9_]+)::([a-zA-Z0-9_~]+)\s*\([^;)]*\)\s*(?:const)?\s*(?:noexcept)?\s*\{").unwrap();
        let func_re = Regex::new(r"^\s*(?:template\s*<[^>]*>\s*)?(?:inline\s+|static\s+|constexpr\s+|virtual\s+)*[a-zA-Z0-9_<>:*&]+\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*\{").unwrap();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = ns_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::ns::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Module, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = class_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = struct_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = method_re.captures(trimmed) {
                let class_name = caps.get(1).unwrap().as_str();
                let method_name = caps.get(2).unwrap().as_str();
                let full_name = format!("{}::{}", class_name, method_name);
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, full_name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            } else if let Some(caps) = func_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                if !matches!(name, "if" | "while" | "for" | "switch" | "catch" | "return") {
                    let id = format!("{}::{}", rel_path, name);
                    let mut entity = CodeEntity::new(&id, name, EntityType::Function, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            }
        }
    }

    /// Parse Java files into packages, classes, interfaces, records, enums, and methods.
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
                let mut entity = CodeEntity::new(&id, name, EntityType::Tag, rel_path, line_num, line_num);
                entity.signature = Some(caps.get(0).unwrap().as_str().to_string());
                graph.add_entity(entity);
            }
        }
    }

    /// Parse HTML documents into semantic tags, elements with IDs, classes, and web components.
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
                    let mut entity = CodeEntity::new(&id, full_name, EntityType::Method, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            } else if let Some(caps) = member_func_re.captures(line) {
                if let Some(pkg) = &current_package {
                    let m_name = caps.get(1).unwrap().as_str();
                    let full_name = format!("{}.{}", pkg, m_name);
                    let id = format!("{}::{}", rel_path, full_name);
                    let mut entity = CodeEntity::new(&id, full_name, EntityType::Method, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            }
        }
    }

    /// Parse Salesforce Apex classes into classes, interfaces, enums, annotated methods, and embedded SOQL references.
    pub fn parse_apex_class(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let class_re = Regex::new(r"(?i)^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:public|global|private)\s+(?:(?:with|without|inherited)\s+sharing\s+)?(?:virtual|abstract)?\s*class\s+([a-zA-Z0-9_]+)").unwrap();
        let iface_re = Regex::new(r"(?i)^\s*(?:public|global)\s+interface\s+([a-zA-Z0-9_]+)").unwrap();
        let enum_re = Regex::new(r"(?i)^\s*(?:public|global|private)\s+enum\s+([a-zA-Z0-9_]+)").unwrap();
        let method_re = Regex::new(r"(?i)^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:public|global|private|protected)\s+(?:static\s+)?[a-zA-Z0-9_<>\[\],\s]+\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*\{").unwrap();
        let soql_re = Regex::new(r"(?i)\[\s*SELECT\s+.*?\s+FROM\s+([a-zA-Z0-9_]+)").unwrap();

        let mut current_class: Option<String> = None;

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = class_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                current_class = Some(name.to_string());
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);

                let apex_id = format!("apex::{}", name);
                if graph.get_entity(&apex_id).is_none() {
                    let mut alias_entity = CodeEntity::new(&apex_id, name, EntityType::Class, rel_path, line_num, line_num);
                    alias_entity.signature = Some(trimmed.to_string());
                    graph.add_entity(alias_entity);
                }
                graph.add_edge(&apex_id, &id, EdgeKind::References);
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
            } else if let Some(caps) = method_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                if !matches!(name, "if" | "while" | "for" | "switch" | "catch" | "return") {
                    let full_name = if let Some(ref cls) = current_class {
                        format!("{}.{}", cls, name)
                    } else {
                        name.to_string()
                    };
                    let id = format!("{}::{}", rel_path, full_name);
                    let mut entity = CodeEntity::new(&id, &full_name, EntityType::Method, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
            }

            // Embedded SOQL query detection linking to SObject
            if let Some(caps) = soql_re.captures(trimmed) {
                let sobject = caps.get(1).unwrap().as_str();
                let sobject_id = format!("sobject::{}", sobject);
                if graph.get_entity(&sobject_id).is_none() {
                    let sobj_entity = CodeEntity::new(&sobject_id, sobject, EntityType::Table, rel_path, line_num, line_num);
                    graph.add_entity(sobj_entity);
                }
                if let Some(ref cls) = current_class {
                    let class_id = format!("{}::{}", rel_path, cls);
                    graph.add_edge(&class_id, &sobject_id, EdgeKind::References);
                }
            }
        }
    }

    /// Parse Salesforce Apex triggers into trigger rules and dependency edges to target SObjects.
    pub fn parse_apex_trigger(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let trigger_re = Regex::new(r"(?i)^\s*trigger\s+([a-zA-Z0-9_]+)\s+on\s+([a-zA-Z0-9_]+)\s*\(([^)]+)\)").unwrap();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = trigger_re.captures(trimmed) {
                let trigger_name = caps.get(1).unwrap().as_str();
                let sobject_name = caps.get(2).unwrap().as_str();
                let events = caps.get(3).unwrap().as_str();

                let trigger_id = format!("{}::trigger::{}", rel_path, trigger_name);
                let mut entity = CodeEntity::new(&trigger_id, trigger_name, EntityType::Rule, rel_path, line_num, line_num);
                entity.signature = Some(format!("trigger {} on {} ({})", trigger_name, sobject_name, events.trim()));
                graph.add_entity(entity);

                // Target SObject node & causal dependency edge
                let sobject_id = format!("sobject::{}", sobject_name);
                if graph.get_entity(&sobject_id).is_none() {
                    let sobj_entity = CodeEntity::new(&sobject_id, sobject_name, EntityType::Table, rel_path, line_num, line_num);
                    graph.add_entity(sobj_entity);
                }
                graph.add_edge(&trigger_id, &sobject_id, EdgeKind::DependsOn);
            }
        }
    }

    /// Parse Salesforce SObject and Custom Field metadata XML files.
    pub fn parse_salesforce_metadata(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let path = Path::new(rel_path);
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Check if Custom Object definition
        if file_name.ends_with(".object-meta.xml") || file_name.ends_with(".object") {
            let sobject_name = if let Some(stripped) = file_name.strip_suffix(".object-meta.xml") {
                stripped
            } else if let Some(stripped) = file_name.strip_suffix(".object") {
                stripped
            } else {
                file_name
            };

            let sobject_id = format!("sobject::{}", sobject_name);
            let mut entity = CodeEntity::new(&sobject_id, sobject_name, EntityType::Table, rel_path, 1, 1);
            entity.signature = Some(format!("CustomObject {}", sobject_name));
            graph.add_entity(entity);
        } else if file_name.ends_with(".field-meta.xml") || file_name.ends_with(".field") {
            // Custom Field definition
            let field_name_re = Regex::new(r"<fullName>([^<]+)</fullName>").unwrap();
            let type_re = Regex::new(r"<type>([^<]+)</type>").unwrap();

            let field_name = if let Some(caps) = field_name_re.captures(content) {
                caps.get(1).unwrap().as_str().to_string()
            } else if let Some(stripped) = file_name.strip_suffix(".field-meta.xml") {
                stripped.to_string()
            } else {
                file_name.to_string()
            };

            let field_type = type_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("Text");
            let field_id = format!("{}::field::{}", rel_path, field_name);
            let mut entity = CodeEntity::new(&field_id, &field_name, EntityType::Column, rel_path, 1, 1);
            entity.signature = Some(format!("CustomField {} ({})", field_name, field_type));
            graph.add_entity(entity);

            // Connect field to parent SObject if detectable in path
            let components: Vec<_> = path.iter().map(|c| c.to_string_lossy().to_string()).collect();
            if let Some(pos) = components.iter().position(|c| c == "objects") {
                if pos + 1 < components.len() {
                    let parent_sobject = &components[pos + 1];
                    let sobject_id = format!("sobject::{}", parent_sobject);
                    if graph.get_entity(&sobject_id).is_none() {
                        let sobj_entity = CodeEntity::new(&sobject_id, parent_sobject.as_str(), EntityType::Table, rel_path, 1, 1);
                        graph.add_entity(sobj_entity);
                    }
                    graph.add_edge(&sobject_id, &field_id, EdgeKind::Contains);
                }
            }
        }
    }

    /// Parse standalone SOQL query files into query views and references to queried SObjects.
    pub fn parse_soql(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let from_re = Regex::new(r"(?i)\bFROM\s+([a-zA-Z0-9_]+)").unwrap();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("--") || trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = from_re.captures(line) {
                let sobject = caps.get(1).unwrap().as_str();
                let query_id = format!("{}::query::{}", rel_path, line_num);
                let mut entity = CodeEntity::new(&query_id, format!("SOQL::{}", sobject), EntityType::View, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);

                let sobject_id = format!("sobject::{}", sobject);
                if graph.get_entity(&sobject_id).is_none() {
                    let sobj_entity = CodeEntity::new(&sobject_id, sobject, EntityType::Table, rel_path, line_num, line_num);
                    graph.add_entity(sobj_entity);
                }
                graph.add_edge(&query_id, &sobject_id, EdgeKind::References);
            }
        }
    }

    /// Parse Aura component markup into component tags and references to controller Apex classes.
    pub fn parse_aura(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let component_re = Regex::new(r#"(?i)<aura:component(?:\s+[^>]*?)?\s+controller=["']([^"']+)["']"#).unwrap();
        let path = Path::new(rel_path);
        let comp_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("auraComponent");

        let comp_id = format!("{}::component::{}", rel_path, comp_name);
        let mut entity = CodeEntity::new(&comp_id, comp_name, EntityType::Tag, rel_path, 1, 1);
        entity.signature = Some(format!("<aura:component name=\"{}\">", comp_name));
        graph.add_entity(entity);

        if let Some(caps) = component_re.captures(content) {
            let controller = caps.get(1).unwrap().as_str();
            let controller_id = format!("apex::{}", controller);
            if graph.get_entity(&controller_id).is_none() {
                let ctrl_entity = CodeEntity::new(&controller_id, controller, EntityType::Class, rel_path, 1, 1);
                graph.add_entity(ctrl_entity);
            }
            graph.add_edge(&comp_id, &controller_id, EdgeKind::References);
        }
    }

    /// Count comma-separated arguments in an Erlang function head taking nested tuples/lists into account.
    fn count_erlang_args(args_str: &str) -> usize {
        let trimmed = args_str.trim();
        if trimmed.is_empty() {
            return 0;
        }
        let mut depth = 0;
        let mut count = 1;
        for ch in trimmed.chars() {
            match ch {
                '(' | '{' | '[' => depth += 1,
                ')' | '}' | ']' => {
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                ',' if depth == 0 => count += 1,
                _ => {}
            }
        }
        count
    }

    /// Parse Erlang module, header, or escript files into modules, behaviors, records, macros, types, functions, and cross-calls.
    pub fn parse_erlang_module(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let mod_re = Regex::new(r#"^\s*-\s*module\s*\(\s*([a-zA-Z0-9_]+)\s*\)\s*\."#).unwrap();
        let beh_re = Regex::new(r#"^\s*-\s*behaviou?r\s*\(\s*([a-zA-Z0-9_]+)\s*\)\s*\."#).unwrap();
        let rec_re = Regex::new(r#"^\s*-\s*record\s*\(\s*([a-zA-Z0-9_]+)\s*,"#).unwrap();
        let def_re = Regex::new(r#"^\s*-\s*define\s*\(\s*([a-zA-Z0-9_]+)"#).unwrap();
        let type_re = Regex::new(r#"^\s*-\s*(?:type|opaque)\s+([a-zA-Z0-9_]+)\s*\("#).unwrap();
        let inc_re = Regex::new(r#"^\s*-\s*include(?:_lib)?\s*\(\s*"([^"]+)"\s*\)\s*\."#).unwrap();
        let func_re = Regex::new(r#"^([a-z][a-zA-Z0-9_]*)\s*\(([^)]*)\)\s*(?:when\s+[^->]+)?\s*->"#).unwrap();
        let call_re = Regex::new(r#"\b([a-z][a-zA-Z0-9_]*):([a-z][a-zA-Z0-9_]*)\s*\("#).unwrap();

        let path = Path::new(rel_path);
        let default_mod = path.file_stem().and_then(|s| s.to_str()).unwrap_or("erlang_mod");
        let mut current_module = default_mod.to_string();
        let mut module_node_id = format!("{}::{}", rel_path, current_module);
        let mut module_registered = false;

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with('%') || trimmed.is_empty() {
                continue;
            }

            // 1. Module declaration
            if let Some(caps) = mod_re.captures(trimmed) {
                let m_name = caps.get(1).unwrap().as_str();
                current_module = m_name.to_string();
                module_node_id = format!("{}::{}", rel_path, m_name);
                let mut entity = CodeEntity::new(&module_node_id, m_name, EntityType::Module, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                module_registered = true;

                // Canonical alias node erlang::<module>
                let alias_id = format!("erlang::{}", m_name);
                if graph.get_entity(&alias_id).is_none() {
                    let mut alias_ent = CodeEntity::new(&alias_id, m_name, EntityType::Module, rel_path, line_num, line_num);
                    alias_ent.signature = Some(trimmed.to_string());
                    graph.add_entity(alias_ent);
                }
                graph.add_edge(&alias_id, &module_node_id, EdgeKind::References);
                continue;
            }

            // Ensure module node is registered if encountering items before/without -module
            if !module_registered && (trimmed.starts_with('-') || func_re.is_match(trimmed)) {
                let mut entity = CodeEntity::new(&module_node_id, &current_module, EntityType::Module, rel_path, line_num, line_num);
                entity.signature = Some(format!("-module({}).", current_module));
                graph.add_entity(entity);
                module_registered = true;
            }

            // 2. Behavior declaration
            if let Some(caps) = beh_re.captures(trimmed) {
                let beh_name = caps.get(1).unwrap().as_str();
                let otp_id = format!("otp::{}", beh_name);
                if graph.get_entity(&otp_id).is_none() {
                    let mut otp_ent = CodeEntity::new(&otp_id, beh_name, EntityType::Interface, rel_path, line_num, line_num);
                    otp_ent.signature = Some(format!("-behaviour({}).", beh_name));
                    graph.add_entity(otp_ent);
                }
                graph.add_edge(&module_node_id, &otp_id, EdgeKind::Implements);
                continue;
            }

            // 3. Record declaration
            if let Some(caps) = rec_re.captures(trimmed) {
                let rec_name = caps.get(1).unwrap().as_str();
                let rec_id = format!("{}::record::{}", rel_path, rec_name);
                let mut entity = CodeEntity::new(&rec_id, rec_name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                graph.add_edge(&module_node_id, &rec_id, EdgeKind::Contains);
                continue;
            }

            // 4. Macro definition
            if let Some(caps) = def_re.captures(trimmed) {
                let macro_name = caps.get(1).unwrap().as_str();
                let macro_id = format!("{}::macro::{}", rel_path, macro_name);
                let mut entity = CodeEntity::new(&macro_id, macro_name, EntityType::Variable, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                graph.add_edge(&module_node_id, &macro_id, EdgeKind::Contains);
                continue;
            }

            // 5. Type declaration
            if let Some(caps) = type_re.captures(trimmed) {
                let type_name = caps.get(1).unwrap().as_str();
                let type_id = format!("{}::type::{}", rel_path, type_name);
                let mut entity = CodeEntity::new(&type_id, type_name, EntityType::Struct, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                graph.add_edge(&module_node_id, &type_id, EdgeKind::Contains);
                continue;
            }

            // 6. Include headers
            if let Some(caps) = inc_re.captures(trimmed) {
                let header = caps.get(1).unwrap().as_str();
                let inc_id = format!("{}::include::{}", rel_path, header);
                if graph.get_entity(&inc_id).is_none() {
                    let mut entity = CodeEntity::new(&inc_id, header, EntityType::Module, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                }
                graph.add_edge(&module_node_id, &inc_id, EdgeKind::Imports);
                continue;
            }

            // 7. Function declaration
            if let Some(caps) = func_re.captures(trimmed) {
                let func_name = caps.get(1).unwrap().as_str();
                let args_str = caps.get(2).unwrap().as_str();
                let arity = Self::count_erlang_args(args_str);
                let full_func_name = format!("{}/{}", func_name, arity);
                let func_id = format!("{}::{}", rel_path, full_func_name);

                if graph.get_entity(&func_id).is_none() {
                    let mut entity = CodeEntity::new(&func_id, &full_func_name, EntityType::Function, rel_path, line_num, line_num);
                    entity.signature = Some(trimmed.to_string());
                    graph.add_entity(entity);
                    graph.add_edge(&module_node_id, &func_id, EdgeKind::Contains);
                }
            }

            // 8. Cross-module calls
            for call_caps in call_re.captures_iter(trimmed) {
                let target_mod = call_caps.get(1).unwrap().as_str();
                if target_mod != current_module && target_mod != "erlang" {
                    let target_id = format!("erlang::{}", target_mod);
                    if graph.get_entity(&target_id).is_none() {
                        let target_ent = CodeEntity::new(&target_id, target_mod, EntityType::Module, rel_path, line_num, line_num);
                        graph.add_entity(target_ent);
                    }
                    graph.add_edge(&module_node_id, &target_id, EdgeKind::Calls);
                }
            }
        }
    }

    /// Parse Erlang OTP application specification (.app / .app.src).
    pub fn parse_erlang_app(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let app_re = Regex::new(r#"\{\s*application\s*,\s*([a-zA-Z0-9_]+)"#).unwrap();
        let mod_list_re = Regex::new(r#"\{\s*modules\s*,\s*\[([^\]]+)\]"#).unwrap();
        let app_list_re = Regex::new(r#"\{\s*applications\s*,\s*\[([^\]]+)\]"#).unwrap();
        let mod_cb_re = Regex::new(r#"\{\s*mod\s*,\s*\{\s*([a-zA-Z0-9_]+)"#).unwrap();

        let path = Path::new(rel_path);
        let default_app = path.file_stem().and_then(|s| s.to_str()).unwrap_or("app");

        let app_name = if let Some(caps) = app_re.captures(content) {
            caps.get(1).unwrap().as_str()
        } else {
            default_app
        };

        let app_id = format!("{}::application::{}", rel_path, app_name);
        let mut app_entity = CodeEntity::new(&app_id, app_name, EntityType::Module, rel_path, 1, 1);
        app_entity.signature = Some(format!("{{application, {}, [...]}}", app_name));
        graph.add_entity(app_entity);

        // Modules contained in the application
        if let Some(caps) = mod_list_re.captures(content) {
            let modules_str = caps.get(1).unwrap().as_str();
            for m in modules_str.split(',') {
                let m_name = m.trim().trim_matches(|c| c == '\'' || c == ' ' || c == '\n' || c == '\r');
                if !m_name.is_empty() {
                    let mod_id = format!("erlang::{}", m_name);
                    if graph.get_entity(&mod_id).is_none() {
                        let m_ent = CodeEntity::new(&mod_id, m_name, EntityType::Module, rel_path, 1, 1);
                        graph.add_entity(m_ent);
                    }
                    graph.add_edge(&app_id, &mod_id, EdgeKind::Contains);
                }
            }
        }

        // Dependent applications
        if let Some(caps) = app_list_re.captures(content) {
            let apps_str = caps.get(1).unwrap().as_str();
            for a in apps_str.split(',') {
                let dep_name = a.trim().trim_matches(|c| c == '\'' || c == ' ' || c == '\n' || c == '\r');
                if !dep_name.is_empty() && dep_name != "kernel" && dep_name != "stdlib" {
                    let dep_id = format!("erlang::app::{}", dep_name);
                    if graph.get_entity(&dep_id).is_none() {
                        let d_ent = CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
                        graph.add_entity(d_ent);
                    }
                    graph.add_edge(&app_id, &dep_id, EdgeKind::DependsOn);
                }
            }
        }

        // Application callback module ({mod, {CallbackMod, []}})
        if let Some(caps) = mod_cb_re.captures(content) {
            let cb_mod = caps.get(1).unwrap().as_str();
            let cb_id = format!("erlang::{}", cb_mod);
            if graph.get_entity(&cb_id).is_none() {
                let cb_ent = CodeEntity::new(&cb_id, cb_mod, EntityType::Module, rel_path, 1, 1);
                graph.add_entity(cb_ent);
            }
            graph.add_edge(&app_id, &cb_id, EdgeKind::References);
        }
    }

    /// Parse Rebar3 build configuration (rebar.config) for dependencies.
    pub fn parse_rebar_config(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let deps_re = Regex::new(r#"\{\s*deps\s*,\s*\[([^\]]+)\]"#).unwrap();
        let dep_entry_re = Regex::new(r#"(?:\{\s*([a-zA-Z0-9_]+)|([a-zA-Z0-9_]+))"#).unwrap();

        let config_id = format!("{}::config", rel_path);
        let mut entity = CodeEntity::new(&config_id, "rebar.config", EntityType::Target, rel_path, 1, 1);
        entity.signature = Some("rebar.config".to_string());
        graph.add_entity(entity);

        if let Some(caps) = deps_re.captures(content) {
            let deps_body = caps.get(1).unwrap().as_str();
            for dep_caps in dep_entry_re.captures_iter(deps_body) {
                let dep_name = dep_caps.get(1).or_else(|| dep_caps.get(2)).map(|m| m.as_str()).unwrap_or("");
                if !dep_name.is_empty() && dep_name != "git" {
                    let dep_id = format!("{}::dep::{}", rel_path, dep_name);
                    let mut dep_entity = CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
                    dep_entity.signature = Some(format!("dep: {}", dep_name));
                    graph.add_entity(dep_entity);
                    graph.add_edge(&config_id, &dep_id, EdgeKind::DependsOn);
                }
            }
        }
    }

    /// Check if a file is an Apigee API proxy bundle component or policy XML.
    pub fn is_apigee_file(path: &Path, content: &str) -> bool {
        let components: Vec<_> = path.iter().map(|c| c.to_string_lossy().to_string()).collect();
        if components.iter().any(|c| c == "apiproxy" || c == "sharedflowbundle") {
            return true;
        }

        let trimmed = content.trim_start();
        if !trimmed.starts_with('<') {
            return false;
        }

        let apigee_roots = [
            "<APIProxy",
            "<SharedFlow",
            "<ProxyEndpoint",
            "<TargetEndpoint",
            "<AssignMessage",
            "<VerifyAPIKey",
            "<SpikeArrest",
            "<Quota",
            "<ResponseCache",
            "<OAuthV2",
            "<ServiceCallout",
            "<Javascript",
            "<JavaCallout",
            "<ExtractVariables",
            "<JSONToXML",
            "<XMLToJSON",
            "<RaiseFault",
            "<BasicAuthentication",
            "<JSONThreatProtection",
            "<XMLThreatProtection",
            "<FlowCallout",
            "<KeyValueMapOperations",
            "<AccessEntity",
            "<StatisticsCollector",
        ];

        apigee_roots.iter().any(|root| trimmed.contains(root))
    }

    /// Parse Apigee API proxy manifests, proxy endpoints, target endpoints, and policies.
    pub fn parse_apigee(rel_path: &str, content: &str, graph: &mut CodeGraph) {
        let path = Path::new(rel_path);
        let default_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("apigee_entity");

        let manifest_re = Regex::new(r#"<APIProxy(?:\s+[^>]*?)?\s+name=["']([^"']+)["']|<SharedFlow(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
        let proxy_re = Regex::new(r#"<ProxyEndpoint(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
        let target_re = Regex::new(r#"<TargetEndpoint(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
        let basepath_re = Regex::new(r#"<BasePath>([^<]+)</BasePath>"#).unwrap();
        let url_re = Regex::new(r#"<URL>([^<]+)</URL>"#).unwrap();
        let step_re = Regex::new(r#"<Step>\s*<Name>([^<]+)</Name>"#).unwrap();
        let route_target_re = Regex::new(r#"<RouteRule[^>]*>[\s\S]*?<TargetEndpoint>([^<]+)</TargetEndpoint>"#).unwrap();
        let resource_url_re = Regex::new(r#"<ResourceURL>jsc://([^<]+)</ResourceURL>"#).unwrap();
        let java_class_re = Regex::new(r#"<ClassName>([^<]+)</ClassName>"#).unwrap();

        // 1. APIProxy or SharedFlow Manifest
        if let Some(caps) = manifest_re.captures(content) {
            let proxy_name = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str()).unwrap_or(default_name);
            let manifest_id = format!("apiproxy::{}", proxy_name);
            let basepath = basepath_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");

            let mut entity = CodeEntity::new(&manifest_id, proxy_name, EntityType::Module, rel_path, 1, 1);
            entity.signature = Some(if basepath.is_empty() {
                format!("APIProxy {}", proxy_name)
            } else {
                format!("APIProxy {} (basepath: {})", proxy_name, basepath)
            });
            graph.add_entity(entity);

            // Connect ProxyEndpoints listed in manifest
            let pe_section_re = Regex::new(r#"<ProxyEndpoints>([\s\S]*?)</ProxyEndpoints>"#).unwrap();
            let pe_item_re = Regex::new(r#"<ProxyEndpoint>([^<]+)</ProxyEndpoint>"#).unwrap();
            if let Some(sec) = pe_section_re.captures(content) {
                for pe_caps in pe_item_re.captures_iter(sec.get(1).unwrap().as_str()) {
                    let pe_name = pe_caps.get(1).unwrap().as_str().trim();
                    if !pe_name.is_empty() {
                        let pe_id = format!("apiproxy::proxy::{}", pe_name);
                        if graph.get_entity(&pe_id).is_none() {
                            let pe_ent = CodeEntity::new(&pe_id, pe_name, EntityType::Interface, rel_path, 1, 1);
                            graph.add_entity(pe_ent);
                        }
                        graph.add_edge(&manifest_id, &pe_id, EdgeKind::Contains);
                    }
                }
            }

            // Connect TargetEndpoints listed in manifest
            let te_section_re = Regex::new(r#"<TargetEndpoints>([\s\S]*?)</TargetEndpoints>"#).unwrap();
            let te_item_re = Regex::new(r#"<TargetEndpoint>([^<]+)</TargetEndpoint>"#).unwrap();
            if let Some(sec) = te_section_re.captures(content) {
                for te_caps in te_item_re.captures_iter(sec.get(1).unwrap().as_str()) {
                    let te_name = te_caps.get(1).unwrap().as_str().trim();
                    if !te_name.is_empty() {
                        let te_id = format!("apiproxy::target::{}", te_name);
                        if graph.get_entity(&te_id).is_none() {
                            let te_ent = CodeEntity::new(&te_id, te_name, EntityType::Interface, rel_path, 1, 1);
                            graph.add_entity(te_ent);
                        }
                        graph.add_edge(&manifest_id, &te_id, EdgeKind::Contains);
                    }
                }
            }

            // Connect Policies listed in manifest
            let pol_section_re = Regex::new(r#"<Policies>([\s\S]*?)</Policies>"#).unwrap();
            let pol_item_re = Regex::new(r#"<Policy>([^<]+)</Policy>"#).unwrap();
            if let Some(sec) = pol_section_re.captures(content) {
                for pol_caps in pol_item_re.captures_iter(sec.get(1).unwrap().as_str()) {
                    let pol_name = pol_caps.get(1).unwrap().as_str().trim();
                    if !pol_name.is_empty() {
                        let pol_id = format!("apiproxy::policy::{}", pol_name);
                        if graph.get_entity(&pol_id).is_none() {
                            let pol_ent = CodeEntity::new(&pol_id, pol_name, EntityType::Rule, rel_path, 1, 1);
                            graph.add_entity(pol_ent);
                        }
                        graph.add_edge(&manifest_id, &pol_id, EdgeKind::Contains);
                    }
                }
            }
            return;
        }

        // 2. ProxyEndpoint
        if content.contains("<ProxyEndpoint") {
            let proxy_name = proxy_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or(default_name);
            let proxy_id = format!("apiproxy::proxy::{}", proxy_name);
            let basepath = basepath_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");

            let mut entity = CodeEntity::new(&proxy_id, proxy_name, EntityType::Interface, rel_path, 1, 1);
            entity.signature = Some(if basepath.is_empty() {
                format!("ProxyEndpoint {}", proxy_name)
            } else {
                format!("ProxyEndpoint {} (basepath: {})", proxy_name, basepath)
            });
            graph.add_entity(entity);

            // Policy call steps
            for step_caps in step_re.captures_iter(content) {
                let step_name = step_caps.get(1).unwrap().as_str();
                let pol_id = format!("apiproxy::policy::{}", step_name);
                if graph.get_entity(&pol_id).is_none() {
                    let pol_entity = CodeEntity::new(&pol_id, step_name, EntityType::Rule, rel_path, 1, 1);
                    graph.add_entity(pol_entity);
                }
                graph.add_edge(&proxy_id, &pol_id, EdgeKind::Calls);
            }

            // TargetEndpoint route rules
            for route_caps in route_target_re.captures_iter(content) {
                let target_name = route_caps.get(1).unwrap().as_str();
                let target_id = format!("apiproxy::target::{}", target_name);
                if graph.get_entity(&target_id).is_none() {
                    let target_entity = CodeEntity::new(&target_id, target_name, EntityType::Interface, rel_path, 1, 1);
                    graph.add_entity(target_entity);
                }
                graph.add_edge(&proxy_id, &target_id, EdgeKind::DependsOn);
            }
            return;
        }

        // 3. TargetEndpoint
        if content.contains("<TargetEndpoint") {
            let target_name = target_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or(default_name);
            let target_id = format!("apiproxy::target::{}", target_name);
            let url = url_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");

            let mut entity = CodeEntity::new(&target_id, target_name, EntityType::Interface, rel_path, 1, 1);
            entity.signature = Some(if url.is_empty() {
                format!("TargetEndpoint {}", target_name)
            } else {
                format!("TargetEndpoint {} (url: {})", target_name, url)
            });
            graph.add_entity(entity);

            // Policy call steps in target
            for step_caps in step_re.captures_iter(content) {
                let step_name = step_caps.get(1).unwrap().as_str();
                let pol_id = format!("apiproxy::policy::{}", step_name);
                if graph.get_entity(&pol_id).is_none() {
                    let pol_entity = CodeEntity::new(&pol_id, step_name, EntityType::Rule, rel_path, 1, 1);
                    graph.add_entity(pol_entity);
                }
                graph.add_edge(&target_id, &pol_id, EdgeKind::Calls);
            }
            return;
        }

        // 4. Policy definition
        let policy_tag_re = Regex::new(r#"<([A-Za-z0-9]+)(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
        if let Some(caps) = policy_tag_re.captures(content) {
            let pol_type = caps.get(1).unwrap().as_str();
            let pol_name = caps.get(2).unwrap().as_str();
            let pol_id = format!("apiproxy::policy::{}", pol_name);

            let mut entity = CodeEntity::new(&pol_id, pol_name, EntityType::Rule, rel_path, 1, 1);
            entity.signature = Some(format!("Policy {} ({})", pol_name, pol_type));
            graph.add_entity(entity);

            // If Javascript policy, extract resource URL
            if let Some(res_caps) = resource_url_re.captures(content) {
                let script_name = res_caps.get(1).unwrap().as_str();
                let res_id = format!("apiproxy::resource::{}", script_name);
                if graph.get_entity(&res_id).is_none() {
                    let res_entity = CodeEntity::new(&res_id, script_name, EntityType::Function, rel_path, 1, 1);
                    graph.add_entity(res_entity);
                }
                graph.add_edge(&pol_id, &res_id, EdgeKind::References);
            }

            // If JavaCallout policy, extract ClassName
            if let Some(cls_caps) = java_class_re.captures(content) {
                let class_name = cls_caps.get(1).unwrap().as_str();
                let cls_id = format!("java::{}", class_name);
                if graph.get_entity(&cls_id).is_none() {
                    let cls_entity = CodeEntity::new(&cls_id, class_name, EntityType::Class, rel_path, 1, 1);
                    graph.add_entity(cls_entity);
                }
                graph.add_edge(&pol_id, &cls_id, EdgeKind::References);
            }
        }
    }

    /// Universal parser router for supported source code languages and markup formats.
    pub fn parse_generic_source(rel_path: &str, content: &str, lang: &str, graph: &mut CodeGraph) {
        match lang {
            "typescript" | "javascript" | "ts" | "js" => {
                Self::parse_typescript(rel_path, content, graph);
                return;
            }
            "go" => {
                Self::parse_go(rel_path, content, graph);
                return;
            }
            "c" => {
                Self::parse_c(rel_path, content, graph);
                return;
            }
            "cpp" | "c++" => {
                Self::parse_cpp(rel_path, content, graph);
                return;
            }
            "java" => {
                Self::parse_java(rel_path, content, graph);
                return;
            }
            "xml" => {
                Self::parse_xml(rel_path, content, graph);
                return;
            }
            "html" => {
                Self::parse_html(rel_path, content, graph);
                return;
            }
            "plsql" | "oracle" => {
                Self::parse_plsql(rel_path, content, graph);
                return;
            }
            "apex" | "cls" => {
                Self::parse_apex_class(rel_path, content, graph);
                return;
            }
            "trigger" => {
                Self::parse_apex_trigger(rel_path, content, graph);
                return;
            }
            "soql" => {
                Self::parse_soql(rel_path, content, graph);
                return;
            }
            "aura" => {
                Self::parse_aura(rel_path, content, graph);
                return;
            }
            "erlang" | "erl" | "hrl" => {
                Self::parse_erlang_module(rel_path, content, graph);
                return;
            }
            "apigee" => {
                Self::parse_apigee(rel_path, content, graph);
                return;
            }
            _ => {}
        }

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

    /// Index a single file, routing to the appropriate language parser or sidecar.
    pub async fn index_single_file(
        repo_root: &Path,
        path: &Path,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
    ) -> Result<bool> {
        if !path.is_file() || Self::is_binary_file(path) {
            return Ok(false);
        }

        let rel_path = match path.strip_prefix(repo_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => path.to_string_lossy().to_string(),
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // 1. Check for Makefiles
        if file_name == "Makefile" || file_name == "GNUmakefile" || extension == "mk" {
            Self::parse_makefile(&rel_path, &content, graph);
            return Ok(true);
        }

        // 2. Check for SQL & PL/SQL files
        if matches!(extension, "sql" | "pls" | "pks" | "pkb" | "prc" | "fnc" | "trg") {
            let dialect = if matches!(extension, "pls" | "pks" | "pkb" | "prc" | "fnc" | "trg") {
                Some("oracle")
            } else {
                None
            };
            match sidecar.parse_sql(&rel_path, &content, dialect).await {
                Ok(entities) => {
                    for ent in entities {
                        graph.add_entity(ent);
                    }
                }
                Err(err) => {
                    debug!("Sidecar SQL parse fallback for {}: {}", rel_path, err);
                }
            }
            return Ok(true);
        }

        // 3. Shell scripts
        let is_shell_ext = matches!(extension, "sh" | "bash" | "zsh");
        let is_shell_shebang = Self::detect_shebang(content.as_bytes()) == Some("bash");
        if is_shell_ext || is_shell_shebang {
            match sidecar.parse_shell(&rel_path, &content).await {
                Ok(entities) => {
                    for ent in entities {
                        graph.add_entity(ent);
                    }
                }
                Err(err) => {
                    debug!("Sidecar Shell parse fallback for {}: {}", rel_path, err);
                }
            }
            return Ok(true);
        }

        // 4. Standard programming languages & markup formats
        let shebang = Self::detect_shebang(content.as_bytes());
        if matches!(extension, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") || shebang == Some("typescript") {
            Self::parse_typescript(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "go") {
            Self::parse_go(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "c" | "h") {
            Self::parse_c(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "cpp" | "cc" | "cxx" | "hpp" | "hxx") {
            Self::parse_cpp(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "java") {
            Self::parse_java(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "cls") {
            Self::parse_apex_class(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "trigger") {
            Self::parse_apex_trigger(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".object-meta.xml") || file_name.ends_with(".field-meta.xml") || matches!(extension, "object" | "field") {
            Self::parse_salesforce_metadata(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "soql") {
            Self::parse_soql(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "erl" | "hrl" | "escript") || shebang == Some("erlang") {
            Self::parse_erlang_module(&rel_path, &content, graph);
            Ok(true)
        } else if file_name.ends_with(".app.src") {
            Self::parse_erlang_app(&rel_path, &content, graph);
            Ok(true)
        } else if file_name == "rebar.config" {
            Self::parse_rebar_config(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "cmp" | "evt" | "intf") {
            Self::parse_aura(&rel_path, &content, graph);
            Ok(true)
        } else if matches!(extension, "app") {
            if content.trim_start().starts_with('<') {
                Self::parse_aura(&rel_path, &content, graph);
            } else {
                Self::parse_erlang_app(&rel_path, &content, graph);
            }
            Ok(true)
        } else if matches!(extension, "xml" | "xsd" | "pom") {
            if Self::is_apigee_file(path, &content) {
                Self::parse_apigee(&rel_path, &content, graph);
            } else {
                Self::parse_xml(&rel_path, &content, graph);
            }
            Ok(true)
        } else if matches!(extension, "html" | "htm") {
            Self::parse_html(&rel_path, &content, graph);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Index the entire repository, coordinating native parsing and the Python sidecar.
    pub async fn index_repository(&self, graph: &mut CodeGraph, sidecar: &SidecarClient) -> Result<usize> {
        let stats = self.sync_repository(graph, sidecar, true).await?;
        Ok(stats.added + stats.modified)
    }

    /// Incremental repository synchronization using mtime and content hashes.
    pub async fn sync_repository(
        &self,
        graph: &mut CodeGraph,
        sidecar: &SidecarClient,
        force: bool,
    ) -> Result<SyncStats> {
        let start = std::time::Instant::now();
        let manifest_path = self.repo_root.join(".rumin/manifest.json");
        let mut manifest = if force {
            FileManifest::default()
        } else {
            FileManifest::load_or_new(&manifest_path)
        };

        let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut stats = SyncStats::default();

        let walker = WalkDir::new(&self.repo_root).into_iter().filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | "venv" | "__pycache__" | ".aimem" | ".cargo" | ".rumin"
            )
        });

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || Self::is_binary_file(path) {
                continue;
            }

            let rel_path = match path.strip_prefix(&self.repo_root) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => path.to_string_lossy().to_string(),
            };
            seen_paths.insert(rel_path.clone());

            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mtime_secs = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let mtime_nanos = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);

            // Fast-path: if not forced and mtime matches manifest, skip file read & hash
            // Supports both legacy second-precision manifests (old_mtime <= 100_000_000_000) and nanosecond manifests
            if !force {
                if let Some((old_mtime, _old_hash)) = manifest.files.get(&rel_path) {
                    let matches_mtime = if *old_mtime > 100_000_000_000 {
                        *old_mtime == mtime_nanos
                    } else {
                        *old_mtime == mtime_secs
                    };

                    if matches_mtime {
                        stats.unchanged += 1;
                        continue;
                    }
                }
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let hash = {
                let mut hasher = Sha256::new();
                hasher.update(content.as_bytes());
                format!("{:x}", hasher.finalize())
            };

            if let Some((old_mtime, old_hash)) = manifest.files.get(&rel_path) {
                let matches_mtime = if *old_mtime > 100_000_000_000 {
                    *old_mtime == mtime_nanos
                } else {
                    *old_mtime == mtime_secs
                };

                if !force && matches_mtime && *old_hash == hash {
                    stats.unchanged += 1;
                    continue;
                }
                // File modified: prune existing entities and re-index
                graph.remove_file(&rel_path);
                if Self::index_single_file(&self.repo_root, path, graph, sidecar).await? {
                    stats.modified += 1;
                }
            } else {
                // New file
                if Self::index_single_file(&self.repo_root, path, graph, sidecar).await? {
                    stats.added += 1;
                }
            }

            manifest.files.insert(rel_path, (mtime_nanos, hash));
        }

        // Detect deleted files
        let to_remove: Vec<String> = manifest
            .files
            .keys()
            .filter(|p| !seen_paths.contains(*p))
            .cloned()
            .collect();

        for rel_path in to_remove {
            graph.remove_file(&rel_path);
            manifest.files.remove(&rel_path);
            stats.deleted += 1;
        }

        let _ = manifest.save(&manifest_path);
        let default_checkpoint = self.repo_root.join(".rumin/checkpoints/default.bin");
        if let Some(parent) = default_checkpoint.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = graph.save_binary_checkpoint(&default_checkpoint);
        stats.total_indexed = graph.node_count();
        stats.duration_ms = start.elapsed().as_millis();

        info!(
            "Sync completed in {}ms: +{} added, ~{} modified, -{} deleted, {} unchanged",
            stats.duration_ms, stats.added, stats.modified, stats.deleted, stats.unchanged
        );
        Ok(stats)
    }
}

/// Statistics returned by incremental sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub total_indexed: usize,
    pub duration_ms: u128,
}

/// On-disk manifest caching file mtimes and SHA256 hashes for incremental sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileManifest {
    pub files: HashMap<String, (u64, String)>,
}

impl FileManifest {
    pub fn load_or_new(manifest_path: &Path) -> Self {
        if manifest_path.exists() {
            if let Ok(content) = std::fs::read_to_string(manifest_path) {
                if let Ok(manifest) = serde_json::from_str(&content) {
                    return manifest;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, manifest_path: &Path) -> Result<()> {
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(manifest_path, json)?;
        Ok(())
    }
}
