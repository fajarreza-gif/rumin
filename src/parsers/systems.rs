use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};
use regex::Regex;

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

/// Parse C++ source and header files into classes, structs, namespaces, methods, and functions.
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

/// Parse Rust source files into modules, structs, enums, traits, functions, methods, and impl blocks.
pub fn parse_rust(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let mod_re = Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-zA-Z0-9_]+)").unwrap();
    let struct_re = Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([a-zA-Z0-9_]+)").unwrap();
    let enum_re = Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+([a-zA-Z0-9_]+)").unwrap();
    let trait_re = Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+([a-zA-Z0-9_]+)").unwrap();
    let fn_re = Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern(?:\s+#[^#]+#)?\s+)?fn\s+([a-zA-Z0-9_]+)").unwrap();
    let impl_re = Regex::new(r"^\s*impl(?:\s*<[^>]+>)?\s+(?:([a-zA-Z0-9_:]+)\s+for\s+)?([a-zA-Z0-9_:]+)").unwrap();
    let use_re = Regex::new(r"^\s*(?:pub\s+)?use\s+([a-zA-Z0-9_:]+)").unwrap();

    let mut current_impl: Option<String> = None;
    let mut brace_depth = 0;
    let mut impl_brace_depth = 0;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = impl_re.captures(trimmed) {
            let target = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            current_impl = Some(target.split("::").last().unwrap_or(target).to_string());
            impl_brace_depth = brace_depth;

            if let Some(trait_match) = caps.get(1) {
                let trait_name = trait_match.as_str();
                let struct_id = format!("{}::{}", rel_path, target);
                let trait_id = format!("{}::{}", rel_path, trait_name);
                graph.add_edge(&struct_id, &trait_id, EdgeKind::Implements);
            }
        }

        if let Some(caps) = mod_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::mod::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Module, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = struct_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = enum_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = trait_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Interface, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
        } else if let Some(caps) = fn_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let (full_name, id, ent_type) = if let Some(ref impl_target) = current_impl {
                let full = format!("{}::{}", impl_target, name);
                let id = format!("{}::{}", rel_path, full);
                (full, id, EntityType::Method)
            } else {
                let id = format!("{}::{}", rel_path, name);
                (name.to_string(), id, EntityType::Function)
            };

            let mut entity = CodeEntity::new(&id, &full_name, ent_type, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);

            if let Some(ref impl_target) = current_impl {
                let struct_id = format!("{}::{}", rel_path, impl_target);
                graph.add_edge(&struct_id, &id, EdgeKind::Contains);
            }
        } else if let Some(caps) = use_re.captures(trimmed) {
            let target = caps.get(1).unwrap().as_str();
            let src_id = format!("{}::file", rel_path);
            graph.add_edge(&src_id, target, EdgeKind::Imports);
        }

        let open = line.chars().filter(|&c| c == '{').count();
        let close = line.chars().filter(|&c| c == '}').count();
        brace_depth = (brace_depth + open).saturating_sub(close);
        if current_impl.is_some() && brace_depth <= impl_brace_depth && close > 0 {
            current_impl = None;
        }
    }
}
