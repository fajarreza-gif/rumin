use ai_rumin::graph::CodeGraph;
use ai_rumin::hippocampal::HippocampalEngine;
use ai_rumin::indexer::Indexer;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{ContextBudget, EntityType};
use std::path::PathBuf;

#[test]
fn test_shebang_detection() {
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env bash\necho hi"), Some("bash"));
    assert_eq!(Indexer::detect_shebang(b"#!/bin/sh\necho hi"), Some("bash"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env python3\nprint(1)"), Some("python"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env node\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env bun\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env deno\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env ts-node\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env escript\nmain(_) -> ok."), Some("erlang"));
    assert_eq!(Indexer::detect_shebang(b"// normal code without shebang"), None);
}

#[test]
fn test_typescript_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
export interface UserSession {
    sessionId: string;
}
export type AuthToken = string;
export enum Role { Admin, User }
export class SessionManager {
    public getSession() {}
}
export async function authenticateUser(token: AuthToken): Promise<boolean> {
    return true;
}
export const generateSessionId = (): string => {
    return "id";
};
"#;
    Indexer::parse_typescript("src/session.ts", content, &mut graph);

    assert!(graph.get_entity("src/session.ts::UserSession").is_some());
    assert_eq!(graph.get_entity("src/session.ts::UserSession").unwrap().entity_type, EntityType::Interface);

    assert!(graph.get_entity("src/session.ts::AuthToken").is_some());
    assert_eq!(graph.get_entity("src/session.ts::AuthToken").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/session.ts::Role").is_some());
    assert_eq!(graph.get_entity("src/session.ts::Role").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/session.ts::SessionManager").is_some());
    assert_eq!(graph.get_entity("src/session.ts::SessionManager").unwrap().entity_type, EntityType::Class);

    assert!(graph.get_entity("src/session.ts::authenticateUser").is_some());
    assert_eq!(graph.get_entity("src/session.ts::authenticateUser").unwrap().entity_type, EntityType::Function);

    assert!(graph.get_entity("src/session.ts::generateSessionId").is_some());
    assert_eq!(graph.get_entity("src/session.ts::generateSessionId").unwrap().entity_type, EntityType::Function);
}

#[test]
fn test_go_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
package service

type UserStore interface {
    GetUser(id int64) error
}

type User struct {
    ID int64
}

type UserService struct {
    store UserStore
}

func NewUserService(store UserStore) *UserService {
    return &UserService{store: store}
}

func (s *UserService) FetchUserProfile(id int64) error {
    return nil
}
"#;
    Indexer::parse_go("pkg/service.go", content, &mut graph);

    assert!(graph.get_entity("pkg/service.go::pkg::service").is_some());
    assert_eq!(graph.get_entity("pkg/service.go::pkg::service").unwrap().entity_type, EntityType::Module);

    assert!(graph.get_entity("pkg/service.go::UserStore").is_some());
    assert_eq!(graph.get_entity("pkg/service.go::UserStore").unwrap().entity_type, EntityType::Interface);

    assert!(graph.get_entity("pkg/service.go::User").is_some());
    assert_eq!(graph.get_entity("pkg/service.go::User").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("pkg/service.go::NewUserService").is_some());
    assert_eq!(graph.get_entity("pkg/service.go::NewUserService").unwrap().entity_type, EntityType::Function);

    assert!(graph.get_entity("pkg/service.go::UserService.FetchUserProfile").is_some());
    assert_eq!(graph.get_entity("pkg/service.go::UserService.FetchUserProfile").unwrap().entity_type, EntityType::Method);
}

#[test]
fn test_c_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
#define BUFFER_MAX_CAPACITY 4096

typedef enum {
    BUFFER_READY,
    BUFFER_OVERFLOW
} BufferState;

typedef struct {
    char data[BUFFER_MAX_CAPACITY];
    BufferState state;
} ByteBuffer;

int byte_buffer_init(ByteBuffer *buf) {
    return 0;
}

int byte_buffer_append(ByteBuffer *buf, const char *src, size_t len) {
    return 0;
}
"#;
    Indexer::parse_c("src/buffer.c", content, &mut graph);

    assert!(graph.get_entity("src/buffer.c::macro::BUFFER_MAX_CAPACITY").is_some());
    assert_eq!(graph.get_entity("src/buffer.c::macro::BUFFER_MAX_CAPACITY").unwrap().entity_type, EntityType::Variable);

    assert!(graph.get_entity("src/buffer.c::type::BufferState").is_some());
    assert_eq!(graph.get_entity("src/buffer.c::type::BufferState").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/buffer.c::type::ByteBuffer").is_some());
    assert_eq!(graph.get_entity("src/buffer.c::type::ByteBuffer").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/buffer.c::byte_buffer_init").is_some());
    assert_eq!(graph.get_entity("src/buffer.c::byte_buffer_init").unwrap().entity_type, EntityType::Function);

    assert!(graph.get_entity("src/buffer.c::byte_buffer_append").is_some());
    assert_eq!(graph.get_entity("src/buffer.c::byte_buffer_append").unwrap().entity_type, EntityType::Function);
}

#[test]
fn test_cpp_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
namespace core::engine {

struct EngineConfig {
    int max_threads;
};

class ExecutionEngine {
public:
    void run();
};

void ExecutionEngine::run() {
}

ExecutionEngine* create_engine() {
    return nullptr;
}

}
"#;
    Indexer::parse_cpp("src/engine.cpp", content, &mut graph);

    assert!(graph.get_entity("src/engine.cpp::ns::core::engine").is_some());
    assert_eq!(graph.get_entity("src/engine.cpp::ns::core::engine").unwrap().entity_type, EntityType::Module);

    assert!(graph.get_entity("src/engine.cpp::EngineConfig").is_some());
    assert_eq!(graph.get_entity("src/engine.cpp::EngineConfig").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/engine.cpp::ExecutionEngine").is_some());
    assert_eq!(graph.get_entity("src/engine.cpp::ExecutionEngine").unwrap().entity_type, EntityType::Class);

    assert!(graph.get_entity("src/engine.cpp::ExecutionEngine::run").is_some());
    assert_eq!(graph.get_entity("src/engine.cpp::ExecutionEngine::run").unwrap().entity_type, EntityType::Method);

    assert!(graph.get_entity("src/engine.cpp::create_engine").is_some());
    assert_eq!(graph.get_entity("src/engine.cpp::create_engine").unwrap().entity_type, EntityType::Function);
}

#[test]
fn test_java_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
package com.airmem.service;

public interface UserRepository {
    String findEmail();
}

public enum AccountStatus {
    ACTIVE, CLOSED
}

public record UserProfile(long id, String email) {}

public class UserService {
    public UserProfile fetchUserProfile(long id) {
        return null;
    }
}
"#;
    Indexer::parse_java("src/UserService.java", content, &mut graph);

    assert!(graph.get_entity("src/UserService.java::pkg::com.airmem.service").is_some());
    assert_eq!(graph.get_entity("src/UserService.java::pkg::com.airmem.service").unwrap().entity_type, EntityType::Module);

    assert!(graph.get_entity("src/UserService.java::UserRepository").is_some());
    assert_eq!(graph.get_entity("src/UserService.java::UserRepository").unwrap().entity_type, EntityType::Interface);

    assert!(graph.get_entity("src/UserService.java::AccountStatus").is_some());
    assert_eq!(graph.get_entity("src/UserService.java::AccountStatus").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/UserService.java::UserProfile").is_some());
    assert_eq!(graph.get_entity("src/UserService.java::UserProfile").unwrap().entity_type, EntityType::Class);

    assert!(graph.get_entity("src/UserService.java::UserService").is_some());
    assert_eq!(graph.get_entity("src/UserService.java::UserService").unwrap().entity_type, EntityType::Class);

    assert!(graph.get_entity("src/UserService.java::fetchUserProfile").is_some());
    assert_eq!(graph.get_entity("src/UserService.java::fetchUserProfile").unwrap().entity_type, EntityType::Method);
}

#[test]
fn test_xml_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<beans>
    <bean id="dataSource" class="org.postgresql.Driver">
        <property name="url" value="jdbc:postgresql://localhost/rumin" />
    </bean>
    <service id="authService" name="AuthenticationService">
    </service>
</beans>
"#;
    Indexer::parse_xml("config/beans.xml", content, &mut graph);

    let entities: Vec<_> = graph.graph.node_weights().filter(|e| e.file_path == "config/beans.xml").collect();
    assert!(!entities.is_empty());
    assert!(entities.iter().any(|e| e.name == "bean#dataSource" && e.entity_type == EntityType::Tag));
    assert!(entities.iter().any(|e| e.name == "service#authService" && e.entity_type == EntityType::Tag));
}

#[test]
fn test_html_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"<!DOCTYPE html>
<html>
<head>
    <title>AI-Rumin Dashboard</title>
</head>
<body>
    <header id="site-header">
        <app-nav class="nav-bar"></app-nav>
    </header>
    <main id="main-content">
        <form id="search-form">
            <template id="result-tmpl"></template>
        </form>
    </main>
</body>
</html>
"#;
    Indexer::parse_html("public/index.html", content, &mut graph);

    let entities: Vec<_> = graph.graph.node_weights().filter(|e| e.file_path == "public/index.html").collect();
    assert!(!entities.is_empty());
    assert!(entities.iter().any(|e| e.name == "title" && e.entity_type == EntityType::Tag));
    assert!(entities.iter().any(|e| e.name == "header#site-header" && e.entity_type == EntityType::Tag));
    assert!(entities.iter().any(|e| e.name == "app-nav" && e.entity_type == EntityType::Tag));
    assert!(entities.iter().any(|e| e.name == "form#search-form" && e.entity_type == EntityType::Tag));
    assert!(entities.iter().any(|e| e.name == "template#result-tmpl" && e.entity_type == EntityType::Tag));
}

#[test]
fn test_oracle_plsql_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
CREATE OR REPLACE PACKAGE emp_admin AS
    PROCEDURE fire_employee(emp_id NUMBER);
    FUNCTION get_salary(emp_id NUMBER) RETURN NUMBER;
END emp_admin;

CREATE OR REPLACE TRIGGER audit_emp
BEFORE UPDATE ON employees
FOR EACH ROW
BEGIN
    INSERT INTO emp_audit VALUES (:OLD.id, SYSDATE);
END;

CREATE OR REPLACE TYPE address_t AS OBJECT (
    street VARCHAR2(50),
    city VARCHAR2(30)
);

CREATE SEQUENCE emp_seq START WITH 1;
"#;
    Indexer::parse_plsql("schema/emp_admin.pkb", content, &mut graph);

    assert!(graph.get_entity("schema/emp_admin.pkb::pkg::emp_admin").is_some());
    assert_eq!(graph.get_entity("schema/emp_admin.pkb::pkg::emp_admin").unwrap().entity_type, EntityType::Module);

    assert!(graph.get_entity("schema/emp_admin.pkb::emp_admin.fire_employee").is_some());
    assert_eq!(graph.get_entity("schema/emp_admin.pkb::emp_admin.fire_employee").unwrap().entity_type, EntityType::Method);

    assert!(graph.get_entity("schema/emp_admin.pkb::emp_admin.get_salary").is_some());
    assert_eq!(graph.get_entity("schema/emp_admin.pkb::emp_admin.get_salary").unwrap().entity_type, EntityType::Method);

    assert!(graph.get_entity("schema/emp_admin.pkb::audit_emp").is_some());
    assert_eq!(graph.get_entity("schema/emp_admin.pkb::audit_emp").unwrap().entity_type, EntityType::Rule);

    assert!(graph.get_entity("schema/emp_admin.pkb::address_t").is_some());
    assert_eq!(graph.get_entity("schema/emp_admin.pkb::address_t").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("schema/emp_admin.pkb::emp_seq").is_some());
    assert_eq!(graph.get_entity("schema/emp_admin.pkb::emp_seq").unwrap().entity_type, EntityType::Variable);
}

#[test]
fn test_apex_class_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
public with sharing class OpportunityService {
    public interface DiscountCalculator {
        Decimal calculate(Decimal amount);
    }

    public enum OpportunityStage {
        PROSPECTING, CLOSED_WON, CLOSED_LOST
    }

    @AuraEnabled
    public static List<Opportunity> fetchWonOpportunities() {
        return [SELECT Id, Name, Amount FROM Opportunity WHERE StageName = 'Closed Won'];
    }
}
"#;
    Indexer::parse_apex_class("classes/OpportunityService.cls", content, &mut graph);

    assert!(graph.get_entity("classes/OpportunityService.cls::OpportunityService").is_some());
    assert_eq!(graph.get_entity("classes/OpportunityService.cls::OpportunityService").unwrap().entity_type, EntityType::Class);

    assert!(graph.get_entity("classes/OpportunityService.cls::DiscountCalculator").is_some());
    assert_eq!(graph.get_entity("classes/OpportunityService.cls::DiscountCalculator").unwrap().entity_type, EntityType::Interface);

    assert!(graph.get_entity("classes/OpportunityService.cls::OpportunityStage").is_some());
    assert_eq!(graph.get_entity("classes/OpportunityService.cls::OpportunityStage").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("classes/OpportunityService.cls::OpportunityService.fetchWonOpportunities").is_some());
    assert_eq!(graph.get_entity("classes/OpportunityService.cls::OpportunityService.fetchWonOpportunities").unwrap().entity_type, EntityType::Method);

    // Embedded SOQL references
    assert!(graph.get_entity("sobject::Opportunity").is_some());
    assert_eq!(graph.get_entity("sobject::Opportunity").unwrap().entity_type, EntityType::Table);

    let opp_service_idx = graph.get_index("classes/OpportunityService.cls::OpportunityService").unwrap();
    let opp_idx = graph.get_index("sobject::Opportunity").unwrap();
    assert!(graph.graph.contains_edge(opp_service_idx, opp_idx));
}

#[test]
fn test_apex_trigger_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
trigger ContactTrigger on Contact (before insert, after update, after delete) {
    ContactTriggerHandler.handle(Trigger.new);
}
"#;
    Indexer::parse_apex_trigger("triggers/ContactTrigger.trigger", content, &mut graph);

    assert!(graph.get_entity("triggers/ContactTrigger.trigger::trigger::ContactTrigger").is_some());
    assert_eq!(graph.get_entity("triggers/ContactTrigger.trigger::trigger::ContactTrigger").unwrap().entity_type, EntityType::Rule);

    assert!(graph.get_entity("sobject::Contact").is_some());
    let trig_idx = graph.get_index("triggers/ContactTrigger.trigger::trigger::ContactTrigger").unwrap();
    let sobj_idx = graph.get_index("sobject::Contact").unwrap();
    assert!(graph.graph.contains_edge(trig_idx, sobj_idx));
}

#[test]
fn test_salesforce_metadata_parser() {
    let mut graph = CodeGraph::new();
    let obj_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomObject xmlns="http://soap.sforce.com/2006/04/metadata">
    <label>Invoice</label>
</CustomObject>
"#;
    Indexer::parse_salesforce_metadata("objects/Invoice__c/Invoice__c.object-meta.xml", obj_content, &mut graph);
    assert!(graph.get_entity("sobject::Invoice__c").is_some());
    assert_eq!(graph.get_entity("sobject::Invoice__c").unwrap().entity_type, EntityType::Table);

    let field_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomField xmlns="http://soap.sforce.com/2006/04/metadata">
    <fullName>TotalAmount__c</fullName>
    <label>Total Amount</label>
    <type>Currency</type>
</CustomField>
"#;
    Indexer::parse_salesforce_metadata("objects/Invoice__c/fields/TotalAmount__c.field-meta.xml", field_content, &mut graph);
    let field_id = "objects/Invoice__c/fields/TotalAmount__c.field-meta.xml::field::TotalAmount__c";
    assert!(graph.get_entity(field_id).is_some());
    assert_eq!(graph.get_entity(field_id).unwrap().entity_type, EntityType::Column);

    let inv_idx = graph.get_index("sobject::Invoice__c").unwrap();
    let fld_idx = graph.get_index(field_id).unwrap();
    assert!(graph.graph.contains_edge(inv_idx, fld_idx));
}

#[test]
fn test_all_salesforce_meta_xml_variations_digestion() {
    // 1. is_supported_file checks on any *-meta.xml variation
    assert!(Indexer::is_supported_file("OrderSync.flow-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("Admin.profile-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("Order_Admin.permissionset-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("Account-Account_Layout.layout-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("CustomLabels.labels-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("Account.workflow-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("MyService.cls-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("Quote.sharingRules-meta.xml", "xml"));
    assert!(Indexer::is_supported_file("es_MX.translation-meta.xml", "xml"));

    let mut graph = CodeGraph::new();

    // 2. Flow metadata with SObject reference
    let flow_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<Flow xmlns="http://soap.sforce.com/2006/04/metadata">
    <description>Syncs order to ERP backend</description>
    <label>Order Sync Flow</label>
    <processType>AutoLaunchedFlow</processType>
    <status>Active</status>
    <object>Order</object>
</Flow>"#;
    Indexer::parse_salesforce_metadata("force-app/main/default/flows/OrderSync.flow-meta.xml", flow_content, &mut graph);

    let flow_ent = graph.get_entity("salesforce::flow::OrderSync").expect("Flow entity should be indexed");
    assert_eq!(flow_ent.name, "Order Sync Flow");
    assert_eq!(flow_ent.entity_type, EntityType::Rule);
    assert_eq!(flow_ent.docstring.as_deref(), Some("Syncs order to ERP backend"));
    assert!(graph.get_entity("sobject::Order").is_some(), "Referenced sobject::Order must be in graph");
    assert!(graph.has_edge("salesforce::flow::OrderSync", "sobject::Order"), "Flow must reference sobject::Order");

    // 3. PermissionSet metadata
    let permset_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<PermissionSet xmlns="http://soap.sforce.com/2006/04/metadata">
    <description>Grants order access</description>
    <label>Order Manager</label>
</PermissionSet>"#;
    Indexer::parse_salesforce_metadata("force-app/main/default/permissionsets/Order_Manager.permissionset-meta.xml", permset_content, &mut graph);
    let perm_ent = graph.get_entity("salesforce::permissionset::Order_Manager").expect("PermissionSet entity");
    assert_eq!(perm_ent.entity_type, EntityType::Rule);

    // 4. CustomLabels metadata
    let labels_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomLabels xmlns="http://soap.sforce.com/2006/04/metadata">
    <labels>
        <fullName>Order_Confirmation_Message</fullName>
        <value>Your order has been placed</value>
    </labels>
</CustomLabels>"#;
    Indexer::parse_salesforce_metadata("force-app/main/default/labels/CustomLabels.labels-meta.xml", labels_content, &mut graph);
    assert!(graph.get_entity("salesforce::labels::CustomLabels").is_some());
    let label_item = graph.get_entity("label::Order_Confirmation_Message").expect("Label item entity");
    assert_eq!(label_item.entity_type, EntityType::Variable);
    assert!(graph.has_edge("salesforce::labels::CustomLabels", "label::Order_Confirmation_Message"));
}

#[test]
fn test_soql_and_aura_parser() {
    let mut graph = CodeGraph::new();
    let soql_content = "SELECT Id, Name FROM Lead WHERE IsConverted = false";
    Indexer::parse_soql("queries/open_leads.soql", soql_content, &mut graph);

    assert!(graph.get_entity("queries/open_leads.soql::query::1").is_some());
    assert!(graph.get_entity("sobject::Lead").is_some());

    let aura_content = r#"<aura:component controller="LeadController" access="global">
    <aura:attribute name="leadId" type="String" />
</aura:component>
"#;
    Indexer::parse_aura("aura/leadCard/leadCard.cmp", aura_content, &mut graph);
    assert!(graph.get_entity("aura/leadCard/leadCard.cmp::component::leadCard").is_some());
    assert!(graph.get_entity("apex::LeadController").is_some());

    let comp_idx = graph.get_index("aura/leadCard/leadCard.cmp::component::leadCard").unwrap();
    let ctrl_idx = graph.get_index("apex::LeadController").unwrap();
    assert!(graph.graph.contains_edge(comp_idx, ctrl_idx));
}

#[test]
fn test_erlang_module_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
-module(math_service).
-behaviour(gen_server).

-record(math_state, {counter = 0, history = []}).
-define(MAX_OPS, 100).
-type op() :: add | sub | mul.
-include("math_defs.hrl").

-export([start_link/0, add/2, handle_call/3]).

start_link() ->
    gen_server:start_link({local, ?MODULE}, ?MODULE, [], []).

add(A, B) ->
    gen_server:call(?MODULE, {add, A, B}).

handle_call({add, A, B}, _From, State) ->
    {reply, A + B, State}.
"#;
    Indexer::parse_erlang_module("src/math_service.erl", content, &mut graph);

    assert!(graph.get_entity("src/math_service.erl::math_service").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::math_service").unwrap().entity_type, EntityType::Module);

    assert!(graph.get_entity("otp::gen_server").is_some());
    assert_eq!(graph.get_entity("otp::gen_server").unwrap().entity_type, EntityType::Interface);

    assert!(graph.get_entity("src/math_service.erl::record::math_state").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::record::math_state").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/math_service.erl::macro::MAX_OPS").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::macro::MAX_OPS").unwrap().entity_type, EntityType::Variable);

    assert!(graph.get_entity("src/math_service.erl::type::op").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::type::op").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("src/math_service.erl::start_link/0").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::start_link/0").unwrap().entity_type, EntityType::Function);

    assert!(graph.get_entity("src/math_service.erl::add/2").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::add/2").unwrap().entity_type, EntityType::Function);

    assert!(graph.get_entity("src/math_service.erl::handle_call/3").is_some());
    assert_eq!(graph.get_entity("src/math_service.erl::handle_call/3").unwrap().entity_type, EntityType::Function);

    // Verify Implements edge to otp::gen_server
    let mod_idx = graph.get_index("src/math_service.erl::math_service").unwrap();
    let otp_idx = graph.get_index("otp::gen_server").unwrap();
    assert!(graph.graph.contains_edge(mod_idx, otp_idx));
}

#[test]
fn test_erlang_header_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
-record(user_session, {id, token, expires}).
-define(SESSION_TIMEOUT, 1800).
"#;
    Indexer::parse_erlang_module("include/session.hrl", content, &mut graph);

    assert!(graph.get_entity("include/session.hrl::record::user_session").is_some());
    assert_eq!(graph.get_entity("include/session.hrl::record::user_session").unwrap().entity_type, EntityType::Struct);

    assert!(graph.get_entity("include/session.hrl::macro::SESSION_TIMEOUT").is_some());
    assert_eq!(graph.get_entity("include/session.hrl::macro::SESSION_TIMEOUT").unwrap().entity_type, EntityType::Variable);
}

#[test]
fn test_erlang_app_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
{application, auth_service, [
    {description, "Authentication Service"},
    {vsn, "1.0.0"},
    {registered, [auth_server]},
    {mod, {auth_app, []}},
    {applications, [kernel, stdlib, crypto]},
    {modules, [auth_server, auth_app]}
]}.
"#;
    Indexer::parse_erlang_app("src/auth_service.app.src", content, &mut graph);

    assert!(graph.get_entity("src/auth_service.app.src::application::auth_service").is_some());
    assert_eq!(graph.get_entity("src/auth_service.app.src::application::auth_service").unwrap().entity_type, EntityType::Module);

    assert!(graph.get_entity("erlang::auth_server").is_some());
    assert!(graph.get_entity("erlang::app::crypto").is_some());

    let app_idx = graph.get_index("src/auth_service.app.src::application::auth_service").unwrap();
    let mod_idx = graph.get_index("erlang::auth_server").unwrap();
    let dep_idx = graph.get_index("erlang::app::crypto").unwrap();
    assert!(graph.graph.contains_edge(app_idx, mod_idx));
    assert!(graph.graph.contains_edge(app_idx, dep_idx));
}

#[test]
fn test_rebar_config_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"
{erl_opts, [debug_info]}.
{deps, [
    {cowboy, "2.9.0"},
    {jsx, "3.1.0"}
]}.
"#;
    Indexer::parse_rebar_config("rebar.config", content, &mut graph);

    assert!(graph.get_entity("rebar.config::config").is_some());
    assert!(graph.get_entity("rebar.config::dep::cowboy").is_some());
    assert!(graph.get_entity("rebar.config::dep::jsx").is_some());
}

#[test]
fn test_apigee_manifest_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<APIProxy name="payments-v1">
    <Basepaths>
        <BasePath>/v1/payments</BasePath>
    </Basepaths>
    <ProxyEndpoints>
        <ProxyEndpoint>default</ProxyEndpoint>
    </ProxyEndpoints>
    <TargetEndpoints>
        <TargetEndpoint>default</TargetEndpoint>
    </TargetEndpoints>
    <Policies>
        <Policy>VerifyApiKey</Policy>
    </Policies>
</APIProxy>
"#;
    Indexer::parse_apigee("apiproxy/payments-v1.xml", content, &mut graph);

    assert!(graph.get_entity("apiproxy::payments-v1").is_some());
    assert_eq!(graph.get_entity("apiproxy::payments-v1").unwrap().entity_type, EntityType::Module);

    let manifest_idx = graph.get_index("apiproxy::payments-v1").unwrap();
    let proxy_idx = graph.get_index("apiproxy::proxy::default").unwrap();
    let target_idx = graph.get_index("apiproxy::target::default").unwrap();
    let policy_idx = graph.get_index("apiproxy::policy::VerifyApiKey").unwrap();

    assert!(graph.graph.contains_edge(manifest_idx, proxy_idx));
    assert!(graph.graph.contains_edge(manifest_idx, target_idx));
    assert!(graph.graph.contains_edge(manifest_idx, policy_idx));
}

#[test]
fn test_apigee_proxy_endpoint_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProxyEndpoint name="default">
    <HTTPProxyConnection>
        <BasePath>/v1/payments</BasePath>
    </HTTPProxyConnection>
    <PreFlow>
        <Request>
            <Step>
                <Name>VerifyApiKey</Name>
            </Step>
        </Request>
    </PreFlow>
    <RouteRule name="default">
        <TargetEndpoint>payment-gateway</TargetEndpoint>
    </RouteRule>
</ProxyEndpoint>
"#;
    Indexer::parse_apigee("apiproxy/proxies/default.xml", content, &mut graph);

    assert!(graph.get_entity("apiproxy::proxy::default").is_some());
    assert_eq!(graph.get_entity("apiproxy::proxy::default").unwrap().entity_type, EntityType::Interface);

    assert!(graph.get_entity("apiproxy::policy::VerifyApiKey").is_some());
    assert!(graph.get_entity("apiproxy::target::payment-gateway").is_some());

    let proxy_idx = graph.get_index("apiproxy::proxy::default").unwrap();
    let policy_idx = graph.get_index("apiproxy::policy::VerifyApiKey").unwrap();
    let target_idx = graph.get_index("apiproxy::target::payment-gateway").unwrap();

    assert!(graph.graph.contains_edge(proxy_idx, policy_idx));
    assert!(graph.graph.contains_edge(proxy_idx, target_idx));
}

#[test]
fn test_apigee_policy_parser() {
    let mut graph = CodeGraph::new();
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<Javascript name="TransformPayload">
    <ResourceURL>jsc://TransformPayload.js</ResourceURL>
</Javascript>
"#;
    Indexer::parse_apigee("apiproxy/policies/TransformPayload.xml", content, &mut graph);

    assert!(graph.get_entity("apiproxy::policy::TransformPayload").is_some());
    assert_eq!(graph.get_entity("apiproxy::policy::TransformPayload").unwrap().entity_type, EntityType::Rule);

    assert!(graph.get_entity("apiproxy::resource::TransformPayload.js").is_some());

    let policy_idx = graph.get_index("apiproxy::policy::TransformPayload").unwrap();
    let res_idx = graph.get_index("apiproxy::resource::TransformPayload.js").unwrap();
    assert!(graph.graph.contains_edge(policy_idx, res_idx));
}

#[tokio::test]
async fn test_end_to_end_fixtures_indexing() {
    let repo_path = PathBuf::from("tests/fixtures");
    let sidecar_path = PathBuf::from("sidecar/worker.py");
    let sidecar = SidecarClient::new(sidecar_path);
    let indexer = Indexer::new(&repo_path);
    let mut graph = CodeGraph::new();

    let count = indexer.index_repository(&mut graph, &sidecar).await.expect("indexing failed");
    assert!(count >= 24, "Expected at least 24 fixture files indexed, found {}", count);

    // Verify entities from diverse languages
    let all_entities: Vec<_> = graph.graph.node_weights().collect();
    assert!(all_entities.len() >= 40, "Expected at least 40 structural entities, got {}", all_entities.len());

    // Verify TypeScript entity
    assert!(all_entities.iter().any(|e| e.name == "UserSession" || e.name == "SessionManager"));

    // Verify Go entity
    assert!(all_entities.iter().any(|e| e.name == "UserStore" || e.name == "UserService.FetchUserProfile"));

    // Verify C entity
    assert!(all_entities.iter().any(|e| e.name == "byte_buffer_init" || e.name == "BUFFER_MAX_CAPACITY"));

    // Verify C++ entity
    assert!(all_entities.iter().any(|e| e.name == "ExecutionEngine::run" || e.name == "create_engine"));

    // Verify Java entity
    assert!(all_entities.iter().any(|e| e.name == "UserService" || e.name == "fetchUserProfile"));

    // Verify XML entity
    assert!(all_entities.iter().any(|e| e.name == "bean#dataSource" || e.name == "bean#userRepository"));

    // Verify HTML entity
    assert!(all_entities.iter().any(|e| e.name == "header#site-header" || e.name == "app-nav" || e.name == "form#search-form"));

    // Verify Oracle PL/SQL entity
    assert!(all_entities.iter().any(|e| e.name == "billing_manager" || e.name == "billing_manager.process_invoice" || e.name == "trg_invoice_audit"));

    // Verify Salesforce ecosystem entities
    assert!(all_entities.iter().any(|e| e.name == "AccountService" || e.name == "AccountService.getActiveAccounts"));
    assert!(all_entities.iter().any(|e| e.name == "AccountTrigger" || e.name == "Account"));
    assert!(all_entities.iter().any(|e| e.name == "Rating__c" || e.name == "accountCard"));

    // Verify Erlang OTP ecosystem entities
    assert!(all_entities.iter().any(|e| e.name == "cache_server" || e.name == "start_link/0" || e.name == "get/1"));
    assert!(all_entities.iter().any(|e| e.name == "cache_sup" || e.name == "cache_app"));
    assert!(all_entities.iter().any(|e| e.name == "cache_entry" || e.name == "DEFAULT_TTL"));

    // Verify Apigee API proxy ecosystem entities
    assert!(all_entities.iter().any(|e| e.name == "orders-api" || e.name == "VerifyApiKey"));
    assert!(all_entities.iter().any(|e| e.name == "RateLimit" || e.name == "FormatResponse"));

    // Verify Associative Recall via HippocampalEngine
    let hippocampal = HippocampalEngine::new(0.85);
    let budget = ContextBudget {
        total_token_limit: 1500,
        max_symbols: 10,
        include_signatures: true,
        expand_bodies: false,
        ..Default::default()
    };

    let recall = hippocampal.query(&graph, "billing invoice audit tax", &budget);
    assert!(!recall.top_symbols.is_empty(), "Hippocampal recall should return associative symbols for billing invoice query");

    // Verify Salesforce associative recall across triggers, SObjects, and controllers
    let sf_recall = hippocampal.query(&graph, "Account trigger rating active", &budget);
    assert!(!sf_recall.top_symbols.is_empty(), "Hippocampal recall should return associative symbols for Salesforce Account query");

    // Verify Erlang OTP associative recall across supervisor, gen_server, and cache header
    let erl_recall = hippocampal.query(&graph, "cache_app supervisor gen_server ttl", &budget);
    assert!(!erl_recall.top_symbols.is_empty(), "Hippocampal recall should return associative symbols for Erlang cache query");

    // Verify Apigee associative recall across APIProxy, ProxyEndpoint, policies, and target
    let apigee_recall = hippocampal.query(&graph, "orders-api basepath verifyapikey rate limit backend", &budget);
    assert!(!apigee_recall.top_symbols.is_empty(), "Hippocampal recall should return associative symbols for Apigee orders-api query");
}
