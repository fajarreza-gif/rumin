use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::Indexer;
use ai_rumin::parsers::*;
use ai_rumin::types::EntityType;

#[test]
fn test_omnistudio_json_path_and_filtering() {
    // 1. DataPack filename conventions
    assert!(is_omnistudio_json_path("IntegrationProcedure/Order_Cancel/Order_Cancel_DataPack.json", "Order_Cancel_DataPack.json"));
    assert!(is_omnistudio_json_path("DataRaptor/DR_GetAccount/DR_GetAccount_Items.json", "DR_GetAccount_Items.json"));
    assert!(is_omnistudio_json_path("IntegrationProcedure/Order_Cancel/Order_Cancel_Element_RemoteCall.json", "Order_Cancel_Element_RemoteCall.json"));
    assert!(is_omnistudio_json_path("Product2/BROADBAND_1G/BROADBAND_1G_DataPack.json", "BROADBAND_1G_DataPack.json"));

    // 2. Folder path conventions
    assert!(is_omnistudio_json_path("vlocity/IntegrationProcedure/Billing_Invoice/config.json", "config.json"));
    assert!(is_omnistudio_json_path("datapacks/DataRaptor/ExtractQuote/mappings.json", "mappings.json"));
    assert!(is_omnistudio_json_path("omnistudio/OmniScript/Account_Update/elements.json", "elements.json"));

    // 3. Exclusions: generic dumps and lockfiles
    assert!(!is_omnistudio_json_path("package-lock.json", "package-lock.json"));
    assert!(!is_omnistudio_json_path("data.json", "data.json"));
    assert!(!is_omnistudio_json_path("payload.json", "payload.json"));

    // 4. Indexer::is_supported_file checks
    assert!(Indexer::is_supported_file("Order_Cancel_DataPack.json", "json"));
    assert!(Indexer::is_supported_file("DR_GetAccount_Items.json", "json"));
    assert!(!Indexer::is_supported_file("payload.json", "json"));
}

#[test]
fn test_integration_procedure_relational_flow_graph() {
    let mut graph = CodeGraph::new();
    let ip_json = r#"{
        "OmniProcessType": "Integration Procedure",
        "%vlocity_namespace%__Type__c": "Order",
        "%vlocity_namespace%__SubType__c": "Cancel",
        "OmniProcessKey": "Order_Cancel",
        "Description": "Cancel customer broadband order and execute billing rollback",
        "%vlocity_namespace%__Element__c": [
            {
                "Name": "ExtractOrderDetails",
                "%vlocity_namespace%__Type__c": "DataRaptor Extract Action",
                "%vlocity_namespace%__Level__c": "0",
                "%vlocity_namespace%__Order__c": "1",
                "PropertySetConfig": {
                    "bundleName": "DR_ExtractOrderAndAccount"
                }
            },
            {
                "Name": "InvokeOrderManagementService",
                "%vlocity_namespace%__Type__c": "Remote Action",
                "%vlocity_namespace%__Level__c": "0",
                "%vlocity_namespace%__Order__c": "2",
                "PropertySetConfig": {
                    "remoteClass": "OrderManagementService",
                    "remoteMethod": "cancelOrder"
                }
            },
            {
                "Name": "CallBillingRollbackSubIP",
                "%vlocity_namespace%__Type__c": "Integration Procedure Action",
                "%vlocity_namespace%__Level__c": "0",
                "%vlocity_namespace%__Order__c": "3",
                "PropertySetConfig": {
                    "integrationProcedureKey": "Billing_RollbackInvoice"
                }
            }
        ]
    }"#;

    parse_omnistudio_json("IntegrationProcedure/Order_Cancel/Order_Cancel_DataPack.json", ip_json, &mut graph);

    // 1. Module entity for the Integration Procedure
    let ip_entity = graph.get_entity("omni_ip::Order_Cancel").expect("IP entity must exist");
    assert_eq!(ip_entity.entity_type, EntityType::Module);
    assert_eq!(ip_entity.metadata.get("type").unwrap(), "Order");
    assert_eq!(ip_entity.metadata.get("subType").unwrap(), "Cancel");

    // 2. Step entities
    assert!(graph.get_entity("omni_ip::Order_Cancel::ExtractOrderDetails").is_some());
    assert!(graph.get_entity("omni_ip::Order_Cancel::InvokeOrderManagementService").is_some());
    assert!(graph.get_entity("omni_ip::Order_Cancel::CallBillingRollbackSubIP").is_some());

    // 3. Relational Flow Graph: Sequencing edges (ExtractOrderDetails -> InvokeOrderManagementService -> CallBillingRollbackSubIP)
    assert!(graph.has_edge("omni_ip::Order_Cancel::ExtractOrderDetails", "omni_ip::Order_Cancel::InvokeOrderManagementService"));
    assert!(graph.has_edge("omni_ip::Order_Cancel::InvokeOrderManagementService", "omni_ip::Order_Cancel::CallBillingRollbackSubIP"));

    // 4. Cross-Language Action Call Edges:
    // Apex class & method invocation
    assert!(graph.has_edge("omni_ip::Order_Cancel::InvokeOrderManagementService", "apex::OrderManagementService"));
    assert!(graph.has_edge("omni_ip::Order_Cancel::InvokeOrderManagementService", "apex::OrderManagementService.cancelOrder"));

    // DataRaptor invocation
    assert!(graph.has_edge("omni_ip::Order_Cancel::ExtractOrderDetails", "omni_dr::DR_ExtractOrderAndAccount"));

    // Sub-IP invocation
    assert!(graph.has_edge("omni_ip::Order_Cancel::CallBillingRollbackSubIP", "omni_ip::Billing_RollbackInvoice"));
}

#[test]
fn test_dataraptor_relational_mappings_and_formula_lineage() {
    let mut graph = CodeGraph::new();
    let dr_json = r#"{
        "Name": "DR_TransformCustomerTier",
        "DataRaptorType": "Transform",
        "Description": "Calculate customer tier and discount based on annual revenue",
        "%vlocity_namespace%__DataRaptorMapItem__c": [
            {
                "Name": "ExtractAccountRevenue",
                "%vlocity_namespace%__InputObjectName__c": "Account",
                "%vlocity_namespace%__InputFieldName__c": "AnnualRevenue",
                "%vlocity_namespace%__OutputObjectName__c": "json",
                "%vlocity_namespace%__OutputFieldName__c": "RawRevenue"
            },
            {
                "Name": "CalculateDiscountFormula",
                "%vlocity_namespace%__FormulaExpression__c": "IF(Account:AnnualRevenue > 1000000, Account:AnnualRevenue * 0.15, Account:AnnualRevenue * 0.05)",
                "%vlocity_namespace%__FormulaResultPath__c": "Account:CalculatedDiscount"
            },
            {
                "Name": "LoadContractDiscount",
                "%vlocity_namespace%__InputObjectName__c": "json",
                "%vlocity_namespace%__OutputObjectName__c": "Contract",
                "%vlocity_namespace%__OutputFieldName__c": "DiscountAmount__c"
            }
        ]
    }"#;

    parse_omnistudio_json("DataRaptor/DR_TransformCustomerTier/DR_TransformCustomerTier_Items.json", dr_json, &mut graph);

    // 1. DataRaptor Module entity
    let dr_entity = graph.get_entity("omni_dr::DR_TransformCustomerTier").expect("DR entity must exist");
    assert_eq!(dr_entity.entity_type, EntityType::Module);

    // 2. SObject nodes extracted from mappings
    assert!(graph.get_entity("sobject::Account").is_some());
    assert!(graph.get_entity("sobject::Account.AnnualRevenue").is_some());
    assert!(graph.get_entity("sobject::Contract").is_some());

    // 3. Formula node-level lineage entity
    let formula_id = "omni_dr::DR_TransformCustomerTier::formula::Account:CalculatedDiscount";
    let formula_entity = graph.get_entity(formula_id).expect("Formula entity must exist");
    assert_eq!(formula_entity.entity_type, EntityType::Function);
    assert!(formula_entity.signature.as_ref().unwrap().contains("IF(Account:AnnualRevenue"));

    // 4. Node-level data lineage edges:
    // Input field (sobject::Account.AnnualRevenue) feeds Formula node
    assert!(graph.has_edge("sobject::Account.AnnualRevenue", formula_id));

    // Formula node defines/references target SObject field (sobject::Account.CalculatedDiscount)
    assert!(graph.has_edge(formula_id, "sobject::Account.CalculatedDiscount"));
}

#[test]
fn test_epc_product_catalog_and_json_attribute_overrides() {
    let mut graph = CodeGraph::new();
    let epc_json = r#"{
        "ProductCode": "BROADBAND_FIBER_1G",
        "Name": "Broadband Fiber 1Gbps Plan",
        "Description": "High speed gigabit residential fiber connectivity",
        "pricingMatrix": "BroadbandPricingMatrix",
        "%vlocity_namespace%__ProductChildItem__c": [
            {
                "ChildProductCode": "WIFI_6_ROUTER",
                "Name": "Wi-Fi 6 Router Hardware"
            },
            {
                "ChildProductCode": "STATIC_IP_ADDON",
                "Name": "Static IP Address"
            }
        ],
        "vlocity_cmt__JSONAttribute__c": [
            {
                "categorycode__c": "NETWORK_SPECS",
                "attributeuniquecode__c": "ATTR_DOWNLOAD_SPEED",
                "value__c": "1000 Mbps"
            },
            {
                "categorycode__c": "NETWORK_SPECS",
                "attributeuniquecode__c": "ATTR_UPLOAD_SPEED",
                "value__c": "500 Mbps"
            },
            {
                "categorycode__c": "BILLING_OPTIONS",
                "attributeuniquecode__c": "ATTR_CONTRACT_TERM",
                "value__c": "24 Months"
            }
        ]
    }"#;

    parse_omnistudio_json("Product2/BROADBAND_FIBER_1G/BROADBAND_FIBER_1G_DataPack.json", epc_json, &mut graph);

    // 1. EPC Product Entity
    let prod_id = "epc::Product::BROADBAND_FIBER_1G";
    let prod_entity = graph.get_entity(prod_id).expect("EPC Product must exist");
    assert_eq!(prod_entity.name, "Broadband Fiber 1Gbps Plan");
    assert_eq!(prod_entity.entity_type, EntityType::Class);

    // 2. ProductChildItem Composition Edges
    assert!(graph.has_edge(prod_id, "epc::Product::WIFI_6_ROUTER"));
    assert!(graph.has_edge(prod_id, "epc::Product::STATIC_IP_ADDON"));

    // 3. Pricing Matrix Reference
    assert!(graph.has_edge(prod_id, "decision_matrix::BroadbandPricingMatrix"));

    // 4. Attribute Entities & Overrides extracted from vlocity_cmt__JSONAttribute__c
    assert!(graph.get_entity("epc::Attribute::ATTR_DOWNLOAD_SPEED").is_some());
    assert!(graph.get_entity("epc::Attribute::ATTR_UPLOAD_SPEED").is_some());
    assert!(graph.get_entity("epc::Attribute::ATTR_CONTRACT_TERM").is_some());

    assert!(graph.has_edge(prod_id, "epc::Attribute::ATTR_DOWNLOAD_SPEED"));
    assert!(graph.has_edge(prod_id, "epc::Attribute::ATTR_UPLOAD_SPEED"));
}

#[test]
fn test_omniscript_lwc_and_remote_actions() {
    let mut graph = CodeGraph::new();
    let os_json = r#"{
        "OmniProcessType": "OmniScript",
        "Type": "CustomerSelfService",
        "SubType": "ChangePlan",
        "elements": [
            {
                "Name": "SelectAddressStep",
                "Type": "Custom Lightning Web Component",
                "PropertySetConfig": {
                    "customLwcName": "customerAddressSelector"
                }
            },
            {
                "Name": "ExecutePlanChangeAction",
                "Type": "Integration Procedure Action",
                "PropertySetConfig": {
                    "integrationProcedureKey": "Plan_ChangeExecution"
                }
            }
        ]
    }"#;

    parse_omnistudio_json("OmniScript/CustomerSelfService_ChangePlan/CustomerSelfService_ChangePlan_DataPack.json", os_json, &mut graph);

    // 1. OmniScript module
    let os_id = "omni_os::CustomerSelfService_ChangePlan";
    assert!(graph.get_entity(os_id).is_some());

    // 2. Custom LWC call edge
    assert!(graph.has_edge(&format!("{}::SelectAddressStep", os_id), "lwc::customerAddressSelector"));

    // 3. IP Action call edge
    assert!(graph.has_edge(&format!("{}::ExecutePlanChangeAction", os_id), "omni_ip::Plan_ChangeExecution"));
}

#[tokio::test]
async fn test_end_to_end_indexer_with_omnistudio_json() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_omni_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let ip_path = temp_dir.join("Cart_Checkout_DataPack.json");
    let ip_json = r#"{
        "OmniProcessType": "Integration Procedure",
        "%vlocity_namespace%__Type__c": "Cart",
        "%vlocity_namespace%__SubType__c": "Checkout",
        "OmniProcessKey": "Cart_Checkout",
        "%vlocity_namespace%__Element__c": [
            {
                "Name": "ValidateCartApex",
                "Type": "Remote Action",
                "PropertySetConfig": {
                    "remoteClass": "CartValidationController",
                    "remoteMethod": "validateCart"
                }
            }
        ]
    }"#;
    std::fs::write(&ip_path, ip_json).unwrap();

    let mut graph = CodeGraph::new();
    let sidecar = ai_rumin::sidecar::SidecarClient::new("dummy.py");

    let res = Indexer::index_single_file(&temp_dir, &ip_path, &mut graph, &sidecar).await.unwrap();
    assert!(res, "Cart_Checkout_DataPack.json must be indexed");
    assert!(graph.get_entity("omni_ip::Cart_Checkout").is_some());
    assert!(graph.get_entity("omni_ip::Cart_Checkout::ValidateCartApex").is_some());

    assert!(graph.has_edge("omni_ip::Cart_Checkout::ValidateCartApex", "apex::CartValidationController"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_empirical_salesforce_source_code_from_sf_b2b() {
    let sf_root = std::path::Path::new("/home/wally/dev/SF-B2B");
    if !sf_root.exists() {
        return; // Skip if run in environments without local SF-B2B repo
    }

    let mut graph = CodeGraph::new();

    // 1. Real OmniIntegrationProcedure XML from SF-B2B
    let oip_rel = "force-app/main/default/omniIntegrationProcedures/ActivateFrameAgreement_Order_Procedure_1.oip-meta.xml";
    let oip_path = sf_root.join(oip_rel);
    if oip_path.exists() {
        let content = std::fs::read_to_string(&oip_path).unwrap();
        parse_omnistudio_metadata(oip_rel, &content, &mut graph);

        assert!(graph.get_entity("omni_ip::Activate Frame Agreement").is_some());
        let step_id = "omni_ip::Activate Frame Agreement::element::ActivateSourceDiscounts";
        assert!(graph.get_entity(step_id).is_some());
        assert!(graph.has_edge(step_id, "apex::ActivateFrameContract"));
        assert!(graph.has_edge(step_id, "apex::ActivateFrameContract.activateSourceDiscounts"));
    }

    // 2. Real OmniDataTransform XML with Formulas from SF-B2B
    let rpt_rel = "force-app/main/default/omniDataTransforms/CPQDMServicabilityCheckFTTHGetProductsInput_1.rpt-meta.xml";
    let rpt_path = sf_root.join(rpt_rel);
    if rpt_path.exists() {
        let content = std::fs::read_to_string(&rpt_path).unwrap();
        parse_omnistudio_metadata(rpt_rel, &content, &mut graph);

        let dr_id = "omni_dr::CPQDMServicabilityCheckFTTHGetProductsInput";
        assert!(graph.get_entity(dr_id).is_some());

        // Node-level formula lineage
        let formula_id = format!("{}::formula::QuoteLineItem:fullAddress", dr_id);
        let formula_ent = graph.get_entity(&formula_id).expect("Formula entity must exist");
        assert_eq!(formula_ent.entity_type, EntityType::Function);
        assert!(formula_ent.signature.as_ref().unwrap().contains("IF("));

        // Field lineage edge
        assert!(graph.has_edge("sobject::QuoteLineItem.vlocity_cmt__QuoteMemberId__r", &formula_id)
            || graph.has_edge("sobject::QuoteLineItem", &formula_id));
    }

    // 3. Real Apex Class from SF-B2B
    let cls_rel = "force-app/main/default/classes/AccountStatusActivationServiceTest.cls";
    let cls_path = sf_root.join(cls_rel);
    if cls_path.exists() {
        let content = std::fs::read_to_string(&cls_path).unwrap();
        parse_apex_class(cls_rel, &content, &mut graph);

        assert!(graph.get_entity("force-app/main/default/classes/AccountStatusActivationServiceTest.cls::AccountStatusActivationServiceTest").is_some());
    }

    // 4. Real Custom SObject metadata from SF-B2B
    let obj_rel = "force-app/main/default/objects/Account/Account.object-meta.xml";
    let obj_path = sf_root.join(obj_rel);
    if obj_path.exists() {
        let content = std::fs::read_to_string(&obj_path).unwrap();
        parse_salesforce_metadata(obj_rel, &content, &mut graph);

        assert!(graph.get_entity("sobject::Account").is_some());
    }
}
