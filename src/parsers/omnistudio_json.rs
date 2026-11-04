use std::collections::HashSet;
use std::path::Path;
use regex::Regex;
use serde_json::Value;
use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};

/// Maximum file size for OmniStudio/JSON source parsing (5MB)
const MAX_OMNISTUDIO_JSON_SIZE: usize = 5 * 1024 * 1024;

/// Check if a relative path and filename indicates an OmniStudio, Vlocity, or Salesforce declarative JSON code artifact.
pub fn is_omnistudio_json_path(rel_path: &str, file_name: &str) -> bool {
    let lower_file = file_name.to_ascii_lowercase();
    let lower_path = rel_path.to_ascii_lowercase().replace('\\', "/");

    // Explicit exclusions for lockfiles and noise
    if lower_file == "package-lock.json"
        || lower_file == "pnpm-lock.yaml"
        || lower_file.ends_with("-lock.json")
        || lower_file.ends_with(".lock.json")
    {
        return false;
    }

    // 1. DataPack filename conventions
    if lower_file.ends_with("_datapack.json")
        || lower_file.ends_with(".datapack.json")
        || lower_file.ends_with("_items.json")
        || (lower_file.contains("_element_") && lower_file.ends_with(".json"))
    {
        return true;
    }

    // 2. OmniStudio / Vlocity folder path conventions
    if lower_path.contains("/integrationprocedure/")
        || lower_path.contains("/dataraptor/")
        || lower_path.contains("/omniscript/")
        || lower_path.contains("/omnicard/")
        || lower_path.contains("/vlocitycard/")
        || lower_path.contains("/product2/")
        || lower_path.contains("/epc/")
        || lower_path.contains("/datapacks/")
        || lower_path.contains("/vlocity/")
        || lower_path.contains("/omnistudio/")
        || lower_path.starts_with("integrationprocedure/")
        || lower_path.starts_with("dataraptor/")
        || lower_path.starts_with("omniscript/")
        || lower_path.starts_with("omnicard/")
        || lower_path.starts_with("product2/")
        || lower_path.starts_with("datapacks/")
    {
        return lower_file.ends_with(".json");
    }

    false
}

/// Content-based detection for OmniStudio, Vlocity Communications Cloud, and EPC JSON code.
pub fn is_omnistudio_json_content(content: &str) -> bool {
    let prefix_sample = if content.len() > 8192 {
        &content[..8192]
    } else {
        content
    };

    prefix_sample.contains("OmniProcessType")
        || prefix_sample.contains("OmniProcessKey")
        || prefix_sample.contains("PropertySetConfig")
        || prefix_sample.contains("vlocity_namespace")
        || prefix_sample.contains("vlocity_cmt")
        || prefix_sample.contains("DataRaptorMapItem")
        || prefix_sample.contains("DRMapItem")
        || prefix_sample.contains("DataRaptorType")
        || prefix_sample.contains("JSONAttribute")
        || prefix_sample.contains("ProductChildItem")
        || prefix_sample.contains("FormulaExpression")
        || prefix_sample.contains("FormulaResultPath")
}

/// Universal OmniStudio and Communications Cloud declarative JSON source parser.
pub fn parse_omnistudio_json(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    if content.len() > MAX_OMNISTUDIO_JSON_SIZE {
        return;
    }

    let parsed: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Handle JSON object or array of objects
    match parsed {
        Value::Object(ref map) => {
            dispatch_omnistudio_object(rel_path, map, graph);
        }
        Value::Array(ref arr) => {
            for item in arr {
                if let Value::Object(ref map) = item {
                    dispatch_omnistudio_object(rel_path, map, graph);
                }
            }
        }
        _ => {}
    }
}

/// Dispatch a parsed JSON object to the specialized OmniStudio/EPC parser.
fn dispatch_omnistudio_object(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    // 1. DataRaptor DataPack / Items / Mappings
    if is_dataraptor_object(obj) {
        parse_dataraptor_json(rel_path, obj, graph);
        return;
    }

    // 2. Integration Procedure DataPack or Element
    if is_integration_procedure_object(obj) {
        parse_integration_procedure_json(rel_path, obj, graph);
        return;
    }

    // 3. Enterprise Product Catalog (EPC) & Product2 DataPack
    if is_epc_product_object(obj) {
        parse_epc_product_json(rel_path, obj, graph);
        return;
    }

    // 4. OmniScript / OmniCard DataPack
    if is_omniscript_object(obj) {
        parse_omniscript_json(rel_path, obj, graph);
        return;
    }

    // 5. Standalone Element JSON (e.g. `_Element_*.json`)
    if obj.contains_key("PropertySetConfig")
        || obj.contains_key("%vlocity_namespace%__PropertySet__c")
        || obj.contains_key("vlocity_cmt__PropertySet__c")
    {
        parse_standalone_element_json(rel_path, obj, graph);
    }
}

// ---------------------------------------------------------------------------
// Detection Helpers
// ---------------------------------------------------------------------------

fn get_string_field<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(val) = obj.get(*k) {
            if let Some(s) = val.as_str() {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn is_integration_procedure_object(obj: &serde_json::Map<String, Value>) -> bool {
    if let Some(proc_type) = get_string_field(obj, &["OmniProcessType", "%vlocity_namespace%__OmniProcessType__c", "vlocity_cmt__OmniProcessType__c", "Type", "%vlocity_namespace%__Type__c"]) {
        if proc_type.eq_ignore_ascii_case("Integration Procedure") {
            return true;
        }
    }
    obj.contains_key("OmniProcessKey")
        || obj.contains_key("%vlocity_namespace%__OmniProcessKey__c")
        || (obj.contains_key("%vlocity_namespace%__Element__c") && obj.contains_key("%vlocity_namespace%__Type__c"))
}

fn is_dataraptor_object(obj: &serde_json::Map<String, Value>) -> bool {
    obj.contains_key("%vlocity_namespace%__DataRaptorMapItem__c")
        || obj.contains_key("vlocity_cmt__DataRaptorMapItem__c")
        || obj.contains_key("DataRaptorMapItem")
        || obj.contains_key("DRMapItem")
        || obj.contains_key("DataRaptorType")
        || obj.contains_key("DRBundle")
        || obj.contains_key("%vlocity_namespace%__DRBundle__c")
        || (obj.contains_key("FormulaExpression") && obj.contains_key("FormulaResultPath"))
        || (obj.contains_key("%vlocity_namespace%__FormulaExpression__c") && obj.contains_key("%vlocity_namespace%__FormulaResultPath__c"))
}

fn is_epc_product_object(obj: &serde_json::Map<String, Value>) -> bool {
    obj.contains_key("ProductCode")
        || obj.contains_key("%vlocity_namespace%__JSONAttribute__c")
        || obj.contains_key("vlocity_cmt__JSONAttribute__c")
        || obj.contains_key("%vlocity_namespace%__ProductChildItem__c")
        || obj.contains_key("vlocity_cmt__ProductChildItem__c")
        || obj.contains_key("ProductChildItem")
}

fn is_omniscript_object(obj: &serde_json::Map<String, Value>) -> bool {
    if let Some(proc_type) = get_string_field(obj, &["OmniProcessType", "%vlocity_namespace%__OmniProcessType__c", "Type"]) {
        if proc_type.eq_ignore_ascii_case("OmniScript") || proc_type.eq_ignore_ascii_case("OS") {
            return true;
        }
    }
    obj.contains_key("OmniScript") || obj.contains_key("OmniCard")
}

// ---------------------------------------------------------------------------
// 1. Integration Procedures (IPs) & Relational Flow Graphs
// ---------------------------------------------------------------------------

pub fn parse_integration_procedure_json(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let proc_type = get_string_field(obj, &["%vlocity_namespace%__Type__c", "Type"]).unwrap_or("IP");
    let proc_sub_type = get_string_field(obj, &["%vlocity_namespace%__SubType__c", "SubType"]).unwrap_or("Default");
    let ip_key = get_string_field(obj, &["OmniProcessKey", "%vlocity_namespace%__OmniProcessKey__c", "Name"])
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}_{}", proc_type, proc_sub_type));

    let ip_id = format!("omni_ip::{}", ip_key);
    let mut ip_entity = CodeEntity::new(&ip_id, &ip_key, EntityType::Module, rel_path, 1, 1);
    ip_entity.signature = Some(format!("IntegrationProcedure {}/{}", proc_type, proc_sub_type));
    ip_entity.metadata.insert("kind".to_string(), "IntegrationProcedure".to_string());
    ip_entity.metadata.insert("type".to_string(), proc_type.to_string());
    ip_entity.metadata.insert("subType".to_string(), proc_sub_type.to_string());

    if let Some(desc) = get_string_field(obj, &["%vlocity_namespace%__Description__c", "Description", "description"]) {
        ip_entity.docstring = Some(desc.to_string());
    }
    graph.add_entity(ip_entity);

    // Extract elements (steps)
    let elements = extract_elements_array(obj);
    let mut prev_step_id: Option<String> = None;

    for (idx, elem_val) in elements.iter().enumerate() {
        if let Value::Object(ref elem_obj) = elem_val {
            let elem_name = get_string_field(elem_obj, &["Name", "name", "%vlocity_namespace%__Name__c"]).unwrap_or_else(|| "Step");
            let elem_type = get_string_field(elem_obj, &["%vlocity_namespace%__Type__c", "Type", "type"]).unwrap_or("Action");
            let level = get_string_field(elem_obj, &["%vlocity_namespace%__Level__c", "Level", "level"]).unwrap_or("0");
            let order = get_string_field(elem_obj, &["%vlocity_namespace%__Order__c", "Order", "order"]).unwrap_or("0");

            let step_id = format!("{}::{}", ip_id, elem_name);
            let mut step_entity = CodeEntity::new(&step_id, elem_name, EntityType::Function, rel_path, idx + 1, idx + 1);
            step_entity.signature = Some(format!("Step {} ({}) [level {} order {}]", elem_name, elem_type, level, order));
            step_entity.metadata.insert("kind".to_string(), "OmniStep".to_string());
            step_entity.metadata.insert("elementType".to_string(), elem_type.to_string());
            step_entity.metadata.insert("level".to_string(), level.to_string());
            step_entity.metadata.insert("order".to_string(), order.to_string());
            graph.add_entity(step_entity);

            // Connect IP to Step
            graph.add_edge(&ip_id, &step_id, EdgeKind::Contains);

            // Sequential Flow Transition: Link previous step to current step (Relational Flow Graph)
            if let Some(ref prev_id) = prev_step_id {
                graph.add_edge(prev_id, &step_id, EdgeKind::Calls);
            }
            prev_step_id = Some(step_id.clone());

            // Check if element has a parent block (e.g. inside ConditionalBlock or LoopBlock)
            if let Some(parent_name) = get_string_field(elem_obj, &["%vlocity_namespace%__ParentElementId__c", "ParentElementId", "parentElementId"]) {
                let parent_id = format!("{}::{}", ip_id, parent_name);
                graph.add_edge(&parent_id, &step_id, EdgeKind::Calls);
            }

            // Inspect PropertySet for relational call actions
            inspect_property_set(elem_obj, &step_id, rel_path, graph);
        }
    }
}

/// Helper to ensure a target entity exists in the graph before adding an edge.
fn ensure_entity_and_add_edge(
    graph: &mut CodeGraph,
    source_id: &str,
    target_id: &str,
    target_name: &str,
    target_type: EntityType,
    rel_path: &str,
    kind: EdgeKind,
) {
    if graph.get_entity(target_id).is_none() {
        let mut target_ent = CodeEntity::new(target_id, target_name, target_type.clone(), rel_path, 1, 1);
        target_ent.signature = Some(format!("{:?} {}", target_type, target_name));
        graph.add_entity(target_ent);
    }
    graph.add_edge(source_id, target_id, kind);
}

/// Helper to parse standalone element JSON files (e.g. `_Element_*.json`).
fn parse_standalone_element_json(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let elem_name = get_string_field(obj, &["Name", "name"]).unwrap_or_else(|| {
        Path::new(rel_path).file_stem().and_then(|s| s.to_str()).unwrap_or("Element")
    });
    let elem_type = get_string_field(obj, &["%vlocity_namespace%__Type__c", "Type"]).unwrap_or("Action");
    let proc_key = get_string_field(obj, &["%vlocity_namespace%__OmniProcessId__c", "OmniProcessKey"]).unwrap_or("IP");

    let ip_id = format!("omni_ip::{}", proc_key);
    let step_id = format!("{}::{}", ip_id, elem_name);

    let mut step_entity = CodeEntity::new(&step_id, elem_name, EntityType::Function, rel_path, 1, 1);
    step_entity.signature = Some(format!("Step {} ({})", elem_name, elem_type));
    step_entity.metadata.insert("elementType".to_string(), elem_type.to_string());
    graph.add_entity(step_entity);

    graph.add_edge(&ip_id, &step_id, EdgeKind::Contains);
    inspect_property_set(obj, &step_id, rel_path, graph);
}

/// Inspect PropertySet config (nested map or serialized JSON string) for cross-language links.
fn inspect_property_set(
    elem_obj: &serde_json::Map<String, Value>,
    step_id: &str,
    rel_path: &str,
    graph: &mut CodeGraph,
) {
    // Resolve PropertySet either as JSON object or JSON string
    let props_opt = elem_obj.get("PropertySetConfig")
        .or_else(|| elem_obj.get("%vlocity_namespace%__PropertySet__c"))
        .or_else(|| elem_obj.get("vlocity_cmt__PropertySet__c"))
        .or_else(|| elem_obj.get("propertySetConfig"));

    let props_map: Option<serde_json::Map<String, Value>> = match props_opt {
        Some(Value::Object(m)) => Some(m.clone()),
        Some(Value::String(s)) => serde_json::from_str(s).ok(),
        _ => None,
    };

    if let Some(props) = props_map {
        // 1. Apex Remote Actions: `remoteClass` & `remoteMethod`
        if let Some(cls) = get_string_field(&props, &["remoteClass", "RemoteClass", "className"]) {
            let apex_cls_id = format!("apex::{}", cls);
            ensure_entity_and_add_edge(graph, step_id, &apex_cls_id, cls, EntityType::Class, rel_path, EdgeKind::Calls);

            if let Some(meth) = get_string_field(&props, &["remoteMethod", "RemoteMethod", "methodName"]) {
                let apex_meth_id = format!("apex::{}.{}", cls, meth);
                ensure_entity_and_add_edge(graph, step_id, &apex_meth_id, &format!("{}.{}", cls, meth), EntityType::Method, rel_path, EdgeKind::Calls);
            }
        }

        // 2. DataRaptor Actions: `bundleName`
        if let Some(bundle) = get_string_field(&props, &["bundleName", "BundleName", "dataRaptorBundle", "bundle"]) {
            let dr_id = format!("omni_dr::{}", bundle);
            ensure_entity_and_add_edge(graph, step_id, &dr_id, bundle, EntityType::Module, rel_path, EdgeKind::Calls);
        }

        // 3. Sub-Integration Procedure Actions: `integrationProcedureKey`
        if let Some(sub_ip) = get_string_field(&props, &["integrationProcedureKey", "IntegrationProcedureKey", "procedureKey"]) {
            let sub_ip_id = format!("omni_ip::{}", sub_ip);
            ensure_entity_and_add_edge(graph, step_id, &sub_ip_id, sub_ip, EntityType::Module, rel_path, EdgeKind::Calls);
        }

        // 4. Custom LWC components: `customLwcName`
        if let Some(lwc) = get_string_field(&props, &["customLwcName", "CustomLwcName", "lwcName"]) {
            let lwc_id = format!("lwc::{}", lwc);
            ensure_entity_and_add_edge(graph, step_id, &lwc_id, lwc, EntityType::Class, rel_path, EdgeKind::Calls);
        }

        // 5. Conditional formulas
        if let Some(formula) = get_string_field(&props, &["executionConditionalFormula", "formula", "Formula"]) {
            parse_formula_identifiers(formula, step_id, rel_path, graph);
        }
    }
}

fn extract_elements_array(obj: &serde_json::Map<String, Value>) -> Vec<Value> {
    for k in &["%vlocity_namespace%__Element__c", "vlocity_cmt__Element__c", "elements", "Elements", "children"] {
        if let Some(val) = obj.get(*k) {
            if let Some(arr) = val.as_array() {
                return arr.clone();
            }
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// 2. DataRaptors (DataMappers) & Field-Level Formula Lineage
// ---------------------------------------------------------------------------

pub fn parse_dataraptor_json(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let bundle_name = get_string_field(obj, &["Name", "name", "%vlocity_namespace%__BundleName__c", "BundleName"])
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            Path::new(rel_path).file_stem().and_then(|s| s.to_str()).unwrap_or("DataRaptor").to_string()
        });

    let dr_type = get_string_field(obj, &["%vlocity_namespace%__Type__c", "Type", "DataRaptorType"]).unwrap_or("Extract");

    let dr_id = format!("omni_dr::{}", bundle_name);
    let mut dr_entity = CodeEntity::new(&dr_id, &bundle_name, EntityType::Module, rel_path, 1, 1);
    dr_entity.signature = Some(format!("DataRaptor {} ({})", bundle_name, dr_type));
    dr_entity.metadata.insert("kind".to_string(), "DataRaptor".to_string());
    dr_entity.metadata.insert("drType".to_string(), dr_type.to_string());

    if let Some(desc) = get_string_field(obj, &["Description", "%vlocity_namespace%__Description__c"]) {
        dr_entity.docstring = Some(desc.to_string());
    }
    graph.add_entity(dr_entity);

    // Extract mapping items
    let items = extract_mapping_items(obj);
    let mut seen_sobjects: HashSet<String> = HashSet::new();

    for (idx, item_val) in items.iter().enumerate() {
        if let Value::Object(ref item) = item_val {
            let line_num = idx + 1;

            // 1. Input SObject and field mapping
            if let Some(input_obj) = get_string_field(item, &["%vlocity_namespace%__InputObjectName__c", "InputObjectName", "inputObjectName"]) {
                if !input_obj.is_empty() && !input_obj.eq_ignore_ascii_case("json") {
                    let sobject_id = format!("sobject::{}", input_obj);
                    if !seen_sobjects.contains(&sobject_id) {
                        let mut sobj_entity = CodeEntity::new(&sobject_id, input_obj, EntityType::Table, rel_path, line_num, line_num);
                        sobj_entity.signature = Some(format!("CustomObject {}", input_obj));
                        graph.add_entity(sobj_entity);
                        seen_sobjects.insert(sobject_id.clone());
                    }
                    graph.add_edge(&dr_id, &sobject_id, EdgeKind::References);

                    if let Some(input_fld) = get_string_field(item, &["%vlocity_namespace%__InputFieldName__c", "InputFieldName", "inputFieldName"]) {
                        if !input_fld.is_empty() && !input_fld.contains(':') && !input_fld.contains(' ') {
                            let field_id = format!("sobject::{}.{}", input_obj, input_fld);
                            let mut col_entity = CodeEntity::new(&field_id, input_fld, EntityType::Column, rel_path, line_num, line_num);
                            col_entity.signature = Some(format!("Field {}.{}", input_obj, input_fld));
                            graph.add_entity(col_entity);
                            graph.add_edge(&sobject_id, &field_id, EdgeKind::Contains);
                            graph.add_edge(&dr_id, &field_id, EdgeKind::References);
                        }
                    }
                }
            }

            // 2. Output SObject (e.g. for Load DataRaptors)
            if let Some(output_obj) = get_string_field(item, &["%vlocity_namespace%__OutputObjectName__c", "OutputObjectName", "outputObjectName"]) {
                if !output_obj.is_empty() && !output_obj.eq_ignore_ascii_case("json") {
                    let sobject_id = format!("sobject::{}", output_obj);
                    if !seen_sobjects.contains(&sobject_id) {
                        let mut sobj_entity = CodeEntity::new(&sobject_id, output_obj, EntityType::Table, rel_path, line_num, line_num);
                        sobj_entity.signature = Some(format!("CustomObject {}", output_obj));
                        graph.add_entity(sobj_entity);
                        seen_sobjects.insert(sobject_id.clone());
                    }
                    graph.add_edge(&dr_id, &sobject_id, EdgeKind::References);
                }
            }

            // 3. Node-Level DataRaptor Formula Lineage
            let formula_expr_opt = get_string_field(item, &["%vlocity_namespace%__FormulaExpression__c", "FormulaExpression", "formulaExpression", "formula"]);
            let formula_res_opt = get_string_field(item, &["%vlocity_namespace%__FormulaResultPath__c", "FormulaResultPath", "formulaResultPath", "resultPath"]);

            if let (Some(expr), Some(res_path)) = (formula_expr_opt, formula_res_opt) {
                parse_dataraptor_formula_lineage(&dr_id, res_path, expr, rel_path, line_num, graph);
            }
        }
    }
}

/// Parse DataRaptor formulas down to node-level lineage.
/// Resolves field transformations inside JSON formula strings to granular SObject and column dependencies.
fn parse_dataraptor_formula_lineage(
    dr_id: &str,
    result_path: &str,
    formula_expr: &str,
    rel_path: &str,
    line_num: usize,
    graph: &mut CodeGraph,
) {
    let formula_id = format!("{}::formula::{}", dr_id, result_path);
    let mut formula_entity = CodeEntity::new(&formula_id, result_path, EntityType::Function, rel_path, line_num, line_num);
    formula_entity.signature = Some(format!("Formula {} = {}", result_path, formula_expr));
    formula_entity.metadata.insert("kind".to_string(), "DataRaptorFormula".to_string());
    formula_entity.metadata.insert("formulaExpression".to_string(), formula_expr.to_string());
    formula_entity.metadata.insert("resultPath".to_string(), result_path.to_string());
    graph.add_entity(formula_entity);

    // Link DR bundle to Formula node
    graph.add_edge(dr_id, &formula_id, EdgeKind::Contains);

    // Regex to match tokenized field references: `Object:Field` or `Object.Field`
    let field_ref_re = Regex::new(r"([A-Za-z0-9_]+)[:\.]([A-Za-z0-9_]+)").unwrap();
    for cap in field_ref_re.captures_iter(formula_expr) {
        let obj_name = cap.get(1).unwrap().as_str();
        let field_name = cap.get(2).unwrap().as_str();

        // Avoid common syntax words or numbers
        if obj_name.chars().all(|c| c.is_ascii_digit()) || field_name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let sobject_id = format!("sobject::{}", obj_name);
        let field_id = format!("sobject::{}.{}", obj_name, field_name);

        if graph.get_entity(&sobject_id).is_none() {
            let mut sobj_entity = CodeEntity::new(&sobject_id, obj_name, EntityType::Table, rel_path, line_num, line_num);
            sobj_entity.signature = Some(format!("CustomObject {}", obj_name));
            graph.add_entity(sobj_entity);
        }

        if graph.get_entity(&field_id).is_none() {
            let mut col_entity = CodeEntity::new(&field_id, field_name, EntityType::Column, rel_path, line_num, line_num);
            col_entity.signature = Some(format!("Field {}.{}", obj_name, field_name));
            graph.add_entity(col_entity);
            graph.add_edge(&sobject_id, &field_id, EdgeKind::Contains);
        }

        // Granular Lineage: Input Field feeds Formula node
        graph.add_edge(&field_id, &formula_id, EdgeKind::References);
        graph.add_edge(&sobject_id, &formula_id, EdgeKind::References);
    }

    // Connect Formula node to Result Path target
    if result_path.contains(':') || result_path.contains('.') {
        let parts: Vec<&str> = result_path.split(&[':', '.'][..]).collect();
        if parts.len() >= 2 {
            let target_obj = parts[0];
            let target_fld = parts[1];
            let target_sobj_id = format!("sobject::{}", target_obj);
            let target_field_id = format!("sobject::{}.{}", target_obj, target_fld);

            if graph.get_entity(&target_sobj_id).is_none() {
                let mut sobj_entity = CodeEntity::new(&target_sobj_id, target_obj, EntityType::Table, rel_path, line_num, line_num);
                sobj_entity.signature = Some(format!("CustomObject {}", target_obj));
                graph.add_entity(sobj_entity);
            }
            if graph.get_entity(&target_field_id).is_none() {
                let mut col_entity = CodeEntity::new(&target_field_id, target_fld, EntityType::Column, rel_path, line_num, line_num);
                col_entity.signature = Some(format!("Field {}.{}", target_obj, target_fld));
                graph.add_entity(col_entity);
                graph.add_edge(&target_sobj_id, &target_field_id, EdgeKind::Contains);
            }
            graph.add_edge(&formula_id, &target_field_id, EdgeKind::References);
        }
    }
}

fn extract_mapping_items(obj: &serde_json::Map<String, Value>) -> Vec<Value> {
    for k in &["%vlocity_namespace%__DataRaptorMapItem__c", "vlocity_cmt__DataRaptorMapItem__c", "items", "Items", "mapItems"] {
        if let Some(val) = obj.get(*k) {
            if let Some(arr) = val.as_array() {
                return arr.clone();
            }
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// 3. Enterprise Product Catalog (EPC) & `vlocity_cmt__JSONAttribute__c`
// ---------------------------------------------------------------------------

pub fn parse_epc_product_json(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let product_code = get_string_field(obj, &["ProductCode", "productCode", "%vlocity_namespace%__GlobalKey__c", "Name"])
        .unwrap_or("PRODUCT");
    let product_name = get_string_field(obj, &["Name", "name", "ProductCode"]).unwrap_or(product_code);

    let prod_id = format!("epc::Product::{}", product_code);
    let mut prod_entity = CodeEntity::new(&prod_id, product_name, EntityType::Class, rel_path, 1, 1);
    prod_entity.signature = Some(format!("EPC Product {} ({})", product_name, product_code));
    prod_entity.metadata.insert("kind".to_string(), "EPCProduct".to_string());
    prod_entity.metadata.insert("productCode".to_string(), product_code.to_string());

    if let Some(desc) = get_string_field(obj, &["Description", "description"]) {
        prod_entity.docstring = Some(desc.to_string());
    }
    graph.add_entity(prod_entity);

    // Link to standard SObject Product2
    let prod2_id = "sobject::Product2";
    if graph.get_entity(prod2_id).is_none() {
        let mut sobj_entity = CodeEntity::new(prod2_id, "Product2", EntityType::Table, rel_path, 1, 1);
        sobj_entity.signature = Some("CustomObject Product2".to_string());
        graph.add_entity(sobj_entity);
    }
    graph.add_edge(&prod_id, prod2_id, EdgeKind::References);

    // 1. Parse Child Product Composition (`ProductChildItem`)
    parse_epc_product_child_items(&prod_id, obj, rel_path, graph);

    // 2. Parse Attribute Blobs & Overrides (`vlocity_cmt__JSONAttribute__c`)
    parse_epc_json_attributes(&prod_id, obj, rel_path, graph);

    // 3. Pricing Matrix / Decision Matrix Rules
    if let Some(matrix) = get_string_field(obj, &["pricingMatrix", "priceMatrix", "matrixName", "%vlocity_namespace%__PriceList__c"]) {
        let matrix_id = format!("decision_matrix::{}", matrix);
        ensure_entity_and_add_edge(graph, &prod_id, &matrix_id, matrix, EntityType::Rule, rel_path, EdgeKind::References);
    }
}

fn parse_epc_product_child_items(
    parent_prod_id: &str,
    obj: &serde_json::Map<String, Value>,
    rel_path: &str,
    graph: &mut CodeGraph,
) {
    let pci_keys = ["%vlocity_namespace%__ProductChildItem__c", "vlocity_cmt__ProductChildItem__c", "ProductChildItem", "childProducts"];
    for k in &pci_keys {
        if let Some(val) = obj.get(*k) {
            if let Some(arr) = val.as_array() {
                for item_val in arr {
                    if let Value::Object(ref item) = item_val {
                        if let Some(child_code) = get_string_field(item, &["ChildProductCode", "%vlocity_namespace%__ChildProductId__c", "ChildProductId", "ProductCode"]) {
                            let child_id = format!("epc::Product::{}", child_code);
                            if graph.get_entity(&child_id).is_none() {
                                let mut child_ent = CodeEntity::new(&child_id, child_code, EntityType::Class, rel_path, 1, 1);
                                child_ent.signature = Some(format!("EPC Product {}", child_code));
                                graph.add_entity(child_ent);
                            }
                            // Parent product composes Child product
                            graph.add_edge(parent_prod_id, &child_id, EdgeKind::Contains);
                        }
                    }
                }
            }
        }
    }
}

fn parse_epc_json_attributes(
    prod_id: &str,
    obj: &serde_json::Map<String, Value>,
    rel_path: &str,
    graph: &mut CodeGraph,
) {
    let attr_keys = ["%vlocity_namespace%__JSONAttribute__c", "vlocity_cmt__JSONAttribute__c", "JSONAttribute", "attributes"];
    for k in &attr_keys {
        if let Some(val) = obj.get(*k) {
            match val {
                Value::String(s) => {
                    // Try parsing as JSON array
                    if let Ok(parsed_arr) = serde_json::from_str::<Value>(s) {
                        if let Some(arr) = parsed_arr.as_array() {
                            process_attribute_nodes(prod_id, arr, rel_path, graph);
                        }
                    } else {
                        // Regex fallback for attributeuniquecode__c in string
                        let attr_re = Regex::new(r#"(?:attributeuniquecode__c|code)":\s*"([^"]+)""#).unwrap();
                        for cap in attr_re.captures_iter(s) {
                            let attr_code = cap.get(1).unwrap().as_str();
                            link_attribute_entity(prod_id, attr_code, None, None, rel_path, graph);
                        }
                    }
                }
                Value::Array(arr) => {
                    process_attribute_nodes(prod_id, arr, rel_path, graph);
                }
                _ => {}
            }
        }
    }
}

fn process_attribute_nodes(
    prod_id: &str,
    arr: &[Value],
    rel_path: &str,
    graph: &mut CodeGraph,
) {
    for item in arr {
        if let Value::Object(ref attr_obj) = item {
            if let Some(code) = get_string_field(attr_obj, &["attributeuniquecode__c", "code", "AttributeCode", "name"]) {
                let cat = get_string_field(attr_obj, &["categorycode__c", "category", "CategoryCode"]);
                let val = get_string_field(attr_obj, &["value__c", "value", "defaultValue"]);
                link_attribute_entity(prod_id, code, cat, val, rel_path, graph);
            }
        }
    }
}

fn link_attribute_entity(
    prod_id: &str,
    attr_code: &str,
    category: Option<&str>,
    value: Option<&str>,
    rel_path: &str,
    graph: &mut CodeGraph,
) {
    let attr_id = format!("epc::Attribute::{}", attr_code);
    if graph.get_entity(&attr_id).is_none() {
        let mut attr_ent = CodeEntity::new(&attr_id, attr_code, EntityType::Variable, rel_path, 1, 1);
        let cat_str = category.unwrap_or("General");
        let val_str = value.unwrap_or("");
        attr_ent.signature = Some(format!("EPC Attribute {} [category: {} default: {}]", attr_code, cat_str, val_str));
        attr_ent.metadata.insert("kind".to_string(), "EPCAttribute".to_string());
        attr_ent.metadata.insert("category".to_string(), cat_str.to_string());
        graph.add_entity(attr_ent);
    }
    // Product defines/overrides Attribute
    graph.add_edge(prod_id, &attr_id, EdgeKind::Contains);
}

// ---------------------------------------------------------------------------
// 4. OmniScripts & OmniCards
// ---------------------------------------------------------------------------

pub fn parse_omniscript_json(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let os_type = get_string_field(obj, &["Type", "%vlocity_namespace%__Type__c"]).unwrap_or("OS");
    let os_sub_type = get_string_field(obj, &["SubType", "%vlocity_namespace%__SubType__c"]).unwrap_or("Default");
    let os_key = format!("{}_{}", os_type, os_sub_type);

    let os_id = format!("omni_os::{}", os_key);
    let mut os_entity = CodeEntity::new(&os_id, &os_key, EntityType::Module, rel_path, 1, 1);
    os_entity.signature = Some(format!("OmniScript {}/{}", os_type, os_sub_type));
    os_entity.metadata.insert("kind".to_string(), "OmniScript".to_string());
    graph.add_entity(os_entity);

    let elements = extract_elements_array(obj);
    for (idx, elem_val) in elements.iter().enumerate() {
        if let Value::Object(ref elem_obj) = elem_val {
            let elem_name = get_string_field(elem_obj, &["Name", "name"]).unwrap_or("Step");
            let elem_type = get_string_field(elem_obj, &["Type", "%vlocity_namespace%__Type__c"]).unwrap_or("Step");

            let step_id = format!("{}::{}", os_id, elem_name);
            let mut step_entity = CodeEntity::new(&step_id, elem_name, EntityType::Function, rel_path, idx + 1, idx + 1);
            step_entity.signature = Some(format!("Step {} ({})", elem_name, elem_type));
            graph.add_entity(step_entity);
            graph.add_edge(&os_id, &step_id, EdgeKind::Contains);

            inspect_property_set(elem_obj, &step_id, rel_path, graph);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_formula_identifiers(formula: &str, step_id: &str, rel_path: &str, graph: &mut CodeGraph) {
    let field_re = Regex::new(r"([A-Za-z0-9_]+)[:\.]([A-Za-z0-9_]+)").unwrap();
    for cap in field_re.captures_iter(formula) {
        let obj_name = cap.get(1).unwrap().as_str();
        let field_name = cap.get(2).unwrap().as_str();

        let sobject_id = format!("sobject::{}", obj_name);
        let field_id = format!("sobject::{}.{}", obj_name, field_name);

        if graph.get_entity(&sobject_id).is_none() {
            let mut sobj_entity = CodeEntity::new(&sobject_id, obj_name, EntityType::Table, rel_path, 1, 1);
            sobj_entity.signature = Some(format!("CustomObject {}", obj_name));
            graph.add_entity(sobj_entity);
        }
        if graph.get_entity(&field_id).is_none() {
            let mut col_entity = CodeEntity::new(&field_id, field_name, EntityType::Column, rel_path, 1, 1);
            col_entity.signature = Some(format!("Field {}.{}", obj_name, field_name));
            graph.add_entity(col_entity);
            graph.add_edge(&sobject_id, &field_id, EdgeKind::Contains);
        }

        graph.add_edge(step_id, &sobject_id, EdgeKind::References);
        graph.add_edge(step_id, &field_id, EdgeKind::References);
    }
}
