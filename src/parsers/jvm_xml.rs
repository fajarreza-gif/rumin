use regex::Regex;
use crate::graph::{CodeGraph, CodeEntity, EntityType};
use crate::types::EdgeKind;

/// Parse Java source files into classes, interfaces, enums, records, packages, imports, and methods.
pub fn parse_java(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let pkg_re = Regex::new(r"^\s*package\s+([a-zA-Z0-9_.]+);").unwrap();
    let class_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+|abstract\s+|static\s+|final\s+)*class\s+([a-zA-Z0-9_]+)").unwrap();
    let iface_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+)*interface\s+([a-zA-Z0-9_]+)").unwrap();
    let enum_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+)*enum\s+([a-zA-Z0-9_]+)").unwrap();
    let record_re = Regex::new(r"^\s*(?:public\s+|protected\s+|private\s+)*record\s+([a-zA-Z0-9_]+)").unwrap();
    let method_re = Regex::new(r"^\s*(?:public|protected|private|static|final|synchronized|abstract|default|native)\s+(?:[\w<>\[\],\s?]+)\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*(?:throws\s+[\w,\s]+)?\s*\{").unwrap();
    let import_re = Regex::new(r"^\s*import\s+(?:static\s+)?([a-zA-Z0-9_.*]+);").unwrap();

    let file_id = format!("{}::file", rel_path);
    let mut file_entity = CodeEntity::new(&file_id, rel_path, EntityType::Module, rel_path, 1, 1);
    file_entity.signature = Some(format!("file {}", rel_path));
    graph.add_entity(file_entity);

    let mut current_class: Option<(String, usize)> = None;
    let mut brace_depth = 0;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = import_re.captures(trimmed) {
            let target = caps.get(1).unwrap().as_str();
            let target_id = format!("java::{}", target);
            if graph.get_entity(&target_id).is_none() {
                let mut ext = CodeEntity::new(&target_id, target, EntityType::Module, "<external>", line_num, line_num);
                ext.signature = Some(format!("import {};", target));
                graph.add_entity(ext);
            }
            graph.add_edge(&file_id, &target_id, EdgeKind::Imports);
        } else if let Some(caps) = pkg_re.captures(trimmed) {
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
            current_class = Some((name.to_string(), brace_depth));
        } else if let Some(caps) = iface_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Interface, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            current_class = Some((name.to_string(), brace_depth));
        } else if let Some(caps) = enum_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            current_class = Some((name.to_string(), brace_depth));
        } else if let Some(caps) = record_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            current_class = Some((name.to_string(), brace_depth));
        } else if let Some(caps) = method_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            if !matches!(name, "if" | "while" | "for" | "switch" | "catch" | "return") {
                let id = format!("{}::{}", rel_path, name);
                let mut entity = CodeEntity::new(&id, name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);

                if let Some((ref cls, _)) = current_class {
                    let class_id = format!("{}::{}", rel_path, cls);
                    graph.add_edge(&class_id, &id, EdgeKind::Contains);

                    let qual_id = format!("{}::{}.{}", rel_path, cls, name);
                    if qual_id != id {
                        let full = format!("{}.{}", cls, name);
                        let mut qual_entity = CodeEntity::new(&qual_id, &full, EntityType::Method, rel_path, line_num, line_num);
                        qual_entity.signature = Some(trimmed.to_string());
                        graph.add_entity(qual_entity);
                        graph.add_edge(&class_id, &qual_id, EdgeKind::Contains);
                    }
                }
            }
        }

        let open = line.chars().filter(|&c| c == '{').count();
        let close = line.chars().filter(|&c| c == '}').count();
        brace_depth = (brace_depth + open).saturating_sub(close);
        if let Some((_, class_depth)) = current_class {
            if brace_depth <= class_depth && close > 0 {
                current_class = None;
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
            if let Some(id_caps) = id_re.captures(attrs) {
                let name = format!("{}#{}", raw_tag, id_caps.get(1).unwrap().as_str());
                let id = format!("{}::{}::{}", rel_path, name, line_num);
                let mut entity = CodeEntity::new(&id, &name, EntityType::Tag, rel_path, line_num, line_num);
                entity.signature = Some(caps.get(0).unwrap().as_str().to_string());
                graph.add_entity(entity);
            }
        }
    }
}

/// Link a foreign key reference to a target table, resolving same-file, existing, or predicted paths.
fn link_table_reference(
    from_tbl_id: &str,
    target_table: &str,
    rel_path: &str,
    target_schema: Option<&str>,
    graph: &mut CodeGraph,
) {
    // 1. Same-file check
    let same_file_id = format!("{}::{}", rel_path, target_table);
    if graph.get_index(&same_file_id).is_some() {
        graph.add_edge(from_tbl_id, &same_file_id, EdgeKind::References);
        link_target_primary_key(from_tbl_id, target_table, graph);
        return;
    }

    // 2. Existing node lookup in graph
    if let Some(target_idx) = graph.find_node_by_name_or_id(target_table) {
        let existing_id = graph.graph[target_idx].id.clone();
        graph.add_edge(from_tbl_id, &existing_id, EdgeKind::References);
        link_target_primary_key(from_tbl_id, target_table, graph);
        return;
    }
    if let Some(s) = target_schema {
        let qual = format!("{}.{}", s, target_table);
        if let Some(target_idx) = graph.find_node_by_name_or_id(&qual) {
            let existing_id = graph.graph[target_idx].id.clone();
            graph.add_edge(from_tbl_id, &existing_id, EdgeKind::References);
            link_target_primary_key(from_tbl_id, target_table, graph);
            return;
        }
    }

    // 3. Predict path in DDL directory (e.g. "catalist/ddl/s_org_ext.sql")
    let parent = std::path::Path::new(rel_path).parent();
    let file_candidate = format!("{}.sql", target_table.to_lowercase());
    let predicted_rel_path = match parent {
        Some(p) if !p.as_os_str().is_empty() => format!("{}/{}", p.display(), file_candidate),
        _ => file_candidate,
    };
    let predicted_id = format!("{}::{}", predicted_rel_path, target_table);

    if graph.get_index(&predicted_id).is_none() {
        let mut placeholder = CodeEntity::new(
            &predicted_id,
            target_table,
            EntityType::Table,
            &predicted_rel_path,
            1,
            1,
        );
        placeholder.signature = Some(format!("TABLE {}", target_table));
        if let Some(s) = target_schema {
            placeholder.metadata.insert("schema".to_string(), s.to_string());
            placeholder.metadata.insert("qualified_name".to_string(), format!("{}.{}", s, target_table));
        }
        placeholder.metadata.insert("short_name".to_string(), target_table.to_string());
        graph.add_entity(placeholder);

        if let Some(s) = target_schema {
            let qualified = format!("{}.{}", s, target_table);
            let mut alias_ent = CodeEntity::new(&qualified, &qualified, EntityType::Table, &predicted_rel_path, 1, 1);
            alias_ent.metadata.insert("is_alias_for".to_string(), predicted_id.clone());
            alias_ent.metadata.insert("schema".to_string(), s.to_string());
            alias_ent.metadata.insert("short_name".to_string(), target_table.to_string());
            graph.add_entity(alias_ent);
            graph.add_edge(&predicted_id, &qualified, EdgeKind::References);
            graph.add_edge(&qualified, &predicted_id, EdgeKind::References);
        }
    }

    graph.add_edge(from_tbl_id, &predicted_id, EdgeKind::References);
    link_target_primary_key(from_tbl_id, target_table, graph);
}

/// Helper to connect foreign key column to target table primary key (e.g. ROW_ID, id)
fn link_target_primary_key(from_id: &str, target_table: &str, graph: &mut CodeGraph) {
    if !from_id.contains('.') {
        return;
    }
    let candidates = [
        format!("{}.ROW_ID", target_table),
        format!("{}.row_id", target_table),
        format!("{}.id", target_table),
        format!("{}.ID", target_table),
    ];
    for cand in &candidates {
        if let Some(idx) = graph.find_node_by_name_or_id(cand) {
            let target_pk_id = graph.graph[idx].id.clone();
            if from_id != target_pk_id {
                graph.add_edge(from_id, &target_pk_id, EdgeKind::References);
                break;
            }
        }
    }
}

/// Parse Oracle PL/SQL files into packages, procedures, functions, triggers, types, sequences, and tables.
pub fn parse_plsql(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let pkg_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?PACKAGE\s+(?:BODY\s+)?(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let proc_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?PROCEDURE\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let func_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let trig_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?TRIGGER\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let type_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?TYPE\s+(?:BODY\s+)?(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let seq_re = Regex::new(r"(?i)CREATE\s+SEQUENCE\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let table_re = Regex::new(r"(?i)CREATE\s+(?:OR\s+REPLACE\s+)?(?:FORCE\s+)?(?:EDITIONABLE\s+)?(TABLE|VIEW|MATERIALIZED\s+VIEW)\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let member_proc_re = Regex::new(r"(?i)^\s*PROCEDURE\s+([`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?)\s*(?:\(|;|$)").unwrap();
    let member_func_re = Regex::new(r"(?i)^\s*FUNCTION\s+([`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?)\s*(?:\(|;|$)").unwrap();
    let col_re = Regex::new(r"(?i)^\s*[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\s+([a-zA-Z0-9_#$]+(?:\s*\([^)]+\))?)").unwrap();
    let fk_re = Regex::new(r"(?i)REFERENCES\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();
    let index_re = Regex::new(r"(?i)CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\s+ON\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\s*\(([^)]+)\)").unwrap();
    let row_count_re = Regex::new(r"(?i)--\s*Columns:\s*\d+\s*\|\s*Recorded\s+Rows:\s*([0-9,]+)").unwrap();
    let comment_re = Regex::new(r"(?i)COMMENT\s+ON\s+COLUMN\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\s+IS\s+'([^']*)'").unwrap();
    let end_re = Regex::new(r"(?i)^\s*END(?:\s+[a-zA-Z0-9_#$]+)?\s*;").unwrap();

    let clean = |s: &str| s.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string();
    let mut current_package: Option<String> = None;
    let mut current_table: Option<(String, String, Option<String>, Option<String>)> = None;
    let mut pending_recorded_rows: Option<u64> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        if let Some(caps) = row_count_re.captures(trimmed) {
            let rows_str = caps.get(1).unwrap().as_str().replace(',', "");
            if let Ok(rows) = rows_str.parse::<u64>() {
                pending_recorded_rows = Some(rows);
            }
        }

        if trimmed.starts_with("--") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = pkg_re.captures(line) {
            let schema_opt = caps.get(1).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(2).unwrap().as_str());
            let name = match &schema_opt {
                Some(s) => format!("{}.{}", s, raw_name),
                None => raw_name,
            };
            current_package = Some(name.clone());
            let id = format!("{}::pkg::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Module, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            if let Some(schema) = &schema_opt {
                entity.metadata.insert("schema".to_string(), schema.clone());
            }
            graph.add_entity(entity);
        } else if let Some(caps) = proc_re.captures(line) {
            let schema_opt = caps.get(1).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(2).unwrap().as_str());
            let name = match &schema_opt {
                Some(s) => format!("{}.{}", s, raw_name),
                None => raw_name,
            };
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            if let Some(schema) = &schema_opt {
                entity.metadata.insert("schema".to_string(), schema.clone());
            }
            graph.add_entity(entity);
        } else if let Some(caps) = func_re.captures(line) {
            let schema_opt = caps.get(1).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(2).unwrap().as_str());
            let name = match &schema_opt {
                Some(s) => format!("{}.{}", s, raw_name),
                None => raw_name,
            };
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Function, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            if let Some(schema) = &schema_opt {
                entity.metadata.insert("schema".to_string(), schema.clone());
            }
            graph.add_entity(entity);
        } else if let Some(caps) = trig_re.captures(line) {
            let schema_opt = caps.get(1).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(2).unwrap().as_str());
            let name = match &schema_opt {
                Some(s) => format!("{}.{}", s, raw_name),
                None => raw_name,
            };
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Rule, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            if let Some(schema) = &schema_opt {
                entity.metadata.insert("schema".to_string(), schema.clone());
            }
            graph.add_entity(entity);
        } else if let Some(caps) = type_re.captures(line) {
            let schema_opt = caps.get(1).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(2).unwrap().as_str());
            let name = match &schema_opt {
                Some(s) => format!("{}.{}", s, raw_name),
                None => raw_name,
            };
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            if let Some(schema) = &schema_opt {
                entity.metadata.insert("schema".to_string(), schema.clone());
            }
            graph.add_entity(entity);
        } else if let Some(caps) = seq_re.captures(line) {
            let schema_opt = caps.get(1).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(2).unwrap().as_str());
            let name = match &schema_opt {
                Some(s) => format!("{}.{}", s, raw_name),
                None => raw_name,
            };
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, &name, EntityType::Variable, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            if let Some(schema) = &schema_opt {
                entity.metadata.insert("schema".to_string(), schema.clone());
            }
            graph.add_entity(entity);
        } else if let Some(caps) = table_re.captures(line) {
            let kind_str = caps.get(1).unwrap().as_str().to_ascii_lowercase();
            let schema_opt = caps.get(2).map(|m| clean(m.as_str()));
            let raw_name = clean(caps.get(3).unwrap().as_str());
            let ent_type = if kind_str.contains("view") { EntityType::View } else { EntityType::Table };
            let primary_id = format!("{}::{}", rel_path, raw_name);
            let mut entity = CodeEntity::new(&primary_id, &raw_name, ent_type.clone(), rel_path, line_num, line_num);
            entity.signature = Some(format!("{} {}", kind_str.to_uppercase(), raw_name));
            entity.metadata.insert("short_name".to_string(), raw_name.clone());
            if let Some(rows) = pending_recorded_rows.take() {
                entity.metadata.insert("recorded_rows".to_string(), rows.to_string());
            }
            if let Some(schema) = &schema_opt {
                let qualified = format!("{}.{}", schema, raw_name);
                entity.metadata.insert("schema".to_string(), schema.clone());
                entity.metadata.insert("qualified_name".to_string(), qualified);
            }
            let recorded_rows_opt = entity.metadata.get("recorded_rows").cloned();
            graph.add_entity(entity);

            let alias_id = if let Some(schema) = &schema_opt {
                let qualified = format!("{}.{}", schema, raw_name);
                let mut alias_entity = CodeEntity::new(&qualified, &qualified, ent_type.clone(), rel_path, line_num, line_num);
                alias_entity.metadata.insert("is_alias_for".to_string(), primary_id.clone());
                alias_entity.metadata.insert("schema".to_string(), schema.clone());
                alias_entity.metadata.insert("short_name".to_string(), raw_name.clone());
                if let Some(rows_str) = &recorded_rows_opt {
                    alias_entity.metadata.insert("recorded_rows".to_string(), rows_str.clone());
                }
                graph.add_entity(alias_entity);
                graph.add_edge(&primary_id, &qualified, EdgeKind::References);
                graph.add_edge(&qualified, &primary_id, EdgeKind::References);

                let path_qualified = format!("{}::{}", rel_path, qualified);
                let mut path_alias = CodeEntity::new(&path_qualified, &qualified, ent_type, rel_path, line_num, line_num);
                path_alias.metadata.insert("is_alias_for".to_string(), primary_id.clone());
                if let Some(rows_str) = &recorded_rows_opt {
                    path_alias.metadata.insert("recorded_rows".to_string(), rows_str.clone());
                }
                graph.add_entity(path_alias);
                graph.add_edge(&primary_id, &path_qualified, EdgeKind::References);
                graph.add_edge(&path_qualified, &primary_id, EdgeKind::References);

                Some(qualified)
            } else {
                None
            };

            current_table = Some((raw_name, primary_id, alias_id, schema_opt));
        } else if let Some((raw_tbl_name, tbl_id, alias_id_opt, schema_opt)) = &current_table {
            if trimmed.starts_with(')') {
                current_table = None;
            } else {
                // 1. Explicit SQL REFERENCES
                if let Some(fk_caps) = fk_re.captures(trimmed) {
                    let fk_schema_opt = fk_caps.get(1).map(|m| clean(m.as_str()));
                    let target_raw_tbl = clean(fk_caps.get(2).unwrap().as_str());
                    link_table_reference(tbl_id, &target_raw_tbl, rel_path, fk_schema_opt.as_deref().or(schema_opt.as_deref()), graph);
                    if let Some(alias_id) = alias_id_opt {
                        link_table_reference(alias_id, &target_raw_tbl, rel_path, fk_schema_opt.as_deref().or(schema_opt.as_deref()), graph);
                    }
                }

                // 2. Column definition & Siebel FK conventions
                if let Some(col_caps) = col_re.captures(trimmed) {
                    let col_name = col_caps.get(1).unwrap().as_str();
                    let col_upper = col_name.to_ascii_uppercase();
                    let reserved = ["CONSTRAINT", "PRIMARY", "FOREIGN", "KEY", "CHECK", "UNIQUE", "INDEX", "PARTITION", "STORAGE", "TABLESPACE", "CREATE", "TABLE", "VIEW", "ALTER"];
                    if !reserved.contains(&col_upper.as_str()) {
                        let col_type = col_caps.get(2).unwrap().as_str().trim();
                        let col_id = format!("{}::{}.{}", rel_path, raw_tbl_name, col_name);
                        let col_display_name = format!("{}.{}", raw_tbl_name, col_name);
                        let mut col_entity = CodeEntity::new(&col_id, &col_display_name, EntityType::Column, rel_path, line_num, line_num);
                        col_entity.signature = Some(format!("{} {}", col_name, col_type));
                        col_entity.metadata.insert("short_name".to_string(), col_name.to_string());
                        col_entity.metadata.insert("table_name".to_string(), raw_tbl_name.clone());
                        if let Some(s) = schema_opt {
                            col_entity.metadata.insert("schema".to_string(), s.clone());
                        }
                        // Siebel convention FK linkage
                        let siebel_target_table: Option<String> = if col_upper == "BILL_ACCNT_ID" || col_upper == "OWNER_ACCNT_ID" || col_upper == "PR_ACCNT_ID" || col_upper == "ACCNT_ID" || (col_upper.ends_with("_ACCNT_ID") && !col_upper.contains("TYPE")) {
                            Some("S_ORG_EXT".to_string())
                        } else if col_upper == "BU_ID" || col_upper.ends_with("_BU_ID") {
                            Some("S_BU".to_string())
                        } else if col_upper == "OU_ID" || col_upper.ends_with("_OU_ID") {
                            Some("S_ORG_EXT".to_string())
                        } else if col_upper == "PAR_ROW_ID" {
                            Some(raw_tbl_name.clone())
                        } else if col_upper == "CAMP_ID" || col_upper == "SRC_ID" || col_upper.ends_with("_SRC_ID") {
                            Some("S_SRC".to_string())
                        } else if col_upper == "POSTN_ID" || col_upper.ends_with("_POSTN_ID") {
                            Some("S_POSTN".to_string())
                        } else if col_upper == "PROD_ID" || col_upper == "PORT_VALID_PROD_ID" || (col_upper.ends_with("_PROD_ID") && !col_upper.contains("RULE")) {
                            Some("S_PROD_INT".to_string())
                        } else if col_upper == "CUR_AGREE_ID" || col_upper == "AGREE_ID" || col_upper.ends_with("_AGREE_ID") {
                            Some("S_DOC_AGREE".to_string())
                        } else if col_upper == "ORDER_ID" || col_upper.ends_with("_ORDER_ID") {
                            Some("S_ORDER".to_string())
                        } else if col_upper == "ORDER_ITEM_ID" || (col_upper.ends_with("_ITEM_ID") && col_upper.contains("ORDER")) {
                            Some("S_ORDER_ITEM".to_string())
                        } else if col_upper == "CON_ID" || col_upper == "PR_CON_ID" || col_upper == "CONTACT_ID" || col_upper == "PRI_EMP_ID" || col_upper.ends_with("_EMP_ID") {
                            Some("S_CONTACT".to_string())
                        } else if col_upper == "DOC_QUOTE_ID" || col_upper == "QUOTE_ID" || col_upper.ends_with("_QUOTE_ID") {
                            Some("S_DOC_QUOTE".to_string())
                        } else if col_upper == "ROOT_ASSET_ID" || col_upper == "PARENT_ASSET_ID" || col_upper == "ASSET_ID" {
                            Some("S_ASSET".to_string())
                        } else if col_upper == "INV_PROF_ID" || col_upper.ends_with("_INV_PROF_ID") {
                            Some("S_INV_PROF".to_string())
                        } else if col_upper == "PRI_LST_ID" || col_upper == "COST_LST_ID" || col_upper.ends_with("_PRI_LST_ID") {
                            Some("S_PRI_LST".to_string())
                        } else if col_upper == "OPTY_ID" || col_upper.ends_with("_OPTY_ID") {
                            Some("S_OPTY".to_string())
                        } else if col_upper == "SRV_REQ_ID" || col_upper.ends_with("_SRV_REQ_ID") || col_upper == "CASE_ID" {
                            Some("S_SRV_REQ".to_string())
                        } else if col_upper == "PARTY_ID" || col_upper.ends_with("_PARTY_ID") {
                            Some("S_PARTY".to_string())
                        } else if col_upper.ends_with("_ID") && raw_tbl_name.starts_with("S_") && !matches!(col_upper.as_str(), "ROW_ID" | "CONFLICT_ID" | "MODIFICATION_NUM" | "INTEGRATION_ID" | "ATTR_ID") {
                            let stem = &col_upper[..col_upper.len() - 3];
                            Some(format!("S_{}", stem))
                        } else {
                            None
                        };

                        if let Some(target_tbl) = &siebel_target_table {
                            col_entity.metadata.insert("references_table".to_string(), target_tbl.clone());
                        }

                        graph.add_entity(col_entity);
                        graph.add_edge(tbl_id, &col_id, EdgeKind::Contains);

                        if let Some(alias_id) = alias_id_opt {
                            let col_alias_id = format!("{}.{}", alias_id, col_name);
                            let mut col_alias = CodeEntity::new(&col_alias_id, &col_alias_id, EntityType::Column, rel_path, line_num, line_num);
                            col_alias.signature = Some(format!("{} {}", col_name, col_type));
                            col_alias.metadata.insert("is_alias_for".to_string(), col_id.clone());
                            if let Some(target_tbl) = &siebel_target_table {
                                col_alias.metadata.insert("references_table".to_string(), target_tbl.clone());
                            }
                            graph.add_entity(col_alias);
                            graph.add_edge(alias_id, &col_alias_id, EdgeKind::Contains);
                            graph.add_edge(tbl_id, &col_alias_id, EdgeKind::Contains);
                            graph.add_edge(alias_id, &col_id, EdgeKind::Contains);
                        }

                        if let Some(target_tbl) = &siebel_target_table {
                            link_table_reference(&col_id, target_tbl, rel_path, schema_opt.as_deref(), graph);
                            if let Some(alias_id) = alias_id_opt {
                                let col_alias_id = format!("{}.{}", alias_id, col_name);
                                link_table_reference(&col_alias_id, target_tbl, rel_path, schema_opt.as_deref(), graph);
                            }

                            link_table_reference(tbl_id, target_tbl, rel_path, schema_opt.as_deref(), graph);
                            if let Some(alias_id) = alias_id_opt {
                                link_table_reference(alias_id, target_tbl, rel_path, schema_opt.as_deref(), graph);
                            }
                        }
                    }
                }
            }
        } else if let Some(comm_caps) = comment_re.captures(line) {
            let col_schema = comm_caps.get(1).map(|m| clean(m.as_str()));
            let tbl_name = clean(comm_caps.get(2).unwrap().as_str());
            let col_name = clean(comm_caps.get(3).unwrap().as_str());
            let comment_text = comm_caps.get(4).unwrap().as_str().to_string();

            let col_id = format!("{}::{}.{}", rel_path, tbl_name, col_name);
            if let Some(idx) = graph.get_index(&col_id) {
                graph.graph[idx].docstring = Some(comment_text.clone());
            }
            if let Some(schema) = col_schema.as_ref() {
                let col_alias_id = format!("{}.{}.{}", schema, tbl_name, col_name);
                if let Some(idx) = graph.get_index(&col_alias_id) {
                    graph.graph[idx].docstring = Some(comment_text.clone());
                }
            }
        } else if let Some(caps) = index_re.captures(line) {
            let tbl_schema_opt = caps.get(3).map(|m| clean(m.as_str()));
            let tbl_name = clean(caps.get(4).unwrap().as_str());
            let cols_raw = caps.get(5).unwrap().as_str();

            let cols: Vec<String> = cols_raw
                .split(',')
                .map(|c| clean(c.trim().split_whitespace().next().unwrap_or("")).to_ascii_uppercase())
                .filter(|c| !c.is_empty())
                .collect();

            let mut target_tbl_indices = Vec::new();
            if let Some(schema) = &tbl_schema_opt {
                let qual = format!("{}.{}", schema, tbl_name);
                if let Some(idx) = graph.find_node_by_name_or_id(&qual) {
                    target_tbl_indices.push(idx);
                }
            }
            if let Some(idx) = graph.find_node_by_name_or_id(&tbl_name) {
                if !target_tbl_indices.contains(&idx) {
                    target_tbl_indices.push(idx);
                }
            }
            let rel_id = format!("{}::{}", rel_path, tbl_name);
            if let Some(idx) = graph.get_index(&rel_id) {
                if !target_tbl_indices.contains(&idx) {
                    target_tbl_indices.push(idx);
                }
            }

            for &t_idx in &target_tbl_indices {
                let mut current_cols: Vec<String> = graph.graph[t_idx]
                    .metadata
                    .get("indexed_columns")
                    .map(|s| s.split(',').map(|x| x.to_string()).collect())
                    .unwrap_or_default();
                for c in &cols {
                    if !current_cols.contains(c) {
                        current_cols.push(c.clone());
                    }
                }
                graph.graph[t_idx].metadata.insert("indexed_columns".to_string(), current_cols.join(","));
            }

            for c in &cols {
                let col_id1 = format!("{}::{}.{}", rel_path, tbl_name, c);
                if let Some(c_idx) = graph.get_index(&col_id1) {
                    graph.graph[c_idx].metadata.insert("is_indexed".to_string(), "true".to_string());
                }
                if let Some(schema) = &tbl_schema_opt {
                    let col_id2 = format!("{}.{}.{}", schema, tbl_name, c);
                    if let Some(c_idx) = graph.get_index(&col_id2) {
                        graph.graph[c_idx].metadata.insert("is_indexed".to_string(), "true".to_string());
                    }
                }
            }
        } else if let Some(caps) = member_proc_re.captures(line) {
            if let Some(pkg) = &current_package {
                let m_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_else(|| clean(caps.get(1).unwrap().as_str()));
                let full_name = format!("{}.{}", pkg, m_name);
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, &full_name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                let pkg_id = format!("{}::pkg::{}", rel_path, pkg);
                graph.add_edge(&pkg_id, &id, EdgeKind::Contains);
            }
        } else if let Some(caps) = member_func_re.captures(line) {
            if let Some(pkg) = &current_package {
                let m_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_else(|| clean(caps.get(1).unwrap().as_str()));
                let full_name = format!("{}.{}", pkg, m_name);
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, &full_name, EntityType::Method, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                let pkg_id = format!("{}::pkg::{}", rel_path, pkg);
                graph.add_edge(&pkg_id, &id, EdgeKind::Contains);
            }
        } else if end_re.is_match(trimmed) {
            current_package = None;
        }
    }
}
