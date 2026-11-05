use ai_rumin::graph::CodeGraph;
use ai_rumin::parsers::oracle_soa::{is_oracle_soa_file, parse_oracle_soa};
use ai_rumin::parsers::telecom::parse_apigee;
use ai_rumin::parsers::jvm_xml::parse_plsql;
use ai_rumin::types::EntityType;
use std::path::Path;

#[test]
fn test_osb_proxy_pipeline_and_java_callout() {
    let mut graph = CodeGraph::new();

    // 1. OSB Proxy Service
    let proxy_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml-fragment xmlns:ser="http://www.bea.com/wli/sb/services" xmlns:tran="http://www.bea.com/wli/sb/transports" xmlns:env="http://www.bea.com/wli/config/env" xmlns:con="http://www.bea.com/wli/sb/pipeline/config">
    <ser:coreEntry isProxy="true" isEnabled="true">
        <ser:binding type="SOAP" isSoap="true">
            <con:wsdl ref="Common/WSDL/BillingService"/>
        </ser:binding>
        <ser:target>
            <con:pipeline ref="Billing/Pipelines/BillingService_Pipeline" xsi:type="con:PipelineRef" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"/>
        </ser:target>
    </ser:coreEntry>
    <ser:endpointConfig>
        <tran:provider-id>http</tran:provider-id>
        <tran:URI>
            <env:value>/cxf/services/BillingService</env:value>
        </tran:URI>
    </ser:endpointConfig>
</xml-fragment>"#;

    assert!(is_oracle_soa_file(Path::new("Billing/ProxyServices/BillingService_PS.proxy"), proxy_xml));
    parse_oracle_soa("Billing/ProxyServices/BillingService_PS.proxy", proxy_xml, &mut graph);

    let proxy_id = "osb_proxy::BillingService_PS";
    let proxy_ent = graph.get_entity(proxy_id).expect("Proxy service entity must exist");
    assert_eq!(proxy_ent.entity_type, EntityType::Interface);
    assert_eq!(proxy_ent.metadata.get("endpointUri").map(|s| s.as_str()), Some("/cxf/services/BillingService"));

    // Verify linkage to Pipeline and WSDL
    assert!(graph.has_edge(proxy_id, "osb_pipeline::BillingService_Pipeline"));
    assert!(graph.has_edge(proxy_id, "wsdl::BillingService"));

    // 2. OSB Pipeline with Routing, Transform, and Java Callout
    let pipeline_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<con:pipelineEntry xmlns:con="http://www.bea.com/wli/sb/pipeline/config" xmlns:con1="http://www.bea.com/wli/sb/stages/transform/config" xmlns:con2="http://www.bea.com/wli/sb/stages/routing/config" name="BillingService_Pipeline">
    <con:pipeline type="request" name="request-pipeline">
        <con:stage id="stage-token" name="GenerateAuthToken">
            <con1:javaCallout>
                <con1:archive ref="Common/JARs/SecurityUtil"/>
                <con1:className>com.ioh.security.TokenUtil</con1:className>
                <con1:method>public static String generateToken(String clientId)</con1:method>
            </con1:javaCallout>
            <con1:xslt ref="Billing/Transforms/NormalizeBillingRequest"/>
        </con:stage>
        <con:stage id="stage-route" name="RouteToBackend">
            <con2:route>
                <con2:service ref="Billing/BusinessServices/GenevaBilling_BS" xsi:type="ref:BusinessServiceRef" xmlns:ref="http://www.bea.com/wli/sb/reference" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"/>
            </con2:route>
        </con:stage>
    </con:pipeline>
</con:pipelineEntry>"#;

    parse_oracle_soa("Billing/Pipelines/BillingService_Pipeline.pipeline", pipeline_xml, &mut graph);

    let pipe_id = "osb_pipeline::BillingService_Pipeline";
    assert!(graph.get_entity(pipe_id).is_some());

    // Routing edge to Business Service
    assert!(graph.has_edge(pipe_id, "osb_bix::GenevaBilling_BS"));

    // Java callout edges (class and method)
    assert!(graph.has_edge(pipe_id, "java::com.ioh.security.TokenUtil"));
    assert!(graph.has_edge(pipe_id, "java::com.ioh.security.TokenUtil.generateToken"));

    // Transform edge
    assert!(graph.has_edge(pipe_id, "transform::NormalizeBillingRequest"));
}

#[test]
fn test_osb_business_service_and_jca_db_adapter() {
    let mut graph = CodeGraph::new();

    let bix_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml-fragment xmlns:ser="http://www.bea.com/wli/sb/services" xmlns:tran="http://www.bea.com/wli/sb/transports" xmlns:env="http://www.bea.com/wli/config/env" xmlns:jca="http://www.bea.com/wli/sb/transports/jca">
    <ser:coreEntry isProxy="false" isEnabled="true">
        <ser:binding type="SOAP" isSoap="true">
            <con:wsdl ref="Billing/WSDL/CreateAccountDBAdapter" xmlns:con="http://www.bea.com/wli/sb/pipeline/config"/>
        </ser:binding>
    </ser:coreEntry>
    <ser:endpointConfig>
        <tran:provider-id>jca</tran:provider-id>
        <tran:URI>
            <env:value>jca://eis/DB/BillingPool</env:value>
        </tran:URI>
        <tran:provider-specific>
            <jca:operation-name>BILLING_PKG.CREATE_ACCOUNT</jca:operation-name>
        </tran:provider-specific>
    </ser:endpointConfig>
</xml-fragment>"#;

    assert!(is_oracle_soa_file(Path::new("Billing/BusinessServices/CreateAccount_BS.bix"), bix_xml));
    parse_oracle_soa("Billing/BusinessServices/CreateAccount_BS.bix", bix_xml, &mut graph);

    let bs_id = "osb_bix::CreateAccount_BS";
    let bs_ent = graph.get_entity(bs_id).expect("Business service entity must exist");
    assert_eq!(bs_ent.entity_type, EntityType::Module);

    // Link to WSDL and PL/SQL Stored Procedure
    assert!(graph.has_edge(bs_id, "wsdl::CreateAccountDBAdapter"));
    assert!(graph.has_edge(bs_id, "plsql::BILLING_PKG.CREATE_ACCOUNT"));
}

#[test]
fn test_wsdl_and_xsd_contracts() {
    let mut graph = CodeGraph::new();

    // 1. WSDL Definition
    let wsdl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<wsdl:definitions name="OrderManagementService" targetNamespace="http://ioh.co.id/orders"
    xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/" xmlns:tns="http://ioh.co.id/orders">
    <wsdl:portType name="OrderPortType">
        <wsdl:operation name="submitOrder">
            <wsdl:input message="tns:SubmitOrderRequest"/>
            <wsdl:output message="tns:SubmitOrderResponse"/>
        </wsdl:operation>
        <wsdl:operation name="cancelOrder">
            <wsdl:input message="tns:CancelOrderRequest"/>
            <wsdl:output message="tns:CancelOrderResponse"/>
        </wsdl:operation>
    </wsdl:portType>
</wsdl:definitions>"#;

    assert!(is_oracle_soa_file(Path::new("Orders/WSDL/OrderManagementService.wsdl"), wsdl_xml));
    parse_oracle_soa("Orders/WSDL/OrderManagementService.wsdl", wsdl_xml, &mut graph);

    let wsdl_id = "wsdl::OrderManagementService";
    assert!(graph.get_entity(wsdl_id).is_some());

    // Verify operations extracted as methods
    assert!(graph.has_edge(wsdl_id, "wsdl::OrderManagementService::submitOrder"));
    assert!(graph.has_edge(wsdl_id, "wsdl::OrderManagementService::cancelOrder"));

    // Verify input/output messages linked to XSD schemas
    assert!(graph.has_edge(wsdl_id, "xsd::SubmitOrderRequest"));
    assert!(graph.has_edge(wsdl_id, "xsd::SubmitOrderResponse"));

    // 2. XSD Schema
    let xsd_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema targetNamespace="http://ioh.co.id/orders/types" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
    <xsd:complexType name="SubmitOrderRequest">
        <xsd:sequence>
            <xsd:element name="orderId" type="xsd:string"/>
            <xsd:element name="accountNumber" type="xsd:string"/>
        </xsd:sequence>
    </xsd:complexType>
    <xsd:complexType name="SubmitOrderResponse">
        <xsd:sequence>
            <xsd:element name="status" type="xsd:string"/>
        </xsd:sequence>
    </xsd:complexType>
</xsd:schema>"#;

    parse_oracle_soa("Orders/Schemas/OrderTypes.xsd", xsd_xml, &mut graph);

    let xsd_id = "xsd::types";
    assert!(graph.has_edge(xsd_id, "xsd::types::SubmitOrderRequest"));
    assert!(graph.has_edge(xsd_id, "xsd::types::SubmitOrderResponse"));
}

#[test]
fn test_end_to_end_apigee_to_osb_to_plsql_topological_chain() {
    let mut graph = CodeGraph::new();

    // 1. Apigee TargetEndpoint calling OSB Proxy
    let apigee_target_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<TargetEndpoint name="BillingServiceTarget">
    <HTTPTargetConnection>
        <URL>https://osb-gateway.internal/cxf/services/BillingService</URL>
    </HTTPTargetConnection>
</TargetEndpoint>"#;
    parse_apigee("apiproxy/targets/BillingServiceTarget.xml", apigee_target_xml, &mut graph);

    // 2. OSB Proxy Service
    let osb_proxy_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml-fragment xmlns:ser="http://www.bea.com/wli/sb/services" xmlns:tran="http://www.bea.com/wli/sb/transports" xmlns:env="http://www.bea.com/wli/config/env" xmlns:con="http://www.bea.com/wli/sb/pipeline/config">
    <ser:coreEntry isProxy="true">
        <ser:target>
            <con:pipeline ref="Billing/Billing_Pipeline"/>
        </ser:target>
    </ser:coreEntry>
    <ser:endpointConfig>
        <tran:URI><env:value>/cxf/services/BillingService</env:value></tran:URI>
    </ser:endpointConfig>
</xml-fragment>"#;
    parse_oracle_soa("Billing/Billing_PS.proxy", osb_proxy_xml, &mut graph);

    // 3. OSB Pipeline routing to Business Service
    let osb_pipeline_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<con:pipelineEntry xmlns:con="http://www.bea.com/wli/sb/pipeline/config" xmlns:con2="http://www.bea.com/wli/sb/stages/routing/config" name="Billing_Pipeline">
    <con:pipeline type="request">
        <con2:route>
            <con2:service ref="Billing/BillingDB_BS" xsi:type="ref:BusinessServiceRef" xmlns:ref="http://www.bea.com/wli/sb/reference" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"/>
        </con2:route>
    </con:pipeline>
</con:pipelineEntry>"#;
    parse_oracle_soa("Billing/Billing_Pipeline.pipeline", osb_pipeline_xml, &mut graph);

    // 4. OSB Business Service invoking DB Adapter
    let osb_bix_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml-fragment xmlns:ser="http://www.bea.com/wli/sb/services" xmlns:tran="http://www.bea.com/wli/sb/transports" xmlns:jca="http://www.bea.com/wli/sb/transports/jca">
    <ser:endpointConfig>
        <tran:provider-id>jca</tran:provider-id>
        <tran:provider-specific>
            <jca:operation-name>BILLING_PKG.CREATE_ACCOUNT</jca:operation-name>
        </tran:provider-specific>
    </ser:endpointConfig>
</xml-fragment>"#;
    parse_oracle_soa("Billing/BillingDB_BS.bix", osb_bix_xml, &mut graph);

    // 5. Backend PL/SQL stored procedure
    let plsql_code = r#"
CREATE OR REPLACE PACKAGE BODY BILLING_PKG AS
    PROCEDURE CREATE_ACCOUNT(p_acc_id IN VARCHAR2) IS
    BEGIN
        INSERT INTO CUSTOMER_ACCOUNT (ACCOUNT_ID) VALUES (p_acc_id);
    END CREATE_ACCOUNT;
END BILLING_PKG;
"#;
    parse_plsql("database/packages/BILLING_PKG.pkb", plsql_code, &mut graph);

    // VERIFY END-TO-END CALL CHAIN:
    // osb_proxy::Billing_PS -> osb_pipeline::Billing_Pipeline -> osb_bix::BillingDB_BS -> plsql::BILLING_PKG.CREATE_ACCOUNT
    assert!(graph.has_edge("osb_proxy::Billing_PS", "osb_pipeline::Billing_Pipeline"));
    assert!(graph.has_edge("osb_pipeline::Billing_Pipeline", "osb_bix::BillingDB_BS"));
    assert!(graph.has_edge("osb_bix::BillingDB_BS", "plsql::BILLING_PKG.CREATE_ACCOUNT"));

    // Verify graph path traversal across the ESB bridge:
    let paths = graph.find_paths("osb_proxy::Billing_PS", "plsql::BILLING_PKG.CREATE_ACCOUNT", 5);
    assert!(!paths.is_empty(), "Topological path from OSB Proxy to PL/SQL Stored Procedure must resolve");
}
