use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::Indexer;
use ai_rumin::parsers::jvm_xml::parse_plsql;
use ai_rumin::parsers::salesforce::{parse_apex_class, parse_salesforce_metadata};
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{CodeEntity, EdgeKind, EntityType};
use std::fs;

#[test]
fn test_dynamic_soql_progressive_resolution_tiers() {
    let mut graph = CodeGraph::new();

    let apex_code = r#"
public class SFA_CreateUpdateAccountContact {
    public static void createAccount() {
        String sobjectName = 'Account';
        String queryAccount = 'select parentid, recordtype.name from ' + sobjectName + ' where Id = :objectId';
        List<Account> accountRecords = Database.query(queryAccount);

        String queryAccountContactRelationship = 'select id, SFA_Primary__c, accountId from ' + 'AccountContactRelation' + ' where accountid = :objectId';
        List<AccountContactRelation> contactRecords = Database.query(queryAccountContactRelationship);

        Schema.SObjectType.Contact.fields.getMap();
        Opportunity.SObjectType.getDescribe();
    }
}
"#;

    parse_apex_class(
        "force-app/main/default/classes/SFA_CreateUpdateAccountContact.cls",
        apex_code,
        &mut graph,
    );

    // Tier 2 verification: Local variable constant folding ('... from ' + sobjectName where sobjectName = 'Account')
    let acc_ent = graph.get_entity("sobject::Account").expect("sobject::Account entity must exist");
    assert_eq!(acc_ent.metadata.get("isDynamicTarget").map(|s| s.as_str()), Some("true"));

    let class_id = "force-app/main/default/classes/SFA_CreateUpdateAccountContact.cls::SFA_CreateUpdateAccountContact";
    assert!(graph.has_edge(class_id, "sobject::Account"), "Class must reference dynamically resolved sobject::Account");

    // Field-level extraction from dynamic SOQL
    assert!(graph.get_entity("sobject::Account.parentid").is_some(), "Account.parentid must be extracted");
    assert!(graph.has_edge(class_id, "sobject::Account.parentid"), "Class must reference Account.parentid");

    // Tier 1 verification: Dynamic string concatenation ('... from ' + 'AccountContactRelation')
    let acr_ent = graph.get_entity("sobject::AccountContactRelation").expect("sobject::AccountContactRelation must exist");
    assert_eq!(acr_ent.metadata.get("isDynamicTarget").map(|s| s.as_str()), Some("true"));
    assert!(graph.has_edge(class_id, "sobject::AccountContactRelation"));
    assert!(graph.get_entity("sobject::AccountContactRelation.SFA_Primary__c").is_some(), "SFA_Primary__c must be extracted");
    assert!(graph.has_edge(class_id, "sobject::AccountContactRelation.SFA_Primary__c"));

    // Tier 3 verification: Schema Token describe references
    assert!(graph.get_entity("sobject::Contact").is_some(), "Contact SObject must be linked via Schema.SObjectType token");
    assert!(graph.has_edge(class_id, "sobject::Contact"));

    assert!(graph.get_entity("sobject::Opportunity").is_some(), "Opportunity SObject must be linked via SObjectType token");
    assert!(graph.has_edge(class_id, "sobject::Opportunity"));
}

#[test]
fn test_cross_database_semantic_bridges_layer_a_external_id_mining() {
    let mut graph = CodeGraph::new();

    // 1. Oracle Siebel DDL Table
    let ddl_content = r#"
CREATE TABLE SIEBEL.S_ORG_EXT (
    ROW_ID VARCHAR2(15) PRIMARY KEY,
    NAME VARCHAR2(100)
);
"#;
    parse_plsql("catalist/ddl/siebel.sql", ddl_content, &mut graph);

    // 2. Salesforce Account external ID field metadata
    let xml_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomField xmlns="http://soap.sforce.com/2006/04/metadata">
    <fullName>SFA_Catalyst_Account_Number__c</fullName>
    <externalId>true</externalId>
    <type>Text</type>
    <length>30</length>
</CustomField>
"#;
    parse_salesforce_metadata(
        "force-app/main/default/objects/Account/fields/SFA_Catalyst_Account_Number__c.field-meta.xml",
        xml_content,
        &mut graph,
    );

    // Verify entity existence
    assert!(graph.get_entity("sobject::Account").is_some());
    assert!(graph.get_entity("SIEBEL.S_ORG_EXT").is_some());
    assert!(graph.get_entity("sobject::Account.SFA_Catalyst_Account_Number__c").is_some());

    // Verify bidirectional SemanticBridge edges
    assert!(
        graph.has_edge("sobject::Account", "SIEBEL.S_ORG_EXT"),
        "sobject::Account must bridge to SIEBEL.S_ORG_EXT"
    );
    assert!(
        graph.has_edge("SIEBEL.S_ORG_EXT", "sobject::Account"),
        "SIEBEL.S_ORG_EXT must bridge back to sobject::Account"
    );
    assert!(
        graph.has_edge("sobject::Account.SFA_Catalyst_Account_Number__c", "SIEBEL.S_ORG_EXT"),
        "External ID column must bridge to SIEBEL.S_ORG_EXT"
    );

    // Verify cross-database pathfinding
    let paths = graph.find_paths("SIEBEL.S_ORG_EXT", "sobject::Account", 3);
    assert!(!paths.is_empty(), "rumin_find_path must find path between SIEBEL.S_ORG_EXT and sobject::Account");
    assert_eq!(paths[0].hops, 1);
    assert!(matches!(paths[0].steps[0].edge, EdgeKind::SemanticBridge));
}

#[test]
fn test_cross_database_semantic_bridges_layer_b_declarative_json_and_csv() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_bridge_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let rumin_dir = temp_dir.join(".rumin");
    fs::create_dir_all(&rumin_dir).unwrap();

    // 1. Write .rumin/bridges.json
    let bridges_json = r#"{
  "semantic_bridges": [
    { "source": "SIEBEL.S_ORG_EXT", "target": "sobject::Account", "type": "ETL_MAPPING" },
    { "source": "SIEBEL.S_PROD_INT", "target": "sobject::Product2", "type": "ETL_MAPPING" },
    { "source": "SIEBEL.S_ASSET", "target": "sobject::Asset", "type": "ETL_MAPPING" },
    { "source": "SIEBEL.S_DOC_QUOTE", "target": "sobject::Quote", "type": "ETL_MAPPING" },
    { "source": "SIEBEL.S_ORDER", "target": "sobject::Order", "type": "ETL_MAPPING" }
  ]
}"#;
    fs::write(rumin_dir.join("bridges.json"), bridges_json).unwrap();

    // 2. Write .rumin/bridges.csv
    let bridges_csv = "SOURCE,TARGET,TYPE\nSIEBEL.S_CONTACT,sobject::Contact,ETL_MAPPING\nSIEBEL.S_SRV_REQ,sobject::Case,ETL_MAPPING\n";
    fs::write(rumin_dir.join("bridges.csv"), bridges_csv).unwrap();

    let mut graph = CodeGraph::new();

    // Add an SObject-to-SObject dependency edge: sobject::Order depends on sobject::Account
    let acc_ent = CodeEntity::new("sobject::Account", "Account", EntityType::Table, "schema", 1, 1);
    let ord_ent = CodeEntity::new("sobject::Order", "Order", EntityType::Table, "schema", 1, 1);
    graph.add_entity(acc_ent);
    graph.add_entity(ord_ent);
    graph.add_edge("sobject::Order", "sobject::Account", EdgeKind::DependsOn);

    // Ingest bridges
    let bridge_count = Indexer::load_semantic_bridges(&temp_dir, &mut graph);
    assert_eq!(bridge_count, 7, "Must load 5 JSON bridges and 2 CSV bridges");

    // Pathfinding 1: Direct cross-database path SIEBEL.S_ORG_EXT -> sobject::Account
    let paths_direct = graph.find_paths("SIEBEL.S_ORG_EXT", "sobject::Account", 3);
    assert!(!paths_direct.is_empty(), "Direct bridge path must exist");
    assert_eq!(paths_direct[0].hops, 1);
    assert!(matches!(paths_direct[0].steps[0].edge, EdgeKind::SemanticBridge));

    // Pathfinding 2: Reverse traversal sobject::Account -> SIEBEL.S_ORG_EXT
    let paths_rev = graph.find_paths("sobject::Account", "SIEBEL.S_ORG_EXT", 3);
    assert!(!paths_rev.is_empty(), "Reverse bridge path must exist");
    assert_eq!(paths_rev[0].hops, 1);

    // Pathfinding 3: Multi-hop path SIEBEL.S_ORDER -> sobject::Order -> sobject::Account
    let paths_multi = graph.find_paths("SIEBEL.S_ORDER", "sobject::Account", 3);
    assert!(!paths_multi.is_empty(), "Multi-hop path across legacy DDL and Salesforce universe must resolve");
    assert_eq!(paths_multi[0].hops, 2);
    assert_eq!(paths_multi[0].steps[0].to.id, "sobject::Order");
    assert_eq!(paths_multi[0].steps[1].to.id, "sobject::Account");

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_end_to_end_indexing_with_bridges_and_dynamic_soql() {
    let temp_repo = std::env::temp_dir().join(format!("rumin_e2e_bridges_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(temp_repo.join(".rumin")).unwrap();
    fs::create_dir_all(temp_repo.join("catalist/ddl")).unwrap();
    fs::create_dir_all(temp_repo.join("force-app/main/default/classes")).unwrap();
    fs::create_dir_all(temp_repo.join("force-app/main/default/objects/Account/fields")).unwrap();

    // 1. Declarative bridge
    let bridges_json = r#"{
  "semantic_bridges": [
    { "source": "SIEBEL.S_ORG_EXT", "target": "sobject::Account", "type": "ETL_MAPPING" }
  ]
}"#;
    fs::write(temp_repo.join(".rumin/bridges.json"), bridges_json).unwrap();

    // 2. DDL Table
    let ddl = "CREATE TABLE SIEBEL.S_ORG_EXT ( ROW_ID VARCHAR2(15) PRIMARY KEY, NAME VARCHAR2(100) );\n";
    fs::write(temp_repo.join("catalist/ddl/siebel.sql"), ddl).unwrap();

    // 3. Salesforce External ID metadata
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomField xmlns="http://soap.sforce.com/2006/04/metadata">
    <fullName>SFA_Catalyst_Account_Number__c</fullName>
    <externalId>true</externalId>
    <type>Text</type>
</CustomField>
"#;
    fs::write(
        temp_repo.join("force-app/main/default/objects/Account/fields/SFA_Catalyst_Account_Number__c.field-meta.xml"),
        xml,
    ).unwrap();

    // 4. Apex Class with dynamic SOQL
    let apex = r#"
public class SFA_AccountSyncHandler {
    public static void execute() {
        String targetObject = 'Account';
        String q = 'SELECT Id, Name FROM ' + targetObject;
        Database.query(q);
    }
}
"#;
    fs::write(
        temp_repo.join("force-app/main/default/classes/SFA_AccountSyncHandler.cls"),
        apex,
    ).unwrap();

    let indexer = Indexer::new(&temp_repo);
    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new("http://127.0.0.1:0");

    let stats = indexer.sync_repository(&mut graph, &sidecar, true).await.unwrap();
    assert!(stats.semantic_bridges.unwrap_or(0) >= 1, "Semantic bridges must be reported in sync stats");

    // Cross-system discovery: Find path from legacy Siebel table to the Apex sync handler!
    // SIEBEL.S_ORG_EXT -> sobject::Account -> SFA_AccountSyncHandler
    let paths = graph.find_paths("SIEBEL.S_ORG_EXT", "sobject::Account", 3);
    assert!(!paths.is_empty(), "Path from SIEBEL.S_ORG_EXT to sobject::Account must resolve");

    let class_id = "force-app/main/default/classes/SFA_AccountSyncHandler.cls::SFA_AccountSyncHandler";
    assert!(graph.has_edge(class_id, "sobject::Account"), "Apex class must link to dynamic Account SObject");

    // Cleanup
    let _ = fs::remove_dir_all(&temp_repo);
}

#[test]
fn test_dynamic_callable_and_invoke_method_dispatch() {
    let mut graph = CodeGraph::new();

    // 1. Target class declaring methods: buildAccount and calculatePrice
    let target_cls = r#"
public class CPQBillingBAMapperV2 implements Callable {
    public static BillingAccount buildAccount(Map<String, Object> input) {
        return new BillingAccount();
    }

    public static void calculatePrice() {}
}
"#;
    parse_apex_class(
        "force-app/main/default/classes/CPQBillingBAMapperV2.cls",
        target_cls,
        &mut graph,
    );

    // Verify target method exists
    assert!(graph.get_entity("apex::CPQBillingBAMapperV2.buildAccount").is_some());

    // 2. Caller class using dynamic Callable & invokeMethod patterns
    let caller_cls = r#"
public class CPQBillingAccountPayload {
    public static Map<String, Object> toTmfBody(Map<String, Object> inputMap) {
        // Multi-line chained invocation with uppercase 'BuildAccount' (case-insensitive resolution)
        new CPQBillingBAMapperV2()
            .invokeMethod('BuildAccount', inputMap, outputMap, null);

        // Same-line invocation
        new CPQBillingBAMapperV2().invokeMethod('calculatePrice', inputMap, outputMap, null);

        // Variable-based invocation
        PricingPlanHelper priceHelper = new PricingPlanHelper();
        priceHelper.invokeMethod('GetCalculationProcedurePrice', inputMap, outputMap, null);

        // Constant string parameter
        String actionName = 'buildAccount';
        CPQBillingBAMapperV2 mapper = new CPQBillingBAMapperV2();
        mapper.invokeMethod(actionName, inputMap, outputMap, null);

        // Type.forName dynamic resolution
        Type.forName('TMFUtils');
    }
}
"#;
    parse_apex_class(
        "force-app/main/default/classes/CPQBillingAccountPayload.cls",
        caller_cls,
        &mut graph,
    );

    let caller_meth = "apex::CPQBillingAccountPayload.toTmfBody";
    let caller_file_meth = "force-app/main/default/classes/CPQBillingAccountPayload.cls::CPQBillingAccountPayload.toTmfBody";

    // Multi-line chained invocation with case-insensitive resolution to buildAccount
    assert!(
        graph.has_edge(caller_meth, "apex::CPQBillingBAMapperV2.buildAccount"),
        "Multi-line invokeMethod('BuildAccount') must link to apex::CPQBillingBAMapperV2.buildAccount"
    );
    assert!(
        graph.has_edge(caller_file_meth, "apex::CPQBillingBAMapperV2.buildAccount")
    );

    // Same-line invocation to calculatePrice
    assert!(
        graph.has_edge(caller_meth, "apex::CPQBillingBAMapperV2.calculatePrice"),
        "Same-line invokeMethod('calculatePrice') must link to apex::CPQBillingBAMapperV2.calculatePrice"
    );

    // Variable-based invocation to PricingPlanHelper
    assert!(
        graph.has_edge(caller_meth, "apex::PricingPlanHelper.GetCalculationProcedurePrice"),
        "Variable-based priceHelper.invokeMethod must link to PricingPlanHelper"
    );

    // Type.forName reference
    assert!(
        graph.has_edge("apex::CPQBillingAccountPayload", "apex::TMFUtils"),
        "Type.forName('TMFUtils') must create a reference edge"
    );
}
