use std::path::Path;
use regex::Regex;
use crate::graph::{CodeGraph, CodeEntity, EntityType, EdgeKind};

/// Normalize project-relative OSB references (e.g. "Services/BusinessServices/BillingBS" -> "BillingBS").
pub fn normalize_osb_ref(raw_ref: &str) -> String {
    let clean = raw_ref.trim();
    clean.rsplit('/').next().unwrap_or(clean).to_string()
}

/// Helper to ensure a target entity exists before adding an edge.
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

/// Detect whether a file or XML payload is an Oracle SOA/OSB artifact (.proxy, .pipeline, .bix, .wsdl, .xsd).
pub fn is_oracle_soa_file(path: &Path, content: &str) -> bool {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(extension, "proxy" | "pipeline" | "bix" | "wsdl") {
        return true;
    }

    let trimmed = content.trim_start();
    if !trimmed.starts_with('<') {
        return false;
    }

    let osb_roots = [
        "proxyServiceEntry",
        "ProxyServiceType",
        "pipelineEntry",
        "businessServiceEntry",
        "BusinessServiceType",
        "definitions",
        "schema",
    ];

    if extension == "xsd" || extension == "xml" {
        let has_osb_ns = content.contains("http://www.bea.com/wli/sb")
            || content.contains("http://schemas.xmlsoap.org/wsdl/")
            || content.contains("http://xmlns.oracle.com")
            || content.contains("http://www.w3.org/2001/XMLSchema");

        if has_osb_ns && osb_roots.iter().any(|r| content.contains(r)) {
            return true;
        }
    }

    false
}

/// Universal parser for Oracle Service Bus (OSB) and SOA artifacts (.proxy, .pipeline, .bix, .wsdl, .xsd).
pub fn parse_oracle_soa(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let path = Path::new(rel_path);
    let default_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("osb_entity");
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // 1. OSB Proxy Service (.proxy or <ser:proxyServiceEntry)
    if extension == "proxy" || content.contains("proxyServiceEntry") || content.contains("ProxyServiceType") {
        parse_osb_proxy(rel_path, content, default_name, graph);
        return;
    }

    // 2. OSB Pipeline (.pipeline or <con:pipelineEntry)
    if extension == "pipeline" || content.contains("pipelineEntry") {
        parse_osb_pipeline(rel_path, content, default_name, graph);
        return;
    }

    // 3. OSB Business Service (.bix or <ser:businessServiceEntry)
    if extension == "bix" || content.contains("businessServiceEntry") || content.contains("BusinessServiceType") {
        parse_osb_business_service(rel_path, content, default_name, graph);
        return;
    }

    // 4. WSDL Service Definition (.wsdl or <wsdl:definitions)
    if extension == "wsdl" || content.contains("<wsdl:definitions") || content.contains("<definitions") {
        parse_wsdl_definition(rel_path, content, default_name, graph);
        return;
    }

    // 5. XSD XML Schema Definition (.xsd or <xsd:schema)
    if extension == "xsd" || content.contains("<xsd:schema") || content.contains("<xs:schema") {
        parse_xsd_schema(rel_path, content, default_name, graph);
    }
}

/// Parse Oracle Service Bus Proxy Service (.proxy).
fn parse_osb_proxy(rel_path: &str, content: &str, default_name: &str, graph: &mut CodeGraph) {
    let name_re = Regex::new(r#"<(?:\w+:)?name>([^<]+)</(?:\w+:)?name>"#).unwrap();
    let uri_re = Regex::new(r#"<(?:\w+:)?URI>\s*<(?:\w+:)?value>([^<]+)</(?:\w+:)?value>"#).unwrap();
    let pipeline_ref_re = Regex::new(r#"<(?:\w+:)?pipeline\s+ref=["']([^"']+)["']"#).unwrap();
    let service_ref_re = Regex::new(r#"<(?:\w+:)?service\s+ref=["']([^"']+)["'](?:\s+xsi:type=["'](?:[^"']*:)?BusinessServiceRef["'])?"#).unwrap();
    let wsdl_ref_re = Regex::new(r#"<(?:\w+:)?wsdl\s+ref=["']([^"']+)["']"#).unwrap();

    let proxy_name = name_re.captures(content).map(|c| c.get(1).unwrap().as_str().trim()).unwrap_or(default_name);
    let proxy_id = format!("osb_proxy::{}", proxy_name);
    let endpoint_uri = uri_re.captures(content).map(|c| c.get(1).unwrap().as_str().trim()).unwrap_or("");

    let mut entity = CodeEntity::new(&proxy_id, proxy_name, EntityType::Interface, rel_path, 1, 1);
    entity.signature = Some(if endpoint_uri.is_empty() {
        format!("OSB ProxyService {}", proxy_name)
    } else {
        format!("OSB ProxyService {} (endpoint: {})", proxy_name, endpoint_uri)
    });
    if !endpoint_uri.is_empty() {
        entity.metadata.insert("endpointUri".to_string(), endpoint_uri.to_string());
    }
    graph.add_entity(entity);

    // Link to Pipeline (.pipeline)
    if let Some(caps) = pipeline_ref_re.captures(content) {
        let target_pipe = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let pipe_id = format!("osb_pipeline::{}", target_pipe);
        ensure_entity_and_add_edge(graph, &proxy_id, &pipe_id, &target_pipe, EntityType::Function, rel_path, EdgeKind::Calls);
    }

    // Direct routing to Business Service (OSB 11g or direct pass-through proxy)
    for caps in service_ref_re.captures_iter(content) {
        let target_bs = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let bs_id = format!("osb_bix::{}", target_bs);
        ensure_entity_and_add_edge(graph, &proxy_id, &bs_id, &target_bs, EntityType::Module, rel_path, EdgeKind::Calls);
    }

    // Link to WSDL contract
    if let Some(caps) = wsdl_ref_re.captures(content) {
        let target_wsdl = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let wsdl_id = format!("wsdl::{}", target_wsdl);
        ensure_entity_and_add_edge(graph, &proxy_id, &wsdl_id, &target_wsdl, EntityType::Interface, rel_path, EdgeKind::References);
    }

    // If OSB 11g embedded message flow is present, parse inner routing actions
    if content.contains("<ser:router") || content.contains("<con:route") {
        parse_pipeline_actions(rel_path, content, &proxy_id, graph);
    }
}

/// Parse Oracle Service Bus Pipeline (.pipeline).
fn parse_osb_pipeline(rel_path: &str, content: &str, default_name: &str, graph: &mut CodeGraph) {
    let name_re = Regex::new(r#"<(?:\w+:)?pipelineEntry(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    let pipe_name = name_re.captures(content).map(|c| c.get(1).unwrap().as_str().trim()).unwrap_or(default_name);
    let pipe_id = format!("osb_pipeline::{}", pipe_name);

    let mut entity = CodeEntity::new(&pipe_id, pipe_name, EntityType::Function, rel_path, 1, 1);
    entity.signature = Some(format!("OSB Pipeline {}", pipe_name));
    graph.add_entity(entity);

    parse_pipeline_actions(rel_path, content, &pipe_id, graph);
}

/// Internal helper to extract routing, transformations, and Java callouts from a pipeline or router block.
fn parse_pipeline_actions(rel_path: &str, content: &str, source_id: &str, graph: &mut CodeGraph) {
    // 1. Business Service routing & callouts
    let bs_route_re = Regex::new(r#"<(?:\w+:)?service\s+ref=["']([^"']+)["'](?:\s+xsi:type=["'](?:[^"']*:)?BusinessServiceRef["'])?"#).unwrap();
    for caps in bs_route_re.captures_iter(content) {
        let raw_ref = caps.get(1).unwrap().as_str();
        if !raw_ref.is_empty() {
            let bs_name = normalize_osb_ref(raw_ref);
            let bs_id = format!("osb_bix::{}", bs_name);
            ensure_entity_and_add_edge(graph, source_id, &bs_id, &bs_name, EntityType::Module, rel_path, EdgeKind::Calls);
        }
    }

    // 2. Sub-pipeline and downstream proxy callouts
    let pipe_callout_re = Regex::new(r#"<(?:\w+:)?service\s+ref=["']([^"']+)["']\s+xsi:type=["'](?:[^"']*:)?PipelineRef["']"#).unwrap();
    for caps in pipe_callout_re.captures_iter(content) {
        let sub_pipe = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let pipe_id = format!("osb_pipeline::{}", sub_pipe);
        ensure_entity_and_add_edge(graph, source_id, &pipe_id, &sub_pipe, EntityType::Function, rel_path, EdgeKind::Calls);
    }

    let proxy_callout_re = Regex::new(r#"<(?:\w+:)?service\s+ref=["']([^"']+)["']\s+xsi:type=["'](?:[^"']*:)?ProxyServiceRef["']"#).unwrap();
    for caps in proxy_callout_re.captures_iter(content) {
        let sub_proxy = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let proxy_id = format!("osb_proxy::{}", sub_proxy);
        ensure_entity_and_add_edge(graph, source_id, &proxy_id, &sub_proxy, EntityType::Interface, rel_path, EdgeKind::Calls);
    }

    // 3. XSLT and XQuery transformations
    let xslt_re = Regex::new(r#"<(?:\w+:)?xslt\s+ref=["']([^"']+)["']"#).unwrap();
    for caps in xslt_re.captures_iter(content) {
        let tr_name = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let tr_id = format!("transform::{}", tr_name);
        ensure_entity_and_add_edge(graph, source_id, &tr_id, &tr_name, EntityType::Function, rel_path, EdgeKind::References);
    }

    let xq_re = Regex::new(r#"<(?:\w+:)?xqueryTransform>[\s\S]*?<(?:\w+:)?resource\s+ref=["']([^"']+)["']"#).unwrap();
    for caps in xq_re.captures_iter(content) {
        let tr_name = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let tr_id = format!("transform::{}", tr_name);
        ensure_entity_and_add_edge(graph, source_id, &tr_id, &tr_name, EntityType::Function, rel_path, EdgeKind::References);
    }

    // 4. Java Callout invocations
    let java_callout_block_re = Regex::new(r#"<(?:\w+:)?javaCallout[^>]*>([\s\S]*?)</(?:\w+:)?javaCallout>"#).unwrap();
    let java_cls_re = Regex::new(r#"<(?:\w+:)?class(?:Name)?>([^<]+)</(?:\w+:)?class(?:Name)?>"#).unwrap();
    let java_meth_re = Regex::new(r#"<(?:\w+:)?method(?:Name)?>([^<]+)</(?:\w+:)?method(?:Name)?>"#).unwrap();

    for block in java_callout_block_re.captures_iter(content) {
        let blk_str = block.get(1).unwrap().as_str();
        if let Some(c_caps) = java_cls_re.captures(blk_str) {
            let cls_full = c_caps.get(1).unwrap().as_str().trim();
            let cls_short = cls_full.rsplit('.').next().unwrap_or(cls_full);
            let java_cls_id = format!("java::{}", cls_full);

            ensure_entity_and_add_edge(graph, source_id, &java_cls_id, cls_short, EntityType::Class, rel_path, EdgeKind::Calls);

            if let Some(m_caps) = java_meth_re.captures(blk_str) {
                let meth_raw = m_caps.get(1).unwrap().as_str().trim();
                // Strip method signature modifiers (e.g. "public static String generateToken(String)" -> "generateToken")
                let meth_clean = meth_raw.split('(').next().unwrap_or(meth_raw).split_whitespace().next_back().unwrap_or(meth_raw);
                let java_meth_id = format!("java::{}.{}", cls_full, meth_clean);
                ensure_entity_and_add_edge(graph, source_id, &java_meth_id, meth_clean, EntityType::Method, rel_path, EdgeKind::Calls);
            }
        }
    }
}

/// Parse Oracle Service Bus Business Service (.bix).
fn parse_osb_business_service(rel_path: &str, content: &str, default_name: &str, graph: &mut CodeGraph) {
    let name_re = Regex::new(r#"<(?:\w+:)?name>([^<]+)</(?:\w+:)?name>"#).unwrap();
    let uri_re = Regex::new(r#"<(?:\w+:)?URI>\s*<(?:\w+:)?value>([^<]+)</(?:\w+:)?value>"#).unwrap();
    let wsdl_ref_re = Regex::new(r#"<(?:\w+:)?wsdl\s+ref=["']([^"']+)["']"#).unwrap();
    let jca_op_re = Regex::new(r#"<(?:\w+:)?operation-name>([^<]+)</(?:\w+:)?operation-name>"#).unwrap();
    let jca_proc_re = Regex::new(r#"<(?:\w+:)?property\s+name=["'](?:ProcedureName|PackageName)["']\s+value=["']([^"']+)["']"#).unwrap();

    let bs_name = name_re.captures(content).map(|c| c.get(1).unwrap().as_str().trim()).unwrap_or(default_name);
    let bs_id = format!("osb_bix::{}", bs_name);
    let endpoint_uri = uri_re.captures(content).map(|c| c.get(1).unwrap().as_str().trim()).unwrap_or("");

    let mut entity = CodeEntity::new(&bs_id, bs_name, EntityType::Module, rel_path, 1, 1);
    entity.signature = Some(if endpoint_uri.is_empty() {
        format!("OSB BusinessService {}", bs_name)
    } else {
        format!("OSB BusinessService {} (endpoint: {})", bs_name, endpoint_uri)
    });
    if !endpoint_uri.is_empty() {
        entity.metadata.insert("endpointUri".to_string(), endpoint_uri.to_string());
    }
    graph.add_entity(entity);

    // Link to WSDL contract if declared
    if let Some(caps) = wsdl_ref_re.captures(content) {
        let target_wsdl = normalize_osb_ref(caps.get(1).unwrap().as_str());
        let wsdl_id = format!("wsdl::{}", target_wsdl);
        ensure_entity_and_add_edge(graph, &bs_id, &wsdl_id, &target_wsdl, EntityType::Interface, rel_path, EdgeKind::References);
    }

    // Link to JCA DB Adapter / Stored Procedure
    if let Some(caps) = jca_op_re.captures(content) {
        let op_name = caps.get(1).unwrap().as_str().trim();
        let plsql_id = format!("plsql::{}", op_name);
        ensure_entity_and_add_edge(graph, &bs_id, &plsql_id, op_name, EntityType::Method, rel_path, EdgeKind::Calls);
    }
    for caps in jca_proc_re.captures_iter(content) {
        let proc_name = caps.get(1).unwrap().as_str().trim();
        let plsql_id = format!("plsql::{}", proc_name);
        ensure_entity_and_add_edge(graph, &bs_id, &plsql_id, proc_name, EntityType::Method, rel_path, EdgeKind::Calls);
    }
}

/// Parse WSDL Service & Operations definition (.wsdl).
fn parse_wsdl_definition(rel_path: &str, content: &str, default_name: &str, graph: &mut CodeGraph) {
    let def_re = Regex::new(r#"<(?:\w+:)?definitions(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    let service_name = def_re.captures(content).map(|c| c.get(1).unwrap().as_str().trim()).unwrap_or(default_name);
    let wsdl_id = format!("wsdl::{}", service_name);

    let mut entity = CodeEntity::new(&wsdl_id, service_name, EntityType::Interface, rel_path, 1, 1);
    entity.signature = Some(format!("WSDL Service {}", service_name));
    graph.add_entity(entity);

    // Extract portType operations
    let op_re = Regex::new(r#"<(?:\w+:)?operation(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    let input_msg_re = Regex::new(r#"<(?:\w+:)?input(?:\s+[^>]*?)?\s+message=["'](?:[^:]+:)?([^"']+)["']"#).unwrap();
    let output_msg_re = Regex::new(r#"<(?:\w+:)?output(?:\s+[^>]*?)?\s+message=["'](?:[^:]+:)?([^"']+)["']"#).unwrap();

    for caps in op_re.captures_iter(content) {
        let op_name = caps.get(1).unwrap().as_str().trim();
        let op_id = format!("{}::{}", wsdl_id, op_name);
        let mut op_ent = CodeEntity::new(&op_id, op_name, EntityType::Method, rel_path, 1, 1);
        op_ent.signature = Some(format!("{}.{}()", service_name, op_name));
        graph.add_entity(op_ent);
        graph.add_edge(&wsdl_id, &op_id, EdgeKind::Contains);
    }

    // Extract input/output messages referencing XSD contracts
    for caps in input_msg_re.captures_iter(content) {
        let msg_name = caps.get(1).unwrap().as_str().trim();
        let xsd_id = format!("xsd::{}", msg_name);
        ensure_entity_and_add_edge(graph, &wsdl_id, &xsd_id, msg_name, EntityType::Struct, rel_path, EdgeKind::References);
    }
    for caps in output_msg_re.captures_iter(content) {
        let msg_name = caps.get(1).unwrap().as_str().trim();
        let xsd_id = format!("xsd::{}", msg_name);
        ensure_entity_and_add_edge(graph, &wsdl_id, &xsd_id, msg_name, EntityType::Struct, rel_path, EdgeKind::References);
    }
}

/// Parse XSD XML Schema Definition (.xsd).
fn parse_xsd_schema(rel_path: &str, content: &str, default_name: &str, graph: &mut CodeGraph) {
    let tns_re = Regex::new(r#"targetNamespace=["']([^"']+)["']"#).unwrap();
    let schema_name = tns_re.captures(content)
        .map(|c| c.get(1).unwrap().as_str().rsplit('/').next().unwrap_or(default_name))
        .unwrap_or(default_name);
    let xsd_id = format!("xsd::{}", schema_name);

    let mut entity = CodeEntity::new(&xsd_id, schema_name, EntityType::Module, rel_path, 1, 1);
    entity.signature = Some(format!("XSD Schema {}", schema_name));
    graph.add_entity(entity);

    // Extract complexType and root element contracts
    let type_re = Regex::new(r#"<(?:\w+:)?(?:complexType|element)(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    for caps in type_re.captures_iter(content) {
        let type_name = caps.get(1).unwrap().as_str().trim();
        if !type_name.is_empty() {
            let type_id = format!("{}::{}", xsd_id, type_name);
            let mut type_ent = CodeEntity::new(&type_id, type_name, EntityType::Struct, rel_path, 1, 1);
            type_ent.signature = Some(format!("{}:{}", schema_name, type_name));
            graph.add_entity(type_ent);
            graph.add_edge(&xsd_id, &type_id, EdgeKind::Contains);
        }
    }
}
