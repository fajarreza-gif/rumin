use std::path::Path;
use regex::Regex;
use crate::graph::{CodeGraph, CodeEntity, EntityType, EdgeKind};

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
