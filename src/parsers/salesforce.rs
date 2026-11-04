use std::collections::HashMap;
use std::path::Path;
use regex::Regex;
use crate::graph::{CodeGraph, CodeEntity, EntityType, EdgeKind};

/// Helper to link an SObject and its queried fields to the active Apex class and canonical IDs.
fn link_sobject_reference(
    graph: &mut CodeGraph,
    rel_path: &str,
    line_num: usize,
    current_class: &Option<String>,
    current_method: &Option<String>,
    sobject: &str,
    fields_str: Option<&str>,
    is_dynamic: bool,
) {
    let sobject_id = format!("sobject::{}", sobject);
    if graph.get_entity(&sobject_id).is_none() {
        let mut sobj_entity = CodeEntity::new(&sobject_id, sobject, EntityType::Table, rel_path, line_num, line_num);
        if is_dynamic {
            sobj_entity.metadata.insert("isDynamicTarget".to_string(), "true".to_string());
        }
        graph.add_entity(sobj_entity);
    }

    if let Some(ref cls) = current_class {
        let class_id = format!("{}::{}", rel_path, cls);
        graph.add_edge(&class_id, &sobject_id, EdgeKind::References);
        let apex_id = format!("apex::{}", cls);
        graph.add_edge(&apex_id, &sobject_id, EdgeKind::References);

        if let Some(ref meth) = current_method {
            let method_id = format!("{}::{}.{}", rel_path, cls, meth);
            graph.add_edge(&method_id, &sobject_id, EdgeKind::References);
            let apex_meth_id = format!("apex::{}.{}", cls, meth);
            graph.add_edge(&apex_meth_id, &sobject_id, EdgeKind::References);
        }

        // Granular field-level SOQL extraction (Gap 2)
        if let Some(f_str) = fields_str {
            for raw_fld in f_str.split(',') {
                let fld = raw_fld.trim();
                if !fld.is_empty() && !fld.contains('(') && !fld.contains(' ') && !fld.contains(':') && !fld.contains('\'') {
                    let field_col_id = format!("sobject::{}.{}", sobject, fld);
                    if graph.get_entity(&field_col_id).is_none() {
                        let mut col_ent = CodeEntity::new(
                            &field_col_id,
                            &format!("{}.{}", sobject, fld),
                            EntityType::Column,
                            rel_path,
                            line_num,
                            line_num,
                        );
                        col_ent.signature = Some(format!("Column {}.{}", sobject, fld));
                        graph.add_entity(col_ent);
                        graph.add_edge(&sobject_id, &field_col_id, EdgeKind::Contains);
                    }
                    graph.add_edge(&class_id, &field_col_id, EdgeKind::References);
                    graph.add_edge(&apex_id, &field_col_id, EdgeKind::References);

                    if let Some(ref meth) = current_method {
                        let method_id = format!("{}::{}.{}", rel_path, cls, meth);
                        graph.add_edge(&method_id, &field_col_id, EdgeKind::References);
                        let apex_meth_id = format!("apex::{}.{}", cls, meth);
                        graph.add_edge(&apex_meth_id, &field_col_id, EdgeKind::References);
                    }
                }
            }
        }
    }
}

/// Parse Salesforce Apex classes into classes, interfaces, enums, annotated methods, and embedded SOQL references.
pub fn parse_apex_class(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let class_re = Regex::new(r"(?i)^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:public|global|private)\s+(?:(?:with|without|inherited)\s+sharing\s+)?(?:virtual|abstract)?\s*class\s+([a-zA-Z0-9_]+)").unwrap();
    let iface_re = Regex::new(r"(?i)^\s*(?:public|global)\s+interface\s+([a-zA-Z0-9_]+)").unwrap();
    let enum_re = Regex::new(r"(?i)^\s*(?:public|global|private)\s+enum\s+([a-zA-Z0-9_]+)").unwrap();
    let method_re = Regex::new(r"(?i)^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:(?:public|global|private|protected)\s+)?(?:static\s+)?[a-zA-Z0-9_<>\[\],\s]+\s+([a-zA-Z0-9_]+)\s*\([^;)]*\)\s*\{").unwrap();
    let soql_re = Regex::new(r"(?i)\[\s*SELECT\s+(.*?)\s+FROM\s+([a-zA-Z0-9_]+)").unwrap();

    // Dynamic SOQL & Schema Describe Detection (Gap 3: Dynamic SOQL Blind Spots)
    let dyn_soql_re = Regex::new(r#"(?i)['"]\s*SELECT\s+(.*?)\s+FROM\s+([a-zA-Z0-9_]+)"#).unwrap();
    let select_prefix_re = Regex::new(r#"(?i)['"]\s*SELECT\s+([^'"]+)['"]"#).unwrap();
    let dyn_from_concat_re = Regex::new(r#"(?i)\bFROM\s*['"]\s*\+\s*['"]([a-zA-Z0-9_]+)['"]"#).unwrap();
    let dyn_from_spaced_re = Regex::new(r#"(?i)['"]\s*FROM\s+([a-zA-Z0-9_]+)\s*['"]"#).unwrap();
    let dyn_from_var_re = Regex::new(r#"(?i)\bFROM\s*['"]\s*\+\s*([a-zA-Z0-9_]+)\b"#).unwrap();
    let full_str_assign_re = Regex::new(r#"(?i)(?:String\s+)?([a-zA-Z0-9_]+)\s*=\s*['"]([^'"]+)['"]"#).unwrap();
    let schema_obj_re = Regex::new(r#"(?i)\bSchema\.SObjectType\.([a-zA-Z0-9_]+)\b"#).unwrap();
    let sobj_type_re = Regex::new(r#"(?i)\b([a-zA-Z0-9_]+)\.[sS]ObjectType\b"#).unwrap();
    let db_query_re = Regex::new(r#"(?i)Database\.(?:query|getQueryLocator)\s*\(\s*([a-zA-Z0-9_]+)\s*\)"#).unwrap();
    let bre_matrix_re = Regex::new(r#"(?i)createCustomAction\s*\(\s*['"]runDecisionMatrix['"]\s*,\s*['"]([^'"]+)['"]"#).unwrap();

    let is_test_class = content.contains("@isTest")
        || content.contains("@IsTest")
        || content.contains("testMethod")
        || rel_path.to_lowercase().contains("/test/")
        || rel_path.ends_with("Test.cls");

    let mut current_class: Option<String> = None;
    let mut current_method: Option<String> = None;
    let mut local_vars: HashMap<String, String> = HashMap::new();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = class_re.captures(trimmed) {
            let name = caps.get(1).unwrap().as_str();
            current_class = Some(name.to_string());
            current_method = None;
            let id = format!("{}::{}", rel_path, name);
            let mut entity = CodeEntity::new(&id, name, EntityType::Class, rel_path, line_num, line_num);
            if is_test_class || name.ends_with("Test") {
                entity.metadata.insert("is_test".to_string(), "true".to_string());
            }
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);

            let apex_id = format!("apex::{}", name);
            if graph.get_entity(&apex_id).is_none() {
                let mut alias_entity = CodeEntity::new(&apex_id, name, EntityType::Class, rel_path, line_num, line_num);
                if is_test_class || name.ends_with("Test") {
                    alias_entity.metadata.insert("is_test".to_string(), "true".to_string());
                }
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
                current_method = Some(name.to_string());
                let full_name = if let Some(ref cls) = current_class {
                    format!("{}.{}", cls, name)
                } else {
                    name.to_string()
                };
                let id = format!("{}::{}", rel_path, full_name);
                let mut entity = CodeEntity::new(&id, &full_name, EntityType::Method, rel_path, line_num, line_num);
                if is_test_class || trimmed.contains("@isTest") || trimmed.contains("@IsTest") || trimmed.contains("testMethod") {
                    entity.metadata.insert("is_test".to_string(), "true".to_string());
                }
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);

                if let Some(ref cls) = current_class {
                    let class_id = format!("{}::{}", rel_path, cls);
                    graph.add_edge(&class_id, &id, EdgeKind::Contains);

                    let apex_method_id = format!("apex::{}.{}", cls, name);
                    if graph.get_entity(&apex_method_id).is_none() {
                        let mut alias_meth = CodeEntity::new(&apex_method_id, &format!("{}.{}", cls, name), EntityType::Method, rel_path, line_num, line_num);
                        if is_test_class {
                            alias_meth.metadata.insert("is_test".to_string(), "true".to_string());
                        }
                        alias_meth.signature = Some(trimmed.to_string());
                        graph.add_entity(alias_meth);
                    }
                    graph.add_edge(&apex_method_id, &id, EdgeKind::References);
                    let apex_cls_id = format!("apex::{}", cls);
                    graph.add_edge(&apex_cls_id, &apex_method_id, EdgeKind::Contains);
                }
            }
        }

        // Bre Decision Matrix invocation in Apex (Wish 1)
        if let Some(caps) = bre_matrix_re.captures(trimmed) {
            let matrix_name = caps.get(1).unwrap().as_str();
            let matrix_id = format!("decision_matrix::{}", matrix_name);
            if graph.get_entity(&matrix_id).is_none() {
                let mut matrix_ent = CodeEntity::new(&matrix_id, matrix_name, EntityType::Table, rel_path, line_num, line_num);
                matrix_ent.signature = Some(format!("DecisionMatrix {}", matrix_name));
                graph.add_entity(matrix_ent);
            }
            if let Some(ref cls) = current_class {
                let class_id = format!("{}::{}", rel_path, cls);
                graph.add_edge(&class_id, &matrix_id, EdgeKind::Calls);
                let apex_id = format!("apex::{}", cls);
                graph.add_edge(&apex_id, &matrix_id, EdgeKind::Calls);
            }
        }

        // Track local string variable assignments for constant propagation (Tier 2)
        if let Some(caps) = full_str_assign_re.captures(trimmed) {
            let var_name = caps.get(1).unwrap().as_str();
            let val = caps.get(2).unwrap().as_str();
            local_vars.insert(var_name.to_string(), val.to_string());
        }

        // 1. Static inline SOQL query: [SELECT ... FROM SObject]
        if let Some(caps) = soql_re.captures(trimmed) {
            let fields_str = caps.get(1).unwrap().as_str();
            let sobject = caps.get(2).unwrap().as_str();
            link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, sobject, Some(fields_str), false);
        }

        // 2. Dynamic SOQL string literal: 'SELECT ... FROM SObject' (Tier 1)
        if let Some(caps) = dyn_soql_re.captures(trimmed) {
            let fields_str = caps.get(1).unwrap().as_str();
            let sobject = caps.get(2).unwrap().as_str();
            link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, sobject, Some(fields_str), true);
        }

        // 3. Dynamic SOQL concatenation: '... FROM ' + 'AccountContactRelation' (Tier 1)
        if let Some(caps) = dyn_from_concat_re.captures(trimmed) {
            let sobject = caps.get(1).unwrap().as_str();
            let fields_str = select_prefix_re.captures(trimmed).map(|c| c.get(1).unwrap().as_str());
            link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, sobject, fields_str, true);
        }
        if let Some(caps) = dyn_from_spaced_re.captures(trimmed) {
            let sobject = caps.get(1).unwrap().as_str();
            let fields_str = select_prefix_re.captures(trimmed).map(|c| c.get(1).unwrap().as_str());
            link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, sobject, fields_str, true);
        }

        // 4. Dynamic SOQL variable reference: '... FROM ' + sobjectVar (Tier 2 Constant Folding)
        if let Some(caps) = dyn_from_var_re.captures(trimmed) {
            let var_name = caps.get(1).unwrap().as_str();
            if let Some(val) = local_vars.get(var_name) {
                let fields_str = select_prefix_re.captures(trimmed).map(|c| c.get(1).unwrap().as_str());
                link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, val, fields_str, true);
            }
        }

        // 5. Database.query(queryVar) check with local variable
        if let Some(caps) = db_query_re.captures(trimmed) {
            let query_var = caps.get(1).unwrap().as_str();
            if let Some(val) = local_vars.get(query_var) {
                if let Some(caps) = dyn_soql_re.captures(val) {
                    let fields_str = caps.get(1).unwrap().as_str();
                    let sobject = caps.get(2).unwrap().as_str();
                    link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, sobject, Some(fields_str), true);
                }
            }
        }

        // 6. Schema Token describe references: Schema.SObjectType.<SObject> (Tier 3)
        for caps in schema_obj_re.captures_iter(trimmed) {
            let sobject = caps.get(1).unwrap().as_str();
            link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, sobject, None, false);
        }

        // 7. SObject.SObjectType references: <SObject>.SObjectType (Tier 3)
        for caps in sobj_type_re.captures_iter(trimmed) {
            let tok = caps.get(1).unwrap().as_str();
            let is_keyword = matches!(
                tok,
                "List" | "Map" | "Set" | "String" | "Boolean" | "Integer" | "Decimal" | "Double" | "Long" | "Date" | "Datetime" | "Time" | "Object" | "Id" | "Blob" | "Schema" | "System" | "Database" | "Test" | "JSON" | "Type" | "Exception" | "SObject" | "ApexPages" | "Http" | "HttpRequest" | "HttpResponse"
            );
            if !is_keyword {
                link_sobject_reference(graph, rel_path, line_num, &current_class, &current_method, tok, None, false);
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
        let ref_to_re = Regex::new(r"<referenceTo>([^<]+)</referenceTo>").unwrap();
        let ref_target = ref_to_re.captures(content).map(|c| c.get(1).unwrap().as_str().to_string());

        let field_id = format!("{}::field::{}", rel_path, field_name);
        let mut entity = CodeEntity::new(&field_id, &field_name, EntityType::Column, rel_path, 1, 1);
        entity.signature = Some(format!("CustomField {} ({})", field_name, field_type));
        let is_external_id = content.contains("<externalId>true</externalId>");
        if is_external_id {
            entity.metadata.insert("isExternalId".to_string(), "true".to_string());
        }
        if let Some(ref target) = ref_target {
            entity.metadata.insert("referenceTo".to_string(), target.clone());
        }
        graph.add_entity(entity);

        if let Some(ref target) = ref_target {
            let target_sobject_id = format!("sobject::{}", target);
            if graph.get_entity(&target_sobject_id).is_none() {
                let target_ent = CodeEntity::new(&target_sobject_id, target.as_str(), EntityType::Table, rel_path, 1, 1);
                graph.add_entity(target_ent);
            }
            graph.add_edge(&field_id, &target_sobject_id, EdgeKind::DependsOn);
        }

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

                // Canonical sobject field entity (sobject::Parent.Field)
                let sobj_field_id = format!("{}.{}", sobject_id, field_name);
                if graph.get_entity(&sobj_field_id).is_none() {
                    let mut sobj_field_ent = CodeEntity::new(
                        &sobj_field_id,
                        &format!("{}.{}", parent_sobject, field_name),
                        EntityType::Column,
                        rel_path,
                        1,
                        1,
                    );
                    sobj_field_ent.signature = Some(format!("CustomField {}.{} ({})", parent_sobject, field_name, field_type));
                    if is_external_id {
                        sobj_field_ent.metadata.insert("isExternalId".to_string(), "true".to_string());
                    }
                    graph.add_entity(sobj_field_ent);
                    graph.add_edge(&sobject_id, &sobj_field_id, EdgeKind::Contains);
                    if let Some(ref target) = ref_target {
                        let target_sobject_id = format!("sobject::{}", target);
                        graph.add_edge(&sobj_field_id, &target_sobject_id, EdgeKind::DependsOn);
                    }
                }

                // Layer A: Automated External ID Semantic Bridge candidate mining
                if is_external_id {
                    let upper_field = field_name.to_ascii_uppercase();
                    let legacy_prefixes = ["CATALYST", "CATALIST", "SIEBEL", "ORACLE", "LEGACY", "SAP", "BILLING"];
                    if legacy_prefixes.iter().any(|prefix| upper_field.contains(prefix)) {
                        let parent_upper = parent_sobject.to_ascii_uppercase();
                        let inferred_source = match parent_upper.as_str() {
                            "ACCOUNT" => Some("SIEBEL.S_ORG_EXT"),
                            "PRODUCT2" => Some("SIEBEL.S_PROD_INT"),
                            "ASSET" => Some("SIEBEL.S_ASSET"),
                            "ORDER" => Some("SIEBEL.S_ORDER"),
                            "QUOTE" => Some("SIEBEL.S_DOC_QUOTE"),
                            "CASE" => Some("SIEBEL.S_SRV_REQ"),
                            "CONTACT" => Some("SIEBEL.S_CONTACT"),
                            _ => None,
                        };

                        if let Some(source_table) = inferred_source {
                            if graph.get_entity(source_table).is_none() {
                                let mut source_ent = CodeEntity::new(source_table, source_table, EntityType::Table, rel_path, 1, 1);
                                source_ent.signature = Some(format!("External Source Table {}", source_table));
                                graph.add_entity(source_ent);
                            }
                            graph.add_edge(&sobject_id, source_table, EdgeKind::SemanticBridge);
                            graph.add_edge(source_table, &sobject_id, EdgeKind::SemanticBridge);
                            graph.add_edge(&sobj_field_id, source_table, EdgeKind::SemanticBridge);
                        }
                    }
                }

                // If this is a foreign key, connect parent SObject directly to target SObject
                if let Some(ref target) = ref_target {
                    let target_sobject_id = format!("sobject::{}", target);
                    graph.add_edge(&sobject_id, &target_sobject_id, EdgeKind::References);
                }
            }
        }
    } else if file_name.ends_with(".md-meta.xml") {
        // Custom Metadata Type (CMDT) record instance
        let base_name = file_name.strip_suffix(".md-meta.xml").unwrap_or(file_name);
        let (cmdt_type, record_name) = if let Some((t, r)) = base_name.split_once('.') {
            (t, r)
        } else {
            (base_name, base_name)
        };

        let label_re = Regex::new(r"<label>([^<]+)</label>").unwrap();
        let label = label_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or(record_name);

        let cmdt_id = format!("cmdt::{}", base_name);
        let mut entity = CodeEntity::new(&cmdt_id, base_name, EntityType::Variable, rel_path, 1, 1);
        entity.signature = Some(format!("CustomMetadata {} ({})", record_name, label));
        entity.metadata.insert("cmdt_type".to_string(), cmdt_type.to_string());
        entity.metadata.insert("record_name".to_string(), record_name.to_string());
        entity.metadata.insert("label".to_string(), label.to_string());

        let field_val_re = Regex::new(r"(?s)<values>\s*<field>([^<]+)</field>\s*<value[^>]*>([^<]*)</value>\s*</values>").unwrap();
        let mut doc = format!("Label: {}\n", label);
        for caps in field_val_re.captures_iter(content) {
            let f = caps.get(1).unwrap().as_str();
            let v = caps.get(2).unwrap().as_str();
            doc.push_str(&format!("{}: {}\n", f, v));
            entity.metadata.insert(f.to_string(), v.to_string());
        }
        entity.docstring = Some(doc);
        graph.add_entity(entity);

        let sobject_id = format!("sobject::{}__mdt", cmdt_type);
        if graph.get_entity(&sobject_id).is_none() {
            let parent_entity = CodeEntity::new(&sobject_id, &format!("{}__mdt", cmdt_type), EntityType::Table, rel_path, 1, 1);
            graph.add_entity(parent_entity);
        }
        graph.add_edge(&sobject_id, &cmdt_id, EdgeKind::Contains);
    } else {
        // Generic Salesforce DX Metadata (*-meta.xml)
        let base_name = if let Some(stripped) = file_name.strip_suffix("-meta.xml") {
            stripped
        } else {
            file_name
        };

        let (meta_kind, inferred_name) = if let Some((name_part, kind_part)) = base_name.rsplit_once('.') {
            (kind_part, name_part)
        } else {
            ("metadata", base_name)
        };

        let root_tag_re = Regex::new(r"<([a-zA-Z0-9_]+)(?:\s+[^>]*)?>").unwrap();
        let root_tag = content
            .lines()
            .find(|line| {
                let t = line.trim();
                !t.starts_with("<?xml") && !t.starts_with("<!--") && t.starts_with('<')
            })
            .and_then(|line| root_tag_re.captures(line))
            .map(|c| c.get(1).unwrap().as_str().to_string())
            .unwrap_or_else(|| meta_kind.to_string());

        let label_re = Regex::new(r"<label>([^<]+)</label>").unwrap();
        let fullname_re = Regex::new(r"<fullName>([^<]+)</fullName>").unwrap();
        let name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
        let desc_re = Regex::new(r"<description>([^<]+)</description>").unwrap();

        let display_name = fullname_re
            .captures(content)
            .or_else(|| label_re.captures(content))
            .or_else(|| name_re.captures(content))
            .map(|c| c.get(1).unwrap().as_str())
            .unwrap_or(inferred_name);

        let entity_id = format!("salesforce::{}::{}", meta_kind.to_ascii_lowercase(), inferred_name);
        let entity_type = match meta_kind.to_ascii_lowercase().as_str() {
            "flow" | "workflow" | "approvalprocess" => EntityType::Rule,
            "profile" | "permissionset" | "custompermission" => EntityType::Rule,
            "layout" | "flexipage" => EntityType::Interface,
            "labels" | "customlabels" | "globalvalueset" | "standardvalueset" => EntityType::Variable,
            _ => EntityType::Module,
        };

        let mut entity = CodeEntity::new(&entity_id, display_name, entity_type, rel_path, 1, 1);
        entity.signature = Some(format!("Salesforce {} {}", root_tag, display_name));
        if let Some(caps) = desc_re.captures(content) {
            entity.docstring = Some(caps.get(1).unwrap().as_str().to_string());
        }
        entity.metadata.insert("metadata_type".to_string(), root_tag.clone());
        graph.add_entity(entity);

        // Cross-linking: check if target SObject is referenced (<object>Account</object> or <targetObject>)
        let object_ref_re = Regex::new(r"<(?:object|targetObject|targetEntity)>([^<]+)</(?:object|targetObject|targetEntity)>").unwrap();
        if let Some(caps) = object_ref_re.captures(content) {
            let target_sobj = caps.get(1).unwrap().as_str();
            let sobj_id = format!("sobject::{}", target_sobj);
            if graph.get_entity(&sobj_id).is_none() {
                let sobj_ent = CodeEntity::new(&sobj_id, target_sobj, EntityType::Table, rel_path, 1, 1);
                graph.add_entity(sobj_ent);
            }
            graph.add_edge(&entity_id, &sobj_id, EdgeKind::References);
        }

        // Sub-elements for CustomLabels: <labels><fullName>X</fullName><value>Y</value></labels>
        if meta_kind.eq_ignore_ascii_case("labels") || root_tag == "CustomLabels" {
            let label_block_re = Regex::new(r"(?s)<labels>(.*?)</labels>").unwrap();
            let sub_fn_re = Regex::new(r"<fullName>([^<]+)</fullName>").unwrap();
            let sub_val_re = Regex::new(r"<value>([^<]*)</value>").unwrap();

            for (idx, cap) in label_block_re.captures_iter(content).enumerate() {
                let block = cap.get(1).unwrap().as_str();
                if let Some(fn_cap) = sub_fn_re.captures(block) {
                    let label_name = fn_cap.get(1).unwrap().as_str();
                    let val = sub_val_re.captures(block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");
                    let label_id = format!("label::{}", label_name);
                    let mut label_ent = CodeEntity::new(&label_id, label_name, EntityType::Variable, rel_path, idx + 1, idx + 1);
                    label_ent.signature = Some(format!("CustomLabel {}: {}", label_name, val));
                    graph.add_entity(label_ent);
                    graph.add_edge(&entity_id, &label_id, EdgeKind::Contains);
                }
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
    let component_re = Regex::new(r#"(?i)<(?:aura:component|apex:page|apex:component)(?:\s+[^>]*?)?\s+controller=["']([^"']+)["']"#).unwrap();
    let extensions_re = Regex::new(r#"(?i)<apex:page(?:\s+[^>]*?)?\s+extensions=["']([^"']+)["']"#).unwrap();
    let attr_re = Regex::new(r#"(?i)<aura:attribute(?:\s+[^>]*?)?\s+name=["']([^"']+)["'](?:\s+[^>]*?)?(?:\s+type=["']([^"']+)["'])?"#).unwrap();
    let path = Path::new(rel_path);
    let comp_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("auraComponent");

    let comp_id = format!("{}::component::{}", rel_path, comp_name);
    let mut entity = CodeEntity::new(&comp_id, comp_name, EntityType::Tag, rel_path, 1, 1);
    entity.signature = Some(format!("<component name=\"{}\">", comp_name));
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

    if let Some(caps) = extensions_re.captures(content) {
        let extensions_str = caps.get(1).unwrap().as_str();
        for ext in extensions_str.split(',') {
            let ext = ext.trim();
            if !ext.is_empty() {
                let ext_id = format!("apex::{}", ext);
                if graph.get_entity(&ext_id).is_none() {
                    let ext_entity = CodeEntity::new(&ext_id, ext, EntityType::Class, rel_path, 1, 1);
                    graph.add_entity(ext_entity);
                }
                graph.add_edge(&comp_id, &ext_id, EdgeKind::References);
            }
        }
    }

    for (idx, line) in content.lines().enumerate() {
        if let Some(caps) = attr_re.captures(line) {
            let attr_name = caps.get(1).unwrap().as_str();
            let attr_type = caps.get(2).map(|m| m.as_str()).unwrap_or("String");
            let attr_id = format!("{}::attr::{}", comp_id, attr_name);
            let mut attr_ent = CodeEntity::new(&attr_id, attr_name, EntityType::Variable, rel_path, idx + 1, idx + 1);
            attr_ent.signature = Some(format!("<aura:attribute name=\"{}\" type=\"{}\"/>", attr_name, attr_type));
            graph.add_entity(attr_ent);
            graph.add_edge(&comp_id, &attr_id, EdgeKind::Contains);
        }
    }
}

/// Parse Omnistudio metadata (Integration Procedures, OmniScripts, UI Cards) into process nodes and action edges.
pub fn parse_omnistudio_metadata(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let path = Path::new(rel_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Special handler for OmniDataTransform / DataRaptor XML (*.rpt-meta.xml)
    if file_name.ends_with(".rpt-meta.xml") || content.contains("<OmniDataTransform") {
        let name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
        let name = name_re.captures(content).map(|c| c.get(1).unwrap().as_str().to_string()).unwrap_or_else(|| {
            file_name.split('.').next().unwrap_or(file_name).to_string()
        });

        let dr_id = format!("omni_dr::{}", name);
        let mut dr_entity = CodeEntity::new(&dr_id, &name, EntityType::Module, rel_path, 1, 1);
        dr_entity.signature = Some(format!("OmniDataTransform {}", name));
        dr_entity.metadata.insert("kind".to_string(), "DataTransform".to_string());
        graph.add_entity(dr_entity);

        let bundle_id = format!("bundle::{}", name);
        if graph.get_entity(&bundle_id).is_none() {
            let mut b_ent = CodeEntity::new(&bundle_id, &name, EntityType::Module, rel_path, 1, 1);
            b_ent.signature = Some(format!("DataRaptorBundle {}", name));
            graph.add_entity(b_ent);
        }
        graph.add_edge(&dr_id, &bundle_id, EdgeKind::Contains);

        let item_block_re = Regex::new(r"(?s)<omniDataTransformItem>(.*?)</omniDataTransformItem>").unwrap();
        let output_obj_re = Regex::new(r"<outputObjectName>([^<]+)</outputObjectName>").unwrap();
        let output_fld_re = Regex::new(r"<outputFieldName>([^<]+)</outputFieldName>").unwrap();
        let formula_expr_re = Regex::new(r"(?s)<formulaExpression>(.*?)</formulaExpression>").unwrap();
        let formula_res_re = Regex::new(r"<formulaResultPath>([^<]+)</formulaResultPath>").unwrap();
        let field_token_re = Regex::new(r"([A-Za-z0-9_]+)[:\.]([A-Za-z0-9_]+)").unwrap();

        for (item_idx, item_cap) in item_block_re.captures_iter(content).enumerate() {
            let block = item_cap.get(1).unwrap().as_str();
            let line_num = item_idx + 1;

            if let Some(out_obj_cap) = output_obj_re.captures(block) {
                let out_obj = out_obj_cap.get(1).unwrap().as_str();
                if out_obj != "Formula" && out_obj != "json" {
                    let sobj_id = format!("sobject::{}", out_obj);
                    if graph.get_entity(&sobj_id).is_none() {
                        let mut sobj_ent = CodeEntity::new(&sobj_id, out_obj, EntityType::Table, rel_path, line_num, line_num);
                        sobj_ent.signature = Some(format!("CustomObject {}", out_obj));
                        graph.add_entity(sobj_ent);
                    }
                    graph.add_edge(&dr_id, &sobj_id, EdgeKind::References);

                    if let Some(out_fld_cap) = output_fld_re.captures(block) {
                        let out_fld = out_fld_cap.get(1).unwrap().as_str();
                        let fld_id = format!("sobject::{}.{}", out_obj, out_fld);
                        if graph.get_entity(&fld_id).is_none() {
                            let mut col_ent = CodeEntity::new(&fld_id, out_fld, EntityType::Column, rel_path, line_num, line_num);
                            col_ent.signature = Some(format!("Field {}.{}", out_obj, out_fld));
                            graph.add_entity(col_ent);
                            graph.add_edge(&sobj_id, &fld_id, EdgeKind::Contains);
                        }
                        graph.add_edge(&dr_id, &fld_id, EdgeKind::References);
                    }
                }
            }

            // Formula Lineage in XML OmniDataTransform
            if let (Some(f_expr_cap), Some(f_res_cap)) = (formula_expr_re.captures(block), formula_res_re.captures(block)) {
                let f_expr = f_expr_cap.get(1).unwrap().as_str().trim();
                let f_res = f_res_cap.get(1).unwrap().as_str().trim();

                let formula_id = format!("{}::formula::{}", dr_id, f_res);
                let mut formula_ent = CodeEntity::new(&formula_id, f_res, EntityType::Function, rel_path, line_num, line_num);
                formula_ent.signature = Some(format!("Formula {} = {}", f_res, f_expr));
                formula_ent.metadata.insert("formulaExpression".to_string(), f_expr.to_string());
                formula_ent.metadata.insert("resultPath".to_string(), f_res.to_string());
                graph.add_entity(formula_ent);
                graph.add_edge(&dr_id, &formula_id, EdgeKind::Contains);

                // Tokenize referenced fields in formula
                for cap in field_token_re.captures_iter(f_expr) {
                    let obj = cap.get(1).unwrap().as_str();
                    let fld = cap.get(2).unwrap().as_str();
                    if !obj.chars().all(|c| c.is_ascii_digit()) && !fld.chars().all(|c| c.is_ascii_digit()) {
                        let sobj_id = format!("sobject::{}", obj);
                        let fld_id = format!("sobject::{}.{}", obj, fld);
                        if graph.get_entity(&sobj_id).is_none() {
                            let mut s_ent = CodeEntity::new(&sobj_id, obj, EntityType::Table, rel_path, line_num, line_num);
                            s_ent.signature = Some(format!("CustomObject {}", obj));
                            graph.add_entity(s_ent);
                        }
                        if graph.get_entity(&fld_id).is_none() {
                            let mut c_ent = CodeEntity::new(&fld_id, fld, EntityType::Column, rel_path, line_num, line_num);
                            c_ent.signature = Some(format!("Field {}.{}", obj, fld));
                            graph.add_entity(c_ent);
                            graph.add_edge(&sobj_id, &fld_id, EdgeKind::Contains);
                        }
                        graph.add_edge(&fld_id, &formula_id, EdgeKind::References);
                    }
                }
            }
        }
        return;
    }

    let (entity_kind, prefix) = if file_name.ends_with(".oip-meta.xml") {
        ("OmniIntegrationProcedure", "omni_ip")
    } else if file_name.ends_with(".os-meta.xml") {
        ("OmniScript", "omni_script")
    } else if file_name.ends_with(".ouc-meta.xml") {
        ("OmniCard", "omni_card")
    } else {
        ("OmniProcess", "omni")
    };

    let name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
    let desc_re = Regex::new(r"<description>([^<]+)</description>").unwrap();

    let name = name_re.captures(content).map(|c| c.get(1).unwrap().as_str().to_string()).unwrap_or_else(|| {
        file_name.split('.').next().unwrap_or(file_name).to_string()
    });

    let description = desc_re.captures(content).map(|c| c.get(1).unwrap().as_str().to_string());
    let proc_id = format!("{}::{}", prefix, name);
    let mut proc_entity = CodeEntity::new(&proc_id, &name, EntityType::Module, rel_path, 1, 1);
    proc_entity.signature = Some(format!("{} {}", entity_kind, name));
    proc_entity.docstring = description;
    proc_entity.metadata.insert("kind".to_string(), entity_kind.to_string());
    graph.add_entity(proc_entity);

    // Parse embedded elements: <omniProcessElements>...</omniProcessElements>
    let element_block_re = Regex::new(r"(?s)<omniProcessElements>(.*?)</omniProcessElements>").unwrap();
    let elem_name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
    let elem_level_re = Regex::new(r"<level>([^<]+)</level>").unwrap();

    // Property regexes (matching unescaped or escaped JSON quotes)
    let remote_class_re = Regex::new(r#"(?:&quot;|")remoteClass(?:&quot;|")\s*:\s*(?:&quot;|")([^"&]+)(?:&quot;|")"#).unwrap();
    let remote_method_re = Regex::new(r#"(?:&quot;|")remoteMethod(?:&quot;|")\s*:\s*(?:&quot;|")([^"&]+)(?:&quot;|")"#).unwrap();
    let bundle_re = Regex::new(r#"(?:&quot;|")bundleName(?:&quot;|")\s*:\s*(?:&quot;|")([^"&]+)(?:&quot;|")"#).unwrap();
    let sub_ip_re = Regex::new(r#"(?:&quot;|")integrationProcedureKey(?:&quot;|")\s*:\s*(?:&quot;|")([^"&]+)(?:&quot;|")"#).unwrap();

    for (el_idx, block_cap) in element_block_re.captures_iter(content).enumerate() {
        let block = block_cap.get(1).unwrap().as_str();
        let el_name = elem_name_re.captures(block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("step");
        let el_level = elem_level_re.captures(block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("0.0");

        let step_id = format!("{}::element::{}", proc_id, el_name);
        let mut step_entity = CodeEntity::new(&step_id, el_name, EntityType::Function, rel_path, el_idx + 1, el_idx + 1);
        step_entity.signature = Some(format!("Step {} (level {})", el_name, el_level));
        step_entity.metadata.insert("level".to_string(), el_level.to_string());

        let mut remote_class: Option<String> = None;
        let mut remote_method: Option<String> = None;
        let mut bundle_name: Option<String> = None;
        let mut sub_key: Option<String> = None;

        // Remote Apex Action
        if let Some(rc_cap) = remote_class_re.captures(block) {
            let cls_name = rc_cap.get(1).unwrap().as_str();
            step_entity.metadata.insert("remoteClass".to_string(), cls_name.to_string());
            if let Some(rm_cap) = remote_method_re.captures(block) {
                let m_name = rm_cap.get(1).unwrap().as_str();
                step_entity.metadata.insert("remoteMethod".to_string(), m_name.to_string());
                remote_method = Some(m_name.to_string());
            }
            remote_class = Some(cls_name.to_string());
        }

        // DataRaptor dependency
        if let Some(dr_cap) = bundle_re.captures(block) {
            let dr_name = dr_cap.get(1).unwrap().as_str();
            step_entity.metadata.insert("bundleName".to_string(), dr_name.to_string());
            bundle_name = Some(dr_name.to_string());
        }

        // Sub-IP invocation
        if let Some(sub_cap) = sub_ip_re.captures(block) {
            let key = sub_cap.get(1).unwrap().as_str();
            step_entity.metadata.insert("subProcedure".to_string(), key.to_string());
            sub_key = Some(key.to_string());
        }

        // Add step entity first so directed edges can connect from it
        graph.add_entity(step_entity);
        graph.add_edge(&proc_id, &step_id, EdgeKind::Contains);

        if let Some(cls_name) = remote_class.as_ref() {
            let target_id = if let Some(cls_idx) = graph.find_node_by_name_or_id(cls_name) {
                graph.graph[cls_idx].id.clone()
            } else {
                let apex_id = format!("apex::{}", cls_name);
                if graph.get_entity(&apex_id).is_none() {
                    let target_ent = CodeEntity::new(&apex_id, cls_name, EntityType::Class, rel_path, 1, 1);
                    graph.add_entity(target_ent);
                }
                apex_id
            };
            graph.add_edge(&step_id, &target_id, EdgeKind::Calls);
        }

        if let (Some(cls_name), Some(meth_name)) = (&remote_class, &remote_method) {
            let meth_id = format!("apex::{}.{}", cls_name, meth_name);
            if graph.get_entity(&meth_id).is_none() {
                let mut m_ent = CodeEntity::new(&meth_id, meth_name, EntityType::Method, rel_path, 1, 1);
                m_ent.signature = Some(format!("ApexMethod {}.{}", cls_name, meth_name));
                graph.add_entity(m_ent);
            }
            graph.add_edge(&step_id, &meth_id, EdgeKind::Calls);
        }

        if let Some(dr_name) = bundle_name {
            let dr_id = format!("bundle::{}", dr_name);
            if graph.get_entity(&dr_id).is_none() {
                let dr_ent = CodeEntity::new(&dr_id, &dr_name, EntityType::Rule, rel_path, 1, 1);
                graph.add_entity(dr_ent);
            }
            graph.add_edge(&step_id, &dr_id, EdgeKind::DependsOn);

            let alias_id = format!("dataraptor::{}", dr_name);
            if graph.get_entity(&alias_id).is_none() {
                let alias_ent = CodeEntity::new(&alias_id, &dr_name, EntityType::Rule, rel_path, 1, 1);
                graph.add_entity(alias_ent);
            }
            graph.add_edge(&step_id, &alias_id, EdgeKind::DependsOn);

            let omni_dr_id = format!("omni_dr::{}", dr_name);
            if graph.get_entity(&omni_dr_id).is_none() {
                let omni_dr_ent = CodeEntity::new(&omni_dr_id, &dr_name, EntityType::Rule, rel_path, 1, 1);
                graph.add_entity(omni_dr_ent);
            }
            graph.add_edge(&step_id, &omni_dr_id, EdgeKind::DependsOn);
        }

        if let Some(key) = sub_key {
            let sub_id = format!("omni_ip::{}", key);
            if graph.get_entity(&sub_id).is_none() {
                let sub_ent = CodeEntity::new(&sub_id, &key, EntityType::Module, rel_path, 1, 1);
                graph.add_entity(sub_ent);
            }
            graph.add_edge(&step_id, &sub_id, EdgeKind::Calls);
        }
    }
}

/// Parse Salesforce Approval Process XML (.approvalProcess-meta.xml)
pub fn parse_approval_process(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let path = Path::new(rel_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let fallback_name = file_name.strip_suffix(".approvalProcess-meta.xml").unwrap_or(file_name);

    let label_re = Regex::new(r"<label>([^<]+)</label>").unwrap();
    let target_obj_re = Regex::new(r"<targetObject>([^<]+)</targetObject>").unwrap();
    let step_re = Regex::new(r"(?s)<approvalStep>(.*?)</approvalStep>").unwrap();
    let step_name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
    let step_label_re = Regex::new(r"<label>([^<]+)</label>").unwrap();
    let approver_re = Regex::new(r"<approver>\s*<name>([^<]+)</name>").unwrap();
    let formula_re = Regex::new(r"<formula>([^<]+)</formula>").unwrap();

    let name = label_re.captures(content).map(|c| c.get(1).unwrap().as_str().to_string()).unwrap_or_else(|| fallback_name.to_string());
    let proc_id = format!("approval_process::{}", fallback_name);

    let mut proc_entity = CodeEntity::new(&proc_id, &name, EntityType::Rule, rel_path, 1, 1);
    proc_entity.signature = Some(format!("ApprovalProcess {}", name));

    let mut target_object_name: Option<String> = None;
    if let Some(caps) = target_obj_re.captures(content) {
        let sobj = caps.get(1).unwrap().as_str();
        proc_entity.metadata.insert("targetObject".to_string(), sobj.to_string());
        target_object_name = Some(sobj.to_string());
    }
    graph.add_entity(proc_entity);

    if let Some(ref sobj) = target_object_name {
        let sobj_id = format!("sobject::{}", sobj);
        if graph.get_entity(&sobj_id).is_none() {
            let sobj_ent = CodeEntity::new(&sobj_id, sobj, EntityType::Table, rel_path, 1, 1);
            graph.add_entity(sobj_ent);
        }
        graph.add_edge(&proc_id, &sobj_id, EdgeKind::References);
    }

    // Extract fields referenced in entry criteria formula
    if let Some(caps) = formula_re.captures(content) {
        let formula = caps.get(1).unwrap().as_str();
        let field_ref_re = Regex::new(r"\b([a-zA-Z0-9_]+__c|[a-zA-Z0-9_]+)\b").unwrap();
        if let Some(ref sobj) = target_object_name {
            for fcap in field_ref_re.captures_iter(formula) {
                let fld = fcap.get(1).unwrap().as_str();
                if fld.ends_with("__c") || matches!(fld, "Status" | "Type" | "TotalAmount" | "Discount") {
                    let field_id = format!("sobject::{}.{}", sobj, fld);
                    if graph.get_entity(&field_id).is_none() {
                        let mut fld_ent = CodeEntity::new(&field_id, &format!("{}.{}", sobj, fld), EntityType::Column, rel_path, 1, 1);
                        fld_ent.signature = Some(format!("Field {}.{}", sobj, fld));
                        graph.add_entity(fld_ent);
                    }
                    graph.add_edge(&proc_id, &field_id, EdgeKind::References);
                }
            }
        }
    }

    // Extract approval steps and assigned approvers
    for (idx, step_cap) in step_re.captures_iter(content).enumerate() {
        let step_block = step_cap.get(1).unwrap().as_str();
        let step_name = step_name_re.captures(step_block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("step");
        let step_label = step_label_re.captures(step_block).map(|c| c.get(1).unwrap().as_str()).unwrap_or(step_name);

        let step_id = format!("{}::step::{}", proc_id, step_name);
        let mut step_ent = CodeEntity::new(&step_id, step_label, EntityType::Rule, rel_path, idx + 1, idx + 1);
        step_ent.signature = Some(format!("ApprovalStep {}", step_label));

        let mut approver_name: Option<String> = None;
        if let Some(appr_cap) = approver_re.captures(step_block) {
            let appr_name = appr_cap.get(1).unwrap().as_str();
            step_ent.metadata.insert("assignedApprover".to_string(), appr_name.to_string());
            approver_name = Some(appr_name.to_string());
        }

        graph.add_entity(step_ent);
        graph.add_edge(&proc_id, &step_id, EdgeKind::Contains);

        if let Some(appr_name) = approver_name {
            if let Some(ref sobj) = target_object_name {
                if appr_name.ends_with("__c") {
                    let field_id = format!("sobject::{}.{}", sobj, appr_name);
                    if graph.get_entity(&field_id).is_none() {
                        let fld_ent = CodeEntity::new(&field_id, &format!("{}.{}", sobj, appr_name), EntityType::Column, rel_path, idx + 1, idx + 1);
                        graph.add_entity(fld_ent);
                    }
                    graph.add_edge(&step_id, &field_id, EdgeKind::References);
                }
            }
        }
    }
}

/// Parse Salesforce Decision Matrix XML (.decisionMatrix-meta.xml)
pub fn parse_decision_matrix(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let path = Path::new(rel_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let fallback_name = file_name.strip_suffix(".decisionMatrix-meta.xml").unwrap_or(file_name);

    let label_re = Regex::new(r"<label>([^<]+)</label>").unwrap();
    let col_re = Regex::new(r"(?s)<columns>(.*?)</columns>").unwrap();
    let col_name_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
    let col_type_re = Regex::new(r"<columnType>([^<]+)</columnType>").unwrap();
    let col_dtype_re = Regex::new(r"<dataType>([^<]+)</dataType>").unwrap();

    let name = label_re.captures(content).map(|c| c.get(1).unwrap().as_str().to_string()).unwrap_or_else(|| fallback_name.to_string());
    let matrix_id = format!("decision_matrix::{}", fallback_name);

    let mut matrix_entity = CodeEntity::new(&matrix_id, &name, EntityType::Table, rel_path, 1, 1);
    matrix_entity.signature = Some(format!("DecisionMatrix {}", name));
    matrix_entity.metadata.insert("kind".to_string(), "DecisionMatrix".to_string());
    graph.add_entity(matrix_entity);

    for (idx, col_cap) in col_re.captures_iter(content).enumerate() {
        let block = col_cap.get(1).unwrap().as_str();
        let c_name = col_name_re.captures(block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("col");
        let c_type = col_type_re.captures(block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("Input");
        let c_dtype = col_dtype_re.captures(block).map(|c| c.get(1).unwrap().as_str()).unwrap_or("Text");

        let col_id = format!("{}.{}", matrix_id, c_name);
        let mut col_ent = CodeEntity::new(&col_id, &format!("{}.{}", fallback_name, c_name), EntityType::Column, rel_path, idx + 1, idx + 1);
        col_ent.signature = Some(format!("{} Column {}: {}", c_type, c_name, c_dtype));
        col_ent.metadata.insert("columnType".to_string(), c_type.to_string());
        col_ent.metadata.insert("dataType".to_string(), c_dtype.to_string());

        graph.add_entity(col_ent);
        graph.add_edge(&matrix_id, &col_id, EdgeKind::Contains);
    }
}
