use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::Indexer;
use ai_rumin::mcp::MCPServer;
use ai_rumin::parsers::jvm_xml::parse_plsql;
use ai_rumin::parsers::salesforce::{parse_apex_class, parse_omnistudio_metadata, parse_salesforce_metadata};
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{CodeEntity, EdgeKind, EntityType};
use serde_json::json;
use std::fs;

#[test]
fn test_sql_schema_qualified_and_column_extraction() {
    let mut graph = CodeGraph::new();
    let sql_code = r#"
        CREATE TABLE SIEBEL.S_ASSET (
            ROW_ID VARCHAR2(15) NOT NULL,
            CREATED_BY VARCHAR2(15),
            ASSET_NUM VARCHAR2(100)
        );

        CREATE OR REPLACE PROCEDURE SIEBEL.SYNC_ASSET_STATUS IS
        BEGIN
            NULL;
        END SYNC_ASSET_STATUS;
    "#;

    parse_plsql("database/siebel_asset.sql", sql_code, &mut graph);

    // Verify table entity
    let table = graph.find_node_by_name_or_id("SIEBEL.S_ASSET");
    assert!(table.is_some(), "Table SIEBEL.S_ASSET should be extracted");
    let table_idx = table.unwrap();
    let table_entity = &graph.graph[table_idx];
    assert_eq!(table_entity.name, "SIEBEL.S_ASSET");
    assert_eq!(table_entity.entity_type, EntityType::Table);

    // Verify procedure entity with schema prefix
    let proc = graph.find_node_by_name_or_id("SIEBEL.SYNC_ASSET_STATUS");
    assert!(proc.is_some(), "Procedure SIEBEL.SYNC_ASSET_STATUS should be extracted");

    // Verify child columns
    let row_id_opt = graph.find_node_by_name_or_id("SIEBEL.S_ASSET.ROW_ID");
    assert!(row_id_opt.is_some(), "Column ROW_ID should be extracted");
    let row_id_idx = row_id_opt.unwrap();
    assert_eq!(graph.graph[row_id_idx].entity_type, EntityType::Column);

    let asset_num_opt = graph.find_node_by_name_or_id("SIEBEL.S_ASSET.ASSET_NUM");
    assert!(asset_num_opt.is_some(), "Column ASSET_NUM should be extracted");

    // Verify Containment edges: SIEBEL.S_ASSET -> Contains -> SIEBEL.S_ASSET.ROW_ID
    let has_contains = graph.graph.contains_edge(table_idx, row_id_idx);
    assert!(has_contains, "Table should have Contains edge to column");
}

#[test]
fn test_custom_metadata_parsing_and_whitelisting() {
    // Whitelist check: all *-meta.xml files are supported
    assert!(Indexer::is_supported_file("Feature_Config.Payment_Gateway.md-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("en_US.translation-meta.xml", "xml"));
    assert!(!Indexer::is_supported_file("en_US.translation.txt", "txt"));

    let mut graph = CodeGraph::new();
    let cmdt_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomMetadata xmlns="http://soap.sforce.com/2006/04/metadata">
    <label>Payment Gateway</label>
    <protected>false</protected>
    <values>
        <field>Endpoint_URL__c</field>
        <value>https://api.gateway.example.com</value>
    </values>
    <values>
        <field>Timeout_Seconds__c</field>
        <value>30</value>
    </values>
</CustomMetadata>"#;

    parse_salesforce_metadata("force-app/main/default/customMetadata/Feature_Config.Payment_Gateway.md-meta.xml", cmdt_content, &mut graph);

    let cmdt_node = graph.find_node_by_name_or_id("Feature_Config.Payment_Gateway");
    assert!(cmdt_node.is_some(), "CMDT record entity should be indexed");
    let cmdt_idx = cmdt_node.unwrap();
    let cmdt = &graph.graph[cmdt_idx];
    assert_eq!(cmdt.name, "Feature_Config.Payment_Gateway");
    assert_eq!(cmdt.entity_type, EntityType::Variable);
    assert!(cmdt.docstring.as_ref().unwrap().contains("Label: Payment Gateway"));
    assert!(cmdt.docstring.as_ref().unwrap().contains("Endpoint_URL__c"));

    // Verify parent SObject type and edge
    let parent_node = graph.id_to_index.get("sobject::Feature_Config__mdt");
    assert!(parent_node.is_some(), "Parent sobject::Feature_Config__mdt should exist");

    let parent_idx = *parent_node.unwrap();
    assert!(graph.graph.contains_edge(parent_idx, cmdt_idx), "Parent sobject should Contain CMDT record");
}

#[test]
fn test_describe_entity_introspection() {
    let mut graph = CodeGraph::new();

    let mut parent = CodeEntity::new("OrderService", "OrderService", EntityType::Class, "src/services/OrderService.java", 1, 100);
    parent.docstring = Some("Handles end-to-end checkout processing.".to_string());
    graph.add_entity(parent);

    let method = CodeEntity::new("OrderService.processOrder", "processOrder", EntityType::Method, "src/services/OrderService.java", 15, 30);
    graph.add_entity(method);

    let field = CodeEntity::new("OrderService.dbClient", "dbClient", EntityType::Variable, "src/services/OrderService.java", 5, 5);
    graph.add_entity(field);

    let caller = CodeEntity::new("CheckoutController", "CheckoutController", EntityType::Class, "src/controllers/CheckoutController.java", 1, 50);
    graph.add_entity(caller);

    let callee = CodeEntity::new("PaymentGateway", "PaymentGateway", EntityType::Class, "src/services/PaymentGateway.java", 1, 80);
    graph.add_entity(callee);

    // Wire relationships
    graph.add_edge("OrderService", "OrderService.processOrder", EdgeKind::Contains);
    graph.add_edge("OrderService", "OrderService.dbClient", EdgeKind::Contains);
    graph.add_edge("CheckoutController", "OrderService", EdgeKind::Calls);
    graph.add_edge("OrderService", "PaymentGateway", EdgeKind::Calls);

    // Introspect OrderService
    let desc = graph.describe_entity("OrderService").expect("Should describe OrderService");
    assert_eq!(desc.entity.id, "OrderService");
    assert_eq!(desc.entity.name, "OrderService");
    assert_eq!(desc.entity.docstring.as_deref(), Some("Handles end-to-end checkout processing."));
    assert_eq!(desc.methods.len(), 1);
    assert_eq!(desc.methods[0].name, "processOrder");
    assert_eq!(desc.fields.len(), 1);
    assert_eq!(desc.fields[0].name, "dbClient");

    // Check relations
    assert_eq!(desc.incoming_relations.len(), 1);
    assert_eq!(desc.incoming_relations[0].2, "CheckoutController");
    assert_eq!(desc.outgoing_relations.len(), 1);
    assert_eq!(desc.outgoing_relations[0].2, "PaymentGateway");
}

#[test]
fn test_symbol_pathfinding() {
    let mut graph = CodeGraph::new();

    graph.add_entity(CodeEntity::new("NodeA", "NodeA", EntityType::Class, "A.rs", 1, 10));
    graph.add_entity(CodeEntity::new("NodeB", "NodeB", EntityType::Class, "B.rs", 1, 10));
    graph.add_entity(CodeEntity::new("NodeC", "NodeC", EntityType::Class, "C.rs", 1, 10));
    graph.add_entity(CodeEntity::new("NodeD", "NodeD", EntityType::Class, "D.rs", 1, 10));

    // A -> Calls -> B -> Calls -> C
    graph.add_edge("NodeA", "NodeB", EdgeKind::Calls);
    graph.add_edge("NodeB", "NodeC", EdgeKind::Calls);

    // Path from A to C
    let paths = graph.find_paths("NodeA", "NodeC", 5);
    assert_eq!(paths.len(), 1);
    let path = &paths[0];
    assert_eq!(path.source, "NodeA");
    assert_eq!(path.target, "NodeC");
    assert_eq!(path.hops, 2);
    assert_eq!(path.steps.len(), 2);
    assert_eq!(path.steps[0].from.name, "NodeA");
    assert_eq!(path.steps[0].to.name, "NodeB");
    assert_eq!(path.steps[0].edge, EdgeKind::Calls);
    assert_eq!(path.steps[1].from.name, "NodeB");
    assert_eq!(path.steps[1].to.name, "NodeC");
    assert_eq!(path.steps[1].edge, EdgeKind::Calls);

    // Path from A to D (disconnected)
    let disconnected = graph.find_paths("NodeA", "NodeD", 5);
    assert!(disconnected.is_empty());
}

#[tokio::test]
async fn test_mcp_tools_describe_and_find_path() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_desc_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();
    let cp_dir = temp_dir.join(".rumin/checkpoints");
    fs::create_dir_all(&cp_dir).unwrap();
    let default_cp = cp_dir.join("default.bin");

    let mut graph = CodeGraph::new();
    let ent_a = CodeEntity::new("ServiceA", "ServiceA", EntityType::Class, "src/A.ts", 1, 20);
    let ent_b = CodeEntity::new("ServiceB", "ServiceB", EntityType::Class, "src/B.ts", 1, 20);
    graph.add_entity(ent_a);
    graph.add_entity(ent_b);
    graph.add_edge("ServiceA", "ServiceB", EdgeKind::Calls);
    graph.save_binary_checkpoint(&default_cp).unwrap();

    let sidecar_script = temp_dir.join("worker.py");
    let server = MCPServer::new(temp_dir.clone(), sidecar_script);
    server.graph_swap().store(std::sync::Arc::new(graph));

    // Call rumin_describe
    let desc_req = json!({
        "jsonrpc": "2.0",
        "id": 101,
        "method": "tools/call",
        "params": {
            "name": "rumin_describe",
            "arguments": {
                "entity_name": "ServiceA"
            }
        }
    });

    let desc_resp = server.handle_request_for_test(desc_req).await;
    assert!(desc_resp.is_some());
    let desc_val = desc_resp.unwrap();
    let text_content = desc_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text_content.contains("\"name\": \"ServiceA\""));
    assert!(text_content.contains("ServiceB"));

    // Call rumin_find_path
    let path_req = json!({
        "jsonrpc": "2.0",
        "id": 102,
        "method": "tools/call",
        "params": {
            "name": "rumin_find_path",
            "arguments": {
                "source_symbol": "ServiceA",
                "target_symbol": "ServiceB",
                "max_depth": 3
            }
        }
    });

    let path_resp = server.handle_request_for_test(path_req).await;
    assert!(path_resp.is_some());
    let path_val = path_resp.unwrap();
    let path_text = path_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(path_text.contains("\"hops\": 1"));
    assert!(path_text.contains("\"source\": \"ServiceA\""));
    assert!(path_text.contains("\"target\": \"ServiceB\""));
}

#[test]
fn test_sobject_reference_to_and_granular_soql_edges() {
    let mut graph = CodeGraph::new();

    // 1. Test <referenceTo> in SObject field metadata (Gap 2)
    let field_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomField xmlns="http://soap.sforce.com/2006/04/metadata">
    <fullName>Order__c</fullName>
    <label>Related Order</label>
    <referenceTo>Order</referenceTo>
    <relationshipName>PaymentSchedules</relationshipName>
    <type>Lookup</type>
</CustomField>"#;

    parse_salesforce_metadata(
        "force-app/main/default/objects/Payment_Schedule__c/fields/Order__c.field-meta.xml",
        field_xml,
        &mut graph,
    );

    let field_node = graph.find_node_by_name_or_id("sobject::Payment_Schedule__c.Order__c");
    assert!(field_node.is_some(), "Field node should be created");
    let target_order = graph.find_node_by_name_or_id("sobject::Order");
    assert!(target_order.is_some(), "Referenced sobject::Order node should be created");
    let parent_sobject = graph.find_node_by_name_or_id("sobject::Payment_Schedule__c");
    assert!(parent_sobject.is_some(), "Parent sobject::Payment_Schedule__c should be created");

    let f_idx = field_node.unwrap();
    let o_idx = target_order.unwrap();
    let p_idx = parent_sobject.unwrap();

    // Edge from field to target sobject: DependsOn
    assert!(graph.graph.contains_edge(f_idx, o_idx), "Field should depend on target SObject");
    // Edge from parent sobject to target sobject: References
    assert!(graph.graph.contains_edge(p_idx, o_idx), "Parent SObject should reference target SObject");

    // 2. Test granular field-level SOQL query extraction (Gap 2)
    let apex_code = r#"
public with sharing class OrderManager {
    public static List<Order> getDraftOrders() {
        return [SELECT Id, Name, TotalAmount, Status FROM Order WHERE Status = 'Draft'];
    }
}
"#;

    parse_apex_class("force-app/main/default/classes/OrderManager.cls", apex_code, &mut graph);

    // Verify field-level column nodes under sobject::Order
    for field_name in &["Id", "Name", "TotalAmount", "Status"] {
        let col_id = format!("sobject::Order.{}", field_name);
        let col_node = graph.find_node_by_name_or_id(&col_id);
        assert!(col_node.is_some(), "Column node {} should be extracted from SOQL", col_id);
        let col_idx = col_node.unwrap();
        assert_eq!(graph.graph[col_idx].entity_type, EntityType::Column);

        // Parent sobject::Order should Contain column
        assert!(graph.graph.contains_edge(o_idx, col_idx), "sobject::Order should Contain {}", col_id);
    }

    // Verify OrderManager class references the extracted field nodes
    let mgr_node = graph.find_node_by_name_or_id("apex::OrderManager").expect("OrderManager class node should exist");
    for field_name in &["Id", "Name", "TotalAmount", "Status"] {
        let col_id = format!("sobject::Order.{}", field_name);
        let col_idx = graph.find_node_by_name_or_id(&col_id).unwrap();
        assert!(graph.graph.contains_edge(mgr_node, col_idx), "OrderManager should have References edge to {}", col_id);
    }
}

#[test]
fn test_omnistudio_metadata_parser() {
    // Gap 5 Whitelist check
    assert!(Indexer::is_supported_file("B2B_Checkout.oip-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("OrderWizard.os-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("ProductCard.ouc-meta.xml", "xml"));

    let mut graph = CodeGraph::new();

    let ip_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<OmniProcess xmlns="http://soap.sforce.com/2006/04/metadata">
    <name>B2B_Checkout</name>
    <description>Checkout orchestration integration procedure</description>
    <omniProcessElements>
        <name>RemoteCartPricing</name>
        <level>0.0</level>
        <propertySetConfig>{&quot;remoteClass&quot;:&quot;CartPricingService&quot;,&quot;remoteMethod&quot;:&quot;calculateTax&quot;}</propertySetConfig>
    </omniProcessElements>
    <omniProcessElements>
        <name>ExtractCatalog</name>
        <level>1.0</level>
        <propertySetConfig>{&quot;bundleName&quot;:&quot;DR_ExtractCatalogItems&quot;}</propertySetConfig>
    </omniProcessElements>
    <omniProcessElements>
        <name>InvokeInventory</name>
        <level>2.0</level>
        <propertySetConfig>{&quot;integrationProcedureKey&quot;:&quot;B2B_ReserveInventory&quot;}</propertySetConfig>
    </omniProcessElements>
</OmniProcess>"#;

    parse_omnistudio_metadata(
        "force-app/main/default/omniIntegrationProcedures/B2B_Checkout.oip-meta.xml",
        ip_xml,
        &mut graph,
    );

    let ip_node = graph.find_node_by_name_or_id("omni_ip::B2B_Checkout");
    assert!(ip_node.is_some(), "Integration Procedure entity should be indexed");
    let ip_idx = ip_node.unwrap();
    assert_eq!(graph.graph[ip_idx].entity_type, EntityType::Module);
    assert_eq!(graph.graph[ip_idx].docstring.as_deref(), Some("Checkout orchestration integration procedure"));

    // Verify child steps
    let step1 = graph.find_node_by_name_or_id("omni_ip::B2B_Checkout::element::RemoteCartPricing");
    let step2 = graph.find_node_by_name_or_id("omni_ip::B2B_Checkout::element::ExtractCatalog");
    let step3 = graph.find_node_by_name_or_id("omni_ip::B2B_Checkout::element::InvokeInventory");
    assert!(step1.is_some());
    assert!(step2.is_some());
    assert!(step3.is_some());

    assert!(graph.graph.contains_edge(ip_idx, step1.unwrap()), "IP contains step 1");
    assert!(graph.graph.contains_edge(ip_idx, step2.unwrap()), "IP contains step 2");
    assert!(graph.graph.contains_edge(ip_idx, step3.unwrap()), "IP contains step 3");

    // Verify Remote Action dependency
    let remote_target = graph.find_node_by_name_or_id("CartPricingService");
    assert!(remote_target.is_some(), "Target CartPricingService should be created");
    assert!(graph.graph.contains_edge(step1.unwrap(), remote_target.unwrap()), "Step 1 should Call CartPricingService");

    // Verify DataRaptor bundle dependency
    let bundle_target = graph.find_node_by_name_or_id("bundle::DR_ExtractCatalogItems");
    assert!(bundle_target.is_some(), "DataRaptor bundle entity should be created");
    assert!(graph.graph.contains_edge(step2.unwrap(), bundle_target.unwrap()), "Step 2 should DependOn DataRaptor");

    // Verify Sub-IP call
    let sub_ip_target = graph.find_node_by_name_or_id("omni_ip::B2B_ReserveInventory");
    assert!(sub_ip_target.is_some(), "Sub-IP entity should be created");
    assert!(graph.graph.contains_edge(step3.unwrap(), sub_ip_target.unwrap()), "Step 3 should Call sub-IP");
}

#[test]
fn test_governor_limit_hazard_audit() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_gov_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let classes_dir = temp_dir.join("classes");
    fs::create_dir_all(&classes_dir).unwrap();

    let apex_code = r#"
public class TestLimits {
    public void executeBatch() {
        for (Integer i = 0; i < 5; i++) {
            List<Contact> cts = [SELECT Id FROM Contact];
            update cts;
            Http h = new Http();
            h.send(new HttpRequest());
        }
        List<Order> allOrders = [SELECT Id, Name FROM Order];
    }
}
"#;
    let cls_rel = "classes/TestLimits.cls";
    fs::write(temp_dir.join(cls_rel), apex_code).unwrap();

    let mut graph = CodeGraph::new();
    parse_apex_class(cls_rel, apex_code, &mut graph);

    let violations = graph.audit_governor_limits(&temp_dir);
    assert!(!violations.is_empty(), "Should detect governor limit violations");

    let soql_in_loop = violations.iter().find(|v| v.rule_name == "soql_in_loop");
    assert!(soql_in_loop.is_some(), "Should flag soql_in_loop");
    assert_eq!(soql_in_loop.unwrap().severity, "error");

    let dml_in_loop = violations.iter().find(|v| v.rule_name == "dml_in_loop");
    assert!(dml_in_loop.is_some(), "Should flag dml_in_loop");
    assert_eq!(dml_in_loop.unwrap().severity, "error");

    let callout_in_loop = violations.iter().find(|v| v.rule_name == "callout_in_loop");
    assert!(callout_in_loop.is_some(), "Should flag callout_in_loop");
    assert_eq!(callout_in_loop.unwrap().severity, "error");

    let unbounded_query = violations.iter().find(|v| v.rule_name == "unbounded_high_cardinality_query");
    assert!(unbounded_query.is_some(), "Should flag unbounded_high_cardinality_query on Order");
    assert_eq!(unbounded_query.unwrap().severity, "warning");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_multi_repo_graph_federation() {
    let mut graph_core = CodeGraph::new();
    let client = CodeEntity::new("AppClient", "AppClient", EntityType::Class, "src/Client.ts", 1, 10);
    graph_core.add_entity(client);

    let mut graph_billing = CodeGraph::new();
    let service = CodeEntity::new("BillingService", "BillingService", EntityType::Class, "src/Billing.ts", 1, 20);
    let invoice = CodeEntity::new("InvoiceTable", "InvoiceTable", EntityType::Table, "db/schema.sql", 1, 30);
    graph_billing.add_entity(service);
    graph_billing.add_entity(invoice);
    graph_billing.add_edge("BillingService", "InvoiceTable", EdgeKind::References);

    let count = graph_core.federate_graph(&graph_billing, "billing");
    assert_eq!(count, 2, "Merged 2 entities");

    // Check prefixed entities exist
    assert!(graph_core.find_node_by_name_or_id("billing::BillingService").is_some());
    assert!(graph_core.find_node_by_name_or_id("billing::InvoiceTable").is_some());

    // Check relationship is maintained
    let s_idx = graph_core.find_node_by_name_or_id("billing::BillingService").unwrap();
    let i_idx = graph_core.find_node_by_name_or_id("billing::InvoiceTable").unwrap();
    assert!(graph_core.graph.contains_edge(s_idx, i_idx));

    // Connect across federated boundary
    graph_core.add_edge("AppClient", "billing::BillingService", EdgeKind::Calls);

    // Path finding across federated repos
    let paths = graph_core.find_paths("AppClient", "billing::InvoiceTable", 3);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].hops, 2);
}

#[tokio::test]
async fn test_surgical_single_file_indexing() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_single_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir.join("src")).unwrap();

    let initial_file = temp_dir.join("src/Alpha.ts");
    fs::write(&initial_file, "export class AlphaRunner { run() {} }\n").unwrap();

    let sidecar = SidecarClient::new(&temp_dir.join("worker.py"));
    let indexer = Indexer::new(&temp_dir);
    let mut graph = CodeGraph::new();

    // Initial full index
    let initial_count = indexer.index_repository(&mut graph, &sidecar).await.unwrap();
    assert!(initial_count >= 1);
    assert!(graph.find_node_by_name_or_id("AlphaRunner").is_some());

    // Surgically add a new file
    let new_file = temp_dir.join("src/Beta.ts");
    fs::write(&new_file, "export class BetaWorker { execute() {} }\n").unwrap();

    let rep1 = indexer.sync_single_file(&new_file, &mut graph, &sidecar).await.unwrap();
    assert_eq!(rep1.action, "indexed");
    assert!(rep1.duration_ms < 100);
    assert!(graph.find_node_by_name_or_id("BetaWorker").is_some(), "BetaWorker should now be in graph");

    // Modify file
    fs::write(&new_file, "export class BetaWorker { execute() {} terminate() {} }\n").unwrap();
    let rep2 = indexer.sync_single_file(&new_file, &mut graph, &sidecar).await.unwrap();
    assert_eq!(rep2.action, "indexed");

    // Delete file
    fs::remove_file(&new_file).unwrap();
    let rep3 = indexer.sync_single_file(&new_file, &mut graph, &sidecar).await.unwrap();
    assert_eq!(rep3.action, "deleted");
    assert!(graph.find_node_by_name_or_id("BetaWorker").is_none(), "BetaWorker should be removed");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_mcp_gap_tools() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_mcp_gaps_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir.join("src")).unwrap();
    let cp_dir = temp_dir.join(".rumin/checkpoints");
    fs::create_dir_all(&cp_dir).unwrap();

    let server = MCPServer::new(temp_dir.clone(), temp_dir.join("worker.py"));

    // 1. rumin_audit_limits
    let audit_req = json!({
        "jsonrpc": "2.0",
        "id": 201,
        "method": "tools/call",
        "params": {
            "name": "rumin_audit_limits",
            "arguments": {}
        }
    });
    let audit_resp = server.handle_request_for_test(audit_req).await.unwrap();
    let audit_text = audit_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(audit_text.contains("[]") || audit_text.starts_with('['));

    // 2. rumin_index_file
    let test_file = temp_dir.join("src/ToolTest.ts");
    fs::write(&test_file, "export class ToolTestClass {}\n").unwrap();

    let index_file_req = json!({
        "jsonrpc": "2.0",
        "id": 202,
        "method": "tools/call",
        "params": {
            "name": "rumin_index_file",
            "arguments": {
                "file_path": test_file.to_str().unwrap()
            }
        }
    });
    let index_file_resp = server.handle_request_for_test(index_file_req).await.unwrap();
    let index_text = index_file_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(index_text.contains("\"action\": \"indexed\""));

    // 3. rumin_federate_repo
    let other_repo = std::env::temp_dir().join(format!("rumin_other_repo_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&other_repo.join("src")).unwrap();
    fs::write(other_repo.join("src/Other.ts"), "export class OtherClass {}\n").unwrap();

    let fed_req = json!({
        "jsonrpc": "2.0",
        "id": 203,
        "method": "tools/call",
        "params": {
            "name": "rumin_federate_repo",
            "arguments": {
                "repo_path": other_repo.to_str().unwrap(),
                "alias": "ext"
            }
        }
    });
    let fed_resp = server.handle_request_for_test(fed_req).await.unwrap();
    let fed_text = fed_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(fed_text.contains("\"status\":\"federated\""));
    assert!(fed_text.contains("\"alias\":\"ext\""));

    let _ = fs::remove_dir_all(&temp_dir);
    let _ = fs::remove_dir_all(&other_repo);
}

#[tokio::test]
async fn test_sql_ddl_audit_checklist_verification() {
    let mut graph = CodeGraph::new();

    // 1. S_ORG_EXT table parsing
    let s_org_ext_sql = r#"
        CREATE TABLE SIEBEL.S_ORG_EXT (
            ROW_ID VARCHAR2(15) NOT NULL,
            NAME VARCHAR2(100) NOT NULL,
            OU_NUM VARCHAR2(30)
        );
    "#;
    parse_plsql("catalist/ddl/s_org_ext.sql", s_org_ext_sql, &mut graph);

    // Checklist Item 2: Primary symbol ID must appear as catalist/ddl/s_org_ext.sql::S_ORG_EXT (NOT ::SIEBEL)
    let org_ext_ent = graph.get_entity("catalist/ddl/s_org_ext.sql::S_ORG_EXT");
    assert!(org_ext_ent.is_some(), "catalist/ddl/s_org_ext.sql::S_ORG_EXT must exist as primary symbol ID");
    let org_ext = org_ext_ent.unwrap();
    assert_eq!(org_ext.name, "S_ORG_EXT");
    assert_eq!(org_ext.id, "catalist/ddl/s_org_ext.sql::S_ORG_EXT");
    assert!(graph.get_entity("catalist/ddl/s_org_ext.sql::SIEBEL").is_none(), "Must NOT have ::SIEBEL as symbol ID");

    // Checklist Item 3: rumin_query(query: "S_ORG_EXT") scores S_ORG_EXT in top 3 PPR results
    let hippocampal = ai_rumin::hippocampal::HippocampalEngine::new(0.20);
    let query_tokens = ai_rumin::hippocampal::HippocampalEngine::tokenize("S_ORG_EXT");
    let seeds = hippocampal.find_seed_nodes(&graph, &query_tokens);
    assert!(!seeds.is_empty(), "Seeds should be found for S_ORG_EXT");
    let ppr_scores = hippocampal.personalized_pagerank(&graph, &seeds);
    assert!(!ppr_scores.is_empty(), "PPR scores must not be empty");
    let top_3: Vec<&str> = ppr_scores.iter().take(3).map(|(id, _)| id.as_str()).collect();
    assert!(
        top_3.contains(&"catalist/ddl/s_org_ext.sql::S_ORG_EXT"),
        "catalist/ddl/s_org_ext.sql::S_ORG_EXT must be in top 3 PPR results, got: {:?}",
        top_3
    );

    // 2. S_ASSET table parsing with Siebel FK columns
    let s_asset_sql = r#"
        CREATE TABLE SIEBEL.S_ASSET (
            ROW_ID VARCHAR2(15) NOT NULL,
            BILL_ACCNT_ID VARCHAR2(15),
            PROD_ID VARCHAR2(15),
            CUR_AGREE_ID VARCHAR2(15)
        );
    "#;
    parse_plsql("catalist/ddl/s_asset.sql", s_asset_sql, &mut graph);

    // Checklist Item 4: rumin_traverse(symbol_id: "catalist/ddl/s_asset.sql::S_ASSET") traverses FK edges to S_ORG_EXT, S_PROD_INT, S_DOC_AGREE
    let asset_neighbors = graph.neighbors("catalist/ddl/s_asset.sql::S_ASSET", petgraph::Direction::Outgoing);
    let ref_neighbor_ids: Vec<String> = asset_neighbors
        .iter()
        .filter(|(_, kind)| **kind == EdgeKind::References)
        .map(|(ent, _)| ent.id.clone())
        .collect();

    assert!(
        ref_neighbor_ids.iter().any(|id| id.contains("S_ORG_EXT")),
        "S_ASSET must have References edge to S_ORG_EXT, got: {:?}",
        ref_neighbor_ids
    );
    assert!(
        ref_neighbor_ids.iter().any(|id| id.contains("S_PROD_INT")),
        "S_ASSET must have References edge to S_PROD_INT, got: {:?}",
        ref_neighbor_ids
    );
    assert!(
        ref_neighbor_ids.iter().any(|id| id.contains("S_DOC_AGREE")),
        "S_ASSET must have References edge to S_DOC_AGREE, got: {:?}",
        ref_neighbor_ids
    );

    // 3. docs/salesforce-schema-ddl-analogue.sql table parsing
    let sf_ddl = r#"
        CREATE TABLE IF NOT EXISTS sf_record_type (
            id CHAR(18) PRIMARY KEY,
            developer_name VARCHAR(80) NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sf_account (
            id CHAR(18) PRIMARY KEY,
            record_type_id CHAR(18) REFERENCES sf_record_type(id),
            parent_id CHAR(18) REFERENCES sf_account(id)
        );
    "#;
    parse_plsql("docs/salesforce-schema-ddl-analogue.sql", sf_ddl, &mut graph);

    // Checklist Item 5: rumin_traverse("docs/salesforce-schema-ddl-analogue.sql::sf_account") traverses FK edges to sf_record_type and sf_account
    let account_neighbors = graph.neighbors("docs/salesforce-schema-ddl-analogue.sql::sf_account", petgraph::Direction::Outgoing);
    let sf_ref_ids: Vec<String> = account_neighbors
        .iter()
        .filter(|(_, kind)| **kind == EdgeKind::References)
        .map(|(ent, _)| ent.id.clone())
        .collect();

    assert!(
        sf_ref_ids.iter().any(|id| id == "docs/salesforce-schema-ddl-analogue.sql::sf_record_type"),
        "sf_account must have References edge to sf_record_type, got: {:?}",
        sf_ref_ids
    );
    assert!(
        sf_ref_ids.iter().any(|id| id == "docs/salesforce-schema-ddl-analogue.sql::sf_account"),
        "sf_account must have References edge to sf_account (self-reference), got: {:?}",
        sf_ref_ids
    );

    // Checklist Item 1: graph stats report sql_tables
    let stats = graph.summary_stats();
    let sql_tables = *stats.get("sql_tables").unwrap_or(&0);
    assert!(sql_tables >= 4, "sql_tables count should be at least 4, got {}", sql_tables);

    // Verify MCP tools rumin_traverse and rumin_query directly on server
    let temp_repo = std::env::temp_dir().join(format!("rumin_ddl_audit_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_repo).unwrap();
    let server = MCPServer::new(temp_repo.clone(), temp_repo.join("worker.py"));
    server.graph_swap().store(std::sync::Arc::new(graph));

    let traverse_req = json!({
        "jsonrpc": "2.0",
        "id": 301,
        "method": "tools/call",
        "params": {
            "name": "rumin_traverse",
            "arguments": {
                "symbol_id": "catalist/ddl/s_asset.sql::S_ASSET",
                "direction": "out",
                "depth": 1
            }
        }
    });
    let traverse_resp = server.handle_request_for_test(traverse_req).await.unwrap();
    let traverse_text = traverse_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(traverse_text.contains("S_ORG_EXT"), "MCP rumin_traverse must output S_ORG_EXT: {}", traverse_text);
    assert!(traverse_text.contains("S_PROD_INT"), "MCP rumin_traverse must output S_PROD_INT: {}", traverse_text);
    assert!(traverse_text.contains("S_DOC_AGREE"), "MCP rumin_traverse must output S_DOC_AGREE: {}", traverse_text);

    let query_req = json!({
        "jsonrpc": "2.0",
        "id": 302,
        "method": "tools/call",
        "params": {
            "name": "rumin_query",
            "arguments": {
                "query": "S_ORG_EXT"
            }
        }
    });
    let query_resp = server.handle_request_for_test(query_req).await.unwrap();
    let query_text = query_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(query_text.contains("catalist/ddl/s_org_ext.sql::S_ORG_EXT"), "MCP rumin_query must return S_ORG_EXT: {}", query_text);

    let _ = fs::remove_dir_all(&temp_repo);
}

#[tokio::test]
async fn test_empirical_sf_b2b_ddl_files() {
    let sf_b2b_root = std::path::Path::new("/home/wally/dev/SF-B2B");
    if !sf_b2b_root.exists() {
        return; // Skip if run in environments without local SF-B2B repo
    }

    let mut graph = CodeGraph::new();

    // Index real files from catalist/ddl and docs
    let files = [
        ("catalist/ddl/s_org_ext.sql", sf_b2b_root.join("catalist/ddl/s_org_ext.sql")),
        ("catalist/ddl/s_asset.sql", sf_b2b_root.join("catalist/ddl/s_asset.sql")),
        ("catalist/ddl/s_prod_int.sql", sf_b2b_root.join("catalist/ddl/s_prod_int.sql")),
        ("catalist/ddl/s_doc_agree.sql", sf_b2b_root.join("catalist/ddl/s_doc_agree.sql")),
        ("docs/salesforce-schema-ddl-analogue.sql", sf_b2b_root.join("docs/salesforce-schema-ddl-analogue.sql")),
    ];

    for (rel_path, abs_path) in files.iter() {
        if abs_path.exists() {
            let content = fs::read_to_string(abs_path).unwrap();
            parse_plsql(rel_path, &content, &mut graph);
        }
    }

    // 1. Verify primary symbol IDs (NOT ::SIEBEL)
    assert!(graph.get_entity("catalist/ddl/s_org_ext.sql::S_ORG_EXT").is_some(), "S_ORG_EXT entity must exist");
    assert!(graph.get_entity("catalist/ddl/s_org_ext.sql::SIEBEL").is_none(), "Must NOT collapse to ::SIEBEL");
    assert!(graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET").is_some(), "S_ASSET entity must exist");

    // 2. Traversal on real S_ASSET
    let asset_neighbors = graph.neighbors("catalist/ddl/s_asset.sql::S_ASSET", petgraph::Direction::Outgoing);
    let ref_neighbor_ids: Vec<String> = asset_neighbors
        .iter()
        .filter(|(_, kind)| **kind == EdgeKind::References)
        .map(|(ent, _)| ent.id.clone())
        .collect();

    assert!(ref_neighbor_ids.iter().any(|id| id.contains("S_ORG_EXT")), "Must traverse S_ORG_EXT: {:?}", ref_neighbor_ids);
    assert!(ref_neighbor_ids.iter().any(|id| id.contains("S_PROD_INT")), "Must traverse S_PROD_INT: {:?}", ref_neighbor_ids);
    assert!(ref_neighbor_ids.iter().any(|id| id.contains("S_DOC_AGREE")), "Must traverse S_DOC_AGREE: {:?}", ref_neighbor_ids);

    // 3. Traversal on real sf_account
    let sf_neighbors = graph.neighbors("docs/salesforce-schema-ddl-analogue.sql::sf_account", petgraph::Direction::Outgoing);
    let sf_ref_ids: Vec<String> = sf_neighbors
        .iter()
        .filter(|(_, kind)| **kind == EdgeKind::References)
        .map(|(ent, _)| ent.id.clone())
        .collect();

    assert!(sf_ref_ids.iter().any(|id| id == "docs/salesforce-schema-ddl-analogue.sql::sf_record_type"), "Must traverse sf_record_type");
    assert!(sf_ref_ids.iter().any(|id| id == "docs/salesforce-schema-ddl-analogue.sql::sf_account"), "Must traverse sf_account");

    // 4. PPR on real S_ORG_EXT
    let hippocampal = ai_rumin::hippocampal::HippocampalEngine::new(0.20);
    let query_tokens = ai_rumin::hippocampal::HippocampalEngine::tokenize("S_ORG_EXT");
    let seeds = hippocampal.find_seed_nodes(&graph, &query_tokens);
    let ppr = hippocampal.personalized_pagerank(&graph, &seeds);
    let top_3: Vec<&str> = ppr.iter().take(3).map(|(id, _)| id.as_str()).collect();
    assert!(top_3.contains(&"catalist/ddl/s_org_ext.sql::S_ORG_EXT"), "S_ORG_EXT must be in top 3 PPR, got: {:?}", top_3);
}

#[tokio::test]
async fn test_remediation_sync_feedback_ux() {
    let temp_repo = std::env::temp_dir().join(format!("rumin_test_sync_ux_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_repo).unwrap();
    let file1 = temp_repo.join("Main.cls");
    fs::write(&file1, "public class Main { public static void run() {} }").unwrap();

    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new(&temp_repo.join("worker.py"));
    let indexer = Indexer::new(&temp_repo);

    // First sync: 1 file added
    let stats1 = indexer.sync_repository_cancellable(&mut graph, &sidecar, false, None).await.unwrap();
    assert_eq!(stats1.added, 1);
    assert!(stats1.status_message.as_ref().unwrap().contains("Synchronized: +1 added"));

    // Second sync: 0 files added -> explicit UX status explaining all files are up-to-date
    let stats2 = indexer.sync_repository_cancellable(&mut graph, &sidecar, false, None).await.unwrap();
    assert_eq!(stats2.added, 0);
    assert_eq!(stats2.unchanged, 1);
    assert_eq!(stats2.manifest_tracked_files, Some(1));
    let msg = stats2.status_message.unwrap();
    assert!(msg.contains("Up-to-date: All 1 tracked files already match on-disk state"), "Status message must explain up-to-date state: {}", msg);
}

#[test]
fn test_remediation_implicit_fk_linkage_table_and_column() {
    let mut graph = CodeGraph::new();

    // 1. Target table S_BU
    let bu_sql = r#"
        CREATE TABLE SIEBEL.S_BU (
            ROW_ID VARCHAR2(15) NOT NULL,
            NAME VARCHAR2(100) NOT NULL
        );
    "#;
    parse_plsql("catalist/ddl/s_bu.sql", bu_sql, &mut graph);

    // 2. Source table S_ASSET with BU_ID and self-referential PAR_ROW_ID
    let asset_sql = r#"
        CREATE TABLE SIEBEL.S_ASSET (
            ROW_ID VARCHAR2(15) NOT NULL,
            PAR_ROW_ID VARCHAR2(15),
            BU_ID VARCHAR2(15) NOT NULL
        );
    "#;
    parse_plsql("catalist/ddl/s_asset.sql", asset_sql, &mut graph);

    // Verify BU_ID column entity metadata
    let col_entity = graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET.BU_ID").expect("BU_ID column must exist");
    assert_eq!(col_entity.metadata.get("references_table").map(|s| s.as_str()), Some("S_BU"));

    // Verify Column -> Target Table edge
    let col_neighbors = graph.neighbors("catalist/ddl/s_asset.sql::S_ASSET.BU_ID", petgraph::Direction::Outgoing);
    let col_refs: Vec<String> = col_neighbors.iter().filter(|(_, k)| **k == EdgeKind::References).map(|(e, _)| e.id.clone()).collect();
    assert!(col_refs.iter().any(|id| id.contains("S_BU")), "BU_ID column must reference S_BU table: {:?}", col_refs);
    assert!(col_refs.iter().any(|id| id.contains("S_BU.ROW_ID")), "BU_ID column must reference S_BU.ROW_ID primary key: {:?}", col_refs);

    // Verify Table -> Target Table edge
    let tbl_neighbors = graph.neighbors("catalist/ddl/s_asset.sql::S_ASSET", petgraph::Direction::Outgoing);
    let tbl_refs: Vec<String> = tbl_neighbors.iter().filter(|(_, k)| **k == EdgeKind::References).map(|(e, _)| e.id.clone()).collect();
    assert!(tbl_refs.iter().any(|id| id.contains("S_BU")), "S_ASSET table must reference S_BU table: {:?}", tbl_refs);

    // Verify Self-referential hierarchy edge (PAR_ROW_ID -> S_ASSET)
    let par_neighbors = graph.neighbors("catalist/ddl/s_asset.sql::S_ASSET.PAR_ROW_ID", petgraph::Direction::Outgoing);
    let par_refs: Vec<String> = par_neighbors.iter().filter(|(_, k)| **k == EdgeKind::References).map(|(e, _)| e.id.clone()).collect();
    assert!(par_refs.iter().any(|id| id == "catalist/ddl/s_asset.sql::S_ASSET"), "PAR_ROW_ID must reference self table S_ASSET: {:?}", par_refs);
}

#[test]
fn test_remediation_column_aware_ppr_and_bidirectional_containment() {
    let mut graph = CodeGraph::new();

    let asset_sql = r#"
        CREATE TABLE SIEBEL.S_ASSET (
            ROW_ID VARCHAR2(15) NOT NULL,
            ASSET_NUM VARCHAR2(100),
            BU_ID VARCHAR2(15) NOT NULL
        );
    "#;
    parse_plsql("catalist/ddl/s_asset.sql", asset_sql, &mut graph);

    // Add a high-degree global hub to simulate cpqAppHandler / B2BCmexAppHandler
    let hub_id = "force-app/main/default/classes/cpqAppHandler.cls::cpqAppHandler";
    let hub = CodeEntity::new(hub_id, "cpqAppHandler", EntityType::Class, "force-app/main/default/classes/cpqAppHandler.cls", 1, 100);
    graph.add_entity(hub);
    for i in 0..15 {
        let dummy_id = format!("dummy::node_{}", i);
        let dummy = CodeEntity::new(&dummy_id, &dummy_id, EntityType::Method, "dummy.cls", 1, 10);
        graph.add_entity(dummy);
        graph.add_edge(hub_id, &dummy_id, EdgeKind::References);
        graph.add_edge(&dummy_id, hub_id, EdgeKind::References);
    }

    let hippocampal = ai_rumin::hippocampal::HippocampalEngine::new(0.20);
    let tokens = ai_rumin::hippocampal::HippocampalEngine::tokenize("BU_ID");
    let seeds = hippocampal.find_seed_nodes(&graph, &tokens);

    // 1. Verify seed bonus
    let col_id = "catalist/ddl/s_asset.sql::S_ASSET.BU_ID";
    assert!(seeds.contains_key(col_id), "BU_ID column must be discovered as a seed node");
    assert!(*seeds.get(col_id).unwrap() >= 45.0, "BU_ID must receive +45 exact short_name bonus");

    // 2. Run PPR: column probability must flow to parent S_ASSET via bidirectional Contains
    let ppr = hippocampal.personalized_pagerank(&graph, &seeds);
    println!("PPR results: {:?}", ppr);
    let top_5: Vec<&str> = ppr.iter().take(5).map(|(id, _)| id.as_str()).collect();

    assert!(top_5.contains(&col_id), "BU_ID column must be in top 5 results: {:?}", top_5);
    assert!(top_5.contains(&"catalist/ddl/s_asset.sql::S_ASSET"), "Parent S_ASSET table must be in top 5 results: {:?}", top_5);
    assert!(!top_5.contains(&hub_id), "Global hub cpqAppHandler must NOT overshadow column match: {:?}", top_5);
}

#[test]
fn test_remediation_volumetric_hazard_audit_and_sql_queries() {
    let temp_repo = std::env::temp_dir().join(format!("rumin_test_volumetric_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_repo).unwrap();
    let ddl_path = temp_repo.join("s_asset_xa.sql");
    let ddl_content = r#"
-- =============================================================================
-- Table: SIEBEL.S_ASSET_XA
-- Columns: 27 | Recorded Rows: 19,821,064
-- =============================================================================
CREATE TABLE SIEBEL.S_ASSET_XA (
    ROW_ID VARCHAR2(15) NOT NULL,
    ASSET_ID VARCHAR2(15) NOT NULL,
    ORDER_ITEM_ID VARCHAR2(15) NOT NULL
);

CREATE UNIQUE INDEX SIEBEL.S_ASSET_XA_P1 ON SIEBEL.S_ASSET_XA (ROW_ID);
CREATE INDEX SIEBEL.S_ASSET_XA_F1 ON SIEBEL.S_ASSET_XA (ASSET_ID);
    "#;
    fs::write(&ddl_path, ddl_content).unwrap();

    let sql_script_path = temp_repo.join("query_migration.sql");
    let sql_script_content = r#"
-- Unbounded query on 19.8M row table
SELECT * FROM SIEBEL.S_ASSET_XA;
    "#;
    fs::write(&sql_script_path, sql_script_content).unwrap();

    let mut graph = CodeGraph::new();
    parse_plsql("s_asset_xa.sql", ddl_content, &mut graph);
    graph.file_to_entities.insert("s_asset_xa.sql".to_string(), vec!["s_asset_xa.sql::S_ASSET_XA".to_string()]);
    graph.file_to_entities.insert("query_migration.sql".to_string(), vec!["query_migration.sql".to_string()]);

    // Verify table volumetric extraction
    let tbl = graph.get_entity("s_asset_xa.sql::S_ASSET_XA").expect("S_ASSET_XA must exist");
    assert_eq!(tbl.metadata.get("recorded_rows").map(|s| s.as_str()), Some("19821064"));
    assert!(tbl.metadata.get("indexed_columns").unwrap().contains("ROW_ID"));
    assert!(tbl.metadata.get("indexed_columns").unwrap().contains("ASSET_ID"));

    // Run audit_governor_limits
    let violations = graph.audit_governor_limits(&temp_repo);

    // 1. Unindexed FK scan violation on 19.8M rows table for ORDER_ITEM_ID
    let unindexed_fk = violations.iter().find(|v| v.rule_name == "unindexed_high_cardinality_fk_scan");
    assert!(unindexed_fk.is_some(), "Must detect unindexed FK scan hazard on ORDER_ITEM_ID: {:?}", violations);
    let v_fk = unindexed_fk.unwrap();
    assert_eq!(v_fk.severity, "critical");
    assert!(v_fk.message.contains("19821064 recorded rows"), "Message must specify volumetric row count: {}", v_fk.message);
    assert!(v_fk.symbol_id.contains("ORDER_ITEM_ID"));

    // Verify ASSET_ID is NOT flagged as unindexed because it has CREATE INDEX S_ASSET_XA_F1
    assert!(!violations.iter().any(|v| v.symbol_id.contains("ASSET_ID")), "Indexed ASSET_ID must not be flagged");

    // 2. Unbounded query on high-volume table in SQL script
    let unbounded_sql = violations.iter().find(|v| v.rule_name == "unbounded_high_cardinality_query" && v.file_path == "query_migration.sql");
    assert!(unbounded_sql.is_some(), "Must detect unbounded query on S_ASSET_XA in query_migration.sql: {:?}", violations);
}

#[test]
fn test_wish1_polyglot_lwc_to_apex_and_bre_matrix_resolution() {
    let mut graph = CodeGraph::new();

    // 1. Apex Service with Decision Matrix call
    let apex_code = r#"
        public with sharing class QuoteDiscountService {
            @AuraEnabled
            public static String prepareApproval(Id quoteId) {
                Invocable.Action action = Invocable.Action.createCustomAction('runDecisionMatrix', 'Product_Discount_Tiering');
                return 'T1';
            }
        }
    "#;
    parse_apex_class("force-app/main/default/classes/QuoteDiscountService.cls", apex_code, &mut graph);

    // 2. LWC JS component importing Apex and SObject schema
    let lwc_code = r#"
        import { LightningElement, api } from 'lwc';
        import prepareApproval from '@salesforce/apex/QuoteDiscountService.prepareApproval';
        import totalDiscount from '@salesforce/schema/Quote.Total_Discount__c';

        export default class CPQ_CheckApproverLine extends LightningElement {
            @api recordId;
            handleCheck() {
                prepareApproval({ quoteId: this.recordId });
            }
        }
    "#;
    ai_rumin::parsers::scripting::parse_typescript("force-app/main/default/lwc/cPQ_CheckApproverLine/cPQ_CheckApproverLine.js", lwc_code, &mut graph);

    let lwc_file_id = "force-app/main/default/lwc/cPQ_CheckApproverLine/cPQ_CheckApproverLine.js::file";

    // Verify polyglot edge: LWC -> apex::QuoteDiscountService (Calls)
    let callers = graph.neighbors("apex::QuoteDiscountService", petgraph::Direction::Incoming);
    assert!(callers.iter().any(|(n, _)| n.id == lwc_file_id), "LWC must have incoming call edge to apex::QuoteDiscountService");

    // Verify polyglot edge: LWC -> apex::QuoteDiscountService.prepareApproval (Calls)
    let meth_callers = graph.neighbors("apex::QuoteDiscountService.prepareApproval", petgraph::Direction::Incoming);
    assert!(meth_callers.iter().any(|(n, _)| n.id == lwc_file_id), "LWC must have incoming call edge to Apex method");

    // Verify polyglot edge: LWC -> sobject::Quote.Total_Discount__c (References)
    let schema_callers = graph.neighbors("sobject::Quote.Total_Discount__c", petgraph::Direction::Incoming);
    assert!(schema_callers.iter().any(|(n, _)| n.id == lwc_file_id), "LWC must have reference edge to schema column");

    // Verify BRE Decision Matrix invocation from Apex: QuoteDiscountService -> decision_matrix::Product_Discount_Tiering
    let matrix_callers = graph.neighbors("decision_matrix::Product_Discount_Tiering", petgraph::Direction::Incoming);
    assert!(matrix_callers.iter().any(|(n, _)| n.id == "apex::QuoteDiscountService"), "Apex class must call decision_matrix::Product_Discount_Tiering");
}

#[test]
fn test_wish2_filtered_semantic_retrieval_and_hub_damping() {
    let mut graph = CodeGraph::new();

    // Create a high-degree hub node (e.g. standard Account)
    let mut account = CodeEntity::new("sobject::Account", "Account", EntityType::Table, "objects/Account.object-meta.xml", 1, 1);
    account.signature = Some("Table Account".to_string());
    graph.add_entity(account);

    // Add 20 dependent classes to make Account a massive hub
    for i in 0..20 {
        let caller_id = format!("classes/Service{}.cls::Service{}", i, i);
        let mut caller = CodeEntity::new(&caller_id, &format!("Service{}", i), EntityType::Class, format!("classes/Service{}.cls", i), 1, 1);
        caller.signature = Some("class Service".to_string());
        graph.add_entity(caller);
        graph.add_edge(&caller_id, "sobject::Account", EdgeKind::References);
    }

    // Create specialized Custom Metadata type
    let mut cmdt = CodeEntity::new("sobject::CPQ_Business_Case_Line_Approver__mdt", "CPQ_Business_Case_Line_Approver__mdt", EntityType::Table, "force-app/main/default/objects/CPQ_Business_Case_Line_Approver__mdt.object-meta.xml", 1, 1);
    cmdt.signature = Some("CustomMetadata CPQ_Business_Case_Line_Approver__mdt".to_string());
    cmdt.metadata.insert("kind".to_string(), "CustomMetadata".to_string());
    graph.add_entity(cmdt);

    let engine = ai_rumin::hippocampal::HippocampalEngine::default();

    // 1. Query with hub damping enabled
    let budget_damped = ai_rumin::types::ContextBudget {
        total_token_limit: 1500,
        max_symbols: 10,
        include_signatures: true,
        expand_bodies: false,
        types: None,
        path_prefix: None,
        dampen_hubs: true,
    };
    let res_damped = engine.query(&graph, "Account CPQ_Business_Case_Line_Approver__mdt", &budget_damped);
    assert!(!res_damped.top_symbols.is_empty());

    // 2. Query with types filter: only custom_metadata
    let budget_types = ai_rumin::types::ContextBudget {
        total_token_limit: 1500,
        max_symbols: 10,
        include_signatures: true,
        expand_bodies: false,
        types: Some(vec!["custom_metadata".to_string()]),
        path_prefix: None,
        dampen_hubs: false,
    };
    let res_types = engine.query(&graph, "Account CPQ_Business_Case_Line_Approver__mdt", &budget_types);
    assert!(res_types.top_symbols.iter().all(|s| s.id.contains("__mdt")));

    // 3. Query with path prefix filter
    let budget_prefix = ai_rumin::types::ContextBudget {
        total_token_limit: 1500,
        max_symbols: 10,
        include_signatures: true,
        expand_bodies: false,
        types: None,
        path_prefix: Some("force-app/".to_string()),
        dampen_hubs: false,
    };
    let res_prefix = engine.query(&graph, "Account CPQ_Business_Case_Line_Approver__mdt", &budget_prefix);
    assert!(res_prefix.top_symbols.iter().all(|s| s.file_path.starts_with("force-app/")));
}

#[test]
fn test_wish3_declarative_metadata_approval_process_and_decision_matrix() {
    let mut graph = CodeGraph::new();

    let approval_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <ApprovalProcess xmlns="http://soap.sforce.com/2006/04/metadata">
        <label>Quote Business Pricing Approval</label>
        <targetObject>Quote</targetObject>
        <entryCriteria>
            <formula>Quote.Total_Discount__c &gt; 0.20</formula>
        </entryCriteria>
        <approvalStep>
            <name>Step_1_Line_Approver</name>
            <label>Step 1: Line Approver</label>
            <assignedApprover>
                <approver>
                    <name>Approver_Line_1__c</name>
                    <type>relatedUserField</type>
                </approver>
            </assignedApprover>
        </approvalStep>
    </ApprovalProcess>"#;

    ai_rumin::parsers::salesforce::parse_approval_process(
        "force-app/main/default/approvalProcesses/Quote.Business_Pricing_Approval.approvalProcess-meta.xml",
        approval_xml,
        &mut graph
    );

    // Verify ApprovalProcess entity
    let ap_id = "approval_process::Quote.Business_Pricing_Approval";
    let ap_ent = graph.get_entity(ap_id).expect("ApprovalProcess must be indexed");
    assert_eq!(ap_ent.name, "Quote Business Pricing Approval");
    assert_eq!(ap_ent.metadata.get("targetObject").unwrap(), "Quote");

    // Verify edge to Quote SObject
    let out_edges = graph.neighbors(ap_id, petgraph::Direction::Outgoing);
    assert!(out_edges.iter().any(|(n, _)| n.id == "sobject::Quote"));
    assert!(out_edges.iter().any(|(n, _)| n.id == "sobject::Quote.Total_Discount__c"));

    // Verify step and approver field link
    let step_id = format!("{}::step::Step_1_Line_Approver", ap_id);
    let step_ent = graph.get_entity(&step_id).expect("ApprovalStep must exist");
    assert_eq!(step_ent.metadata.get("assignedApprover").unwrap(), "Approver_Line_1__c");

    let step_out = graph.neighbors(&step_id, petgraph::Direction::Outgoing);
    assert!(step_out.iter().any(|(n, _)| n.id == "sobject::Quote.Approver_Line_1__c"));

    // Test Decision Matrix Parser
    let matrix_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <DecisionMatrix xmlns="http://soap.sforce.com/2006/04/metadata">
        <label>Product Discount Tiering</label>
        <columns>
            <name>ProductCode</name>
            <dataType>Text</dataType>
            <columnType>Input</columnType>
        </columns>
        <columns>
            <name>T1</name>
            <dataType>Number</dataType>
            <columnType>Output</columnType>
        </columns>
    </DecisionMatrix>"#;

    ai_rumin::parsers::salesforce::parse_decision_matrix(
        "force-app/main/default/decisionMatrix/Product_Discount_Tiering.decisionMatrix-meta.xml",
        matrix_xml,
        &mut graph
    );

    let dm_id = "decision_matrix::Product_Discount_Tiering";
    let dm_ent = graph.get_entity(dm_id).expect("DecisionMatrix must be indexed");
    assert_eq!(dm_ent.name, "Product Discount Tiering");

    let col_input = graph.get_entity(&format!("{}.ProductCode", dm_id)).expect("Input column must exist");
    assert_eq!(col_input.metadata.get("columnType").unwrap(), "Input");

    let col_output = graph.get_entity(&format!("{}.T1", dm_id)).expect("Output column must exist");
    assert_eq!(col_output.metadata.get("columnType").unwrap(), "Output");
}

#[test]
fn test_wish4_four_quadrant_impact_analysis() {
    let mut graph = CodeGraph::new();

    // 1. Apex Service
    let svc_code = r#"
        public with sharing class QuoteDiscountService {
            public static String prepareApproval(Id quoteId) {
                DiscountCalculator.compute();
                return 'T1';
            }
        }
    "#;
    parse_apex_class("force-app/main/default/classes/QuoteDiscountService.cls", svc_code, &mut graph);

    // 2. Downstream Helper
    let helper_code = r#"
        public with sharing class DiscountCalculator {
            public static void compute() {
                List<Quote> q = [SELECT Id, Total_Discount__c FROM Quote];
            }
        }
    "#;
    parse_apex_class("force-app/main/default/classes/DiscountCalculator.cls", helper_code, &mut graph);
    graph.add_edge("apex::QuoteDiscountService.prepareApproval", "apex::DiscountCalculator.compute", EdgeKind::Calls);

    // 3. LWC UI Component calling Apex
    let lwc_code = r#"
        import { LightningElement } from 'lwc';
        import prepareApproval from '@salesforce/apex/QuoteDiscountService.prepareApproval';
        export default class CPQ_CheckApproverLine extends LightningElement {}
    "#;
    ai_rumin::parsers::scripting::parse_typescript("force-app/main/default/lwc/cPQ_CheckApproverLine/cPQ_CheckApproverLine.js", lwc_code, &mut graph);

    // 4. Apex Unit Test
    let test_code = r#"
        @isTest
        public class QuoteDiscountServiceTest {
            @isTest static void testPrepareApproval() {
                QuoteDiscountService.prepareApproval(null);
            }
        }
    "#;
    parse_apex_class("force-app/main/default/classes/QuoteDiscountServiceTest.cls", test_code, &mut graph);
    graph.add_edge("apex::QuoteDiscountServiceTest.testPrepareApproval", "apex::QuoteDiscountService.prepareApproval", EdgeKind::Calls);

    // Run 4-Quadrant Impact Analysis
    let report = graph.impact_analysis("apex::QuoteDiscountService.prepareApproval", 3);

    // Assert Upstream Callers includes LWC UI
    assert!(report.upstream_callers.iter().any(|c| c.category == "ui_presentation" && c.file_path.contains("cPQ_CheckApproverLine")),
        "Upstream callers must include LWC component: {:?}", report.upstream_callers);

    // Assert Downstream Callees includes DiscountCalculator
    assert!(report.downstream_callees.iter().any(|c| c.name.contains("DiscountCalculator")),
        "Downstream callees must include DiscountCalculator: {:?}", report.downstream_callees);

    // Assert Covering Tests includes QuoteDiscountServiceTest
    assert!(report.covering_tests.iter().any(|t| t.category == "unit_test" && t.name.contains("QuoteDiscountServiceTest")),
        "Covering tests must include QuoteDiscountServiceTest: {:?}", report.covering_tests);

    // Assert Schema Footprint includes Quote
    assert!(report.schema_footprint.iter().any(|s| s.name.contains("Quote")),
        "Schema footprint must include Quote: {:?}", report.schema_footprint);
}



