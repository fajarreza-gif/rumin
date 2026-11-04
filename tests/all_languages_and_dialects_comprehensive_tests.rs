use ai_rumin::graph::CodeGraph;
use ai_rumin::hippocampal::HippocampalEngine;
use ai_rumin::indexer::Indexer;
use ai_rumin::parsers;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{EdgeKind, EntityType};
use std::fs;

// =============================================================================
// 1. RUST LANGUAGE PARSER TESTS (.rs)
// =============================================================================
#[test]
fn test_comprehensive_rust_parser() {
    let mut graph = CodeGraph::new();
    let rust_code = r#"
        pub mod network;
        mod storage;

        pub struct ConnectionPool {
            pub max_conns: usize,
            timeout_ms: u64,
        }

        pub struct UnitStruct;
        pub struct TupleStruct(pub u32, String);

        pub enum ConnectionState {
            Disconnected,
            Connecting { attempt: u32 },
            Connected(String),
        }

        pub trait ChannelHandler {
            fn handle_message(&self, msg: &[u8]) -> bool;
        }

        impl ConnectionPool {
            pub fn new(max_conns: usize) -> Self {
                Self { max_conns, timeout_ms: 5000 }
            }

            pub async fn acquire(&self) -> Result<(), ()> {
                Ok(())
            }
        }

        impl ChannelHandler for ConnectionPool {
            fn handle_message(&self, _msg: &[u8]) -> bool {
                true
            }
        }

        pub async fn run_daemon() {
            let _pool = ConnectionPool::new(10);
        }
    "#;

    parsers::parse_rust("src/telecom/pool.rs", rust_code, &mut graph);

    // 1. Modules
    assert!(graph.get_entity("src/telecom/pool.rs::mod::network").is_some());
    assert_eq!(graph.get_entity("src/telecom/pool.rs::mod::network").unwrap().entity_type, EntityType::Module);
    assert!(graph.get_entity("src/telecom/pool.rs::mod::storage").is_some());

    // 2. Structs
    assert!(graph.get_entity("src/telecom/pool.rs::ConnectionPool").is_some());
    assert_eq!(graph.get_entity("src/telecom/pool.rs::ConnectionPool").unwrap().entity_type, EntityType::Struct);
    assert!(graph.get_entity("src/telecom/pool.rs::UnitStruct").is_some());
    assert!(graph.get_entity("src/telecom/pool.rs::TupleStruct").is_some());

    // 3. Enums
    assert!(graph.get_entity("src/telecom/pool.rs::ConnectionState").is_some());
    assert_eq!(graph.get_entity("src/telecom/pool.rs::ConnectionState").unwrap().entity_type, EntityType::Class);

    // 4. Traits & Impls
    assert!(graph.get_entity("src/telecom/pool.rs::ChannelHandler").is_some());
    assert_eq!(graph.get_entity("src/telecom/pool.rs::ChannelHandler").unwrap().entity_type, EntityType::Interface);

    // 5. Impl methods & Trait Impl edge
    assert!(graph.get_entity("src/telecom/pool.rs::ConnectionPool::new").is_some());
    assert_eq!(graph.get_entity("src/telecom/pool.rs::ConnectionPool::new").unwrap().entity_type, EntityType::Method);
    assert!(graph.get_entity("src/telecom/pool.rs::ConnectionPool::acquire").is_some());

    let pool_neighbors = graph.neighbors("src/telecom/pool.rs::ConnectionPool", petgraph::Direction::Outgoing);
    let impl_edges: Vec<String> = pool_neighbors.iter().filter(|(_, k)| **k == EdgeKind::Implements).map(|(e, _)| e.id.clone()).collect();
    assert!(impl_edges.iter().any(|id| id.contains("ChannelHandler")), "ConnectionPool must implement ChannelHandler: {:?}", impl_edges);

    // 6. Standalone functions
    assert!(graph.get_entity("src/telecom/pool.rs::run_daemon").is_some());
    assert_eq!(graph.get_entity("src/telecom/pool.rs::run_daemon").unwrap().entity_type, EntityType::Function);
}

// =============================================================================
// 2. PYTHON LANGUAGE PARSER TESTS (.py & Shebang)
// =============================================================================
#[test]
fn test_comprehensive_python_parser() {
    let mut graph = CodeGraph::new();
    let py_code = r#"
"""Enterprise Billing Reconciliation Engine."""
import os
import sys
from datetime import datetime, timezone
from base_engine import BaseReconciler, AuditMixin

class OrderReconciler(BaseReconciler, AuditMixin):
    """Reconciles orders between CRM and billing catalog."""
    def __init__(self, tenant_id: str):
        self.tenant_id = tenant_id

    async def reconcile_batch(self, batch_id: str) -> int:
        return 42

def calculate_checksum(data: bytes) -> str:
    return "sha256"

async def async_dispatch_alert(alert_type: str):
    pass
    "#;

    parsers::parse_python("services/billing/reconciler.py", py_code, &mut graph);

    // 1. Classes & Inheritance
    let cls = graph.get_entity("services/billing/reconciler.py::OrderReconciler").expect("OrderReconciler must exist");
    assert_eq!(cls.entity_type, EntityType::Class);

    let cls_deps = graph.neighbors("services/billing/reconciler.py::OrderReconciler", petgraph::Direction::Outgoing);
    let dep_ids: Vec<String> = cls_deps.iter().filter(|(_, k)| **k == EdgeKind::DependsOn).map(|(e, _)| e.id.clone()).collect();
    assert!(dep_ids.iter().any(|id| id.contains("BaseReconciler")), "OrderReconciler must inherit BaseReconciler");
    assert!(dep_ids.iter().any(|id| id.contains("AuditMixin")), "OrderReconciler must inherit AuditMixin");

    // 2. Methods
    assert!(graph.get_entity("services/billing/reconciler.py::OrderReconciler.__init__").is_some());
    assert_eq!(graph.get_entity("services/billing/reconciler.py::OrderReconciler.__init__").unwrap().entity_type, EntityType::Method);
    assert!(graph.get_entity("services/billing/reconciler.py::OrderReconciler.reconcile_batch").is_some());

    // 3. Functions (sync and async)
    assert!(graph.get_entity("services/billing/reconciler.py::calculate_checksum").is_some());
    assert_eq!(graph.get_entity("services/billing/reconciler.py::calculate_checksum").unwrap().entity_type, EntityType::Function);
    assert!(graph.get_entity("services/billing/reconciler.py::async_dispatch_alert").is_some());

    // 4. Imports
    let file_neighbors = graph.neighbors("services/billing/reconciler.py::file", petgraph::Direction::Outgoing);
    let import_ids: Vec<String> = file_neighbors.iter().filter(|(_, k)| **k == EdgeKind::Imports).map(|(e, _)| e.id.clone()).collect();
    assert!(import_ids.iter().any(|id| id == "py::os"));
    assert!(import_ids.iter().any(|id| id == "py::datetime"));
}

// =============================================================================
// 3. TYPESCRIPT, JAVASCRIPT, TSX, JSX, CJS, MJS PARSER TESTS
// =============================================================================
#[test]
fn test_comprehensive_typescript_and_javascript_parser() {
    let mut graph = CodeGraph::new();

    // 3.1 TypeScript (.ts)
    let ts_code = r#"
        import { AxiosInstance } from "axios";
        const lodash = require("lodash");

        export interface Subscriber {
            msisdn: string;
            imsi: string;
        }

        export type IMSI = string;

        export enum SubscriberStatus {
            ACTIVE = "ACTIVE",
            SUSPENDED = "SUSPENDED"
        }

        export class HssClient {
            private client: AxiosInstance;
            public async fetchSubscriber(id: IMSI): Promise<Subscriber> {
                return { msisdn: "123", imsi: id };
            }
        }

        export async function initializeHss(): Promise<HssClient> {
            return new HssClient();
        }

        export const queryQuota = async (id: string): Promise<number> => {
            return 100;
        };
    "#;
    parsers::parse_typescript("src/telecom/hss.ts", ts_code, &mut graph);

    assert!(graph.get_entity("src/telecom/hss.ts::Subscriber").is_some());
    assert_eq!(graph.get_entity("src/telecom/hss.ts::Subscriber").unwrap().entity_type, EntityType::Interface);
    assert!(graph.get_entity("src/telecom/hss.ts::IMSI").is_some());
    assert!(graph.get_entity("src/telecom/hss.ts::SubscriberStatus").is_some());
    assert!(graph.get_entity("src/telecom/hss.ts::HssClient").is_some());
    assert!(graph.get_entity("src/telecom/hss.ts::HssClient.fetchSubscriber").is_some());
    assert!(graph.get_entity("src/telecom/hss.ts::initializeHss").is_some());
    assert!(graph.get_entity("src/telecom/hss.ts::queryQuota").is_some());

    // Check imports
    assert!(graph.get_entity("ts::axios").is_some());
    assert!(graph.get_entity("ts::lodash").is_some());

    // 3.2 React TSX (.tsx)
    let tsx_code = r#"
        import React, { useState } from "react";
        export interface DashboardProps {
            accountNum: string;
        }
        export const SubscriberDashboard: React.FC<DashboardProps> = (props) => {
            const [active, setActive] = useState(true);
            return <div>{props.accountNum}</div>;
        };
    "#;
    parsers::parse_typescript("src/components/SubscriberDashboard.tsx", tsx_code, &mut graph);
    assert!(graph.get_entity("src/components/SubscriberDashboard.tsx::DashboardProps").is_some());
    assert!(graph.get_entity("src/components/SubscriberDashboard.tsx::SubscriberDashboard").is_some());

    // 3.3 ES Module JavaScript (.mjs)
    let mjs_code = r#"
        import { format } from "date-fns";
        export function generateReport(date) {
            return format(date, "yyyy-MM-dd");
        }
        export const exportToCsv = (data) => {
            return "csv";
        };
    "#;
    parsers::parse_typescript("scripts/report.mjs", mjs_code, &mut graph);
    assert!(graph.get_entity("scripts/report.mjs::generateReport").is_some());
    assert!(graph.get_entity("scripts/report.mjs::exportToCsv").is_some());
}

// =============================================================================
// 4. GO SYSTEMS LANGUAGE PARSER TESTS (.go)
// =============================================================================
#[test]
fn test_comprehensive_go_parser() {
    let mut graph = CodeGraph::new();
    let go_code = r#"
        package gateway

        import (
            "context"
            "net/http"
        )

        type RouteConfig struct {
            Prefix string
            TargetURL string
        }

        type ProxyHandler interface {
            ServeHTTP(w http.ResponseWriter, r *http.Request)
            Shutdown(ctx context.Context) error
        }

        func (rc *RouteConfig) Validate() bool {
            return rc.Prefix != ""
        }

        func (rc RouteConfig) GetTarget() string {
            return rc.TargetURL
        }

        func NewProxyServer(cfg *RouteConfig) ProxyHandler {
            return nil
        }
    "#;

    parsers::parse_go("cmd/gateway/server.go", go_code, &mut graph);

    // Package
    assert!(graph.get_entity("cmd/gateway/server.go::pkg::gateway").is_some());

    // Struct & Interface
    assert!(graph.get_entity("cmd/gateway/server.go::RouteConfig").is_some());
    assert_eq!(graph.get_entity("cmd/gateway/server.go::RouteConfig").unwrap().entity_type, EntityType::Struct);
    assert!(graph.get_entity("cmd/gateway/server.go::ProxyHandler").is_some());
    assert_eq!(graph.get_entity("cmd/gateway/server.go::ProxyHandler").unwrap().entity_type, EntityType::Interface);

    // Receiver methods (pointer and value)
    assert!(graph.get_entity("cmd/gateway/server.go::RouteConfig.Validate").is_some());
    assert_eq!(graph.get_entity("cmd/gateway/server.go::RouteConfig.Validate").unwrap().entity_type, EntityType::Method);
    assert!(graph.get_entity("cmd/gateway/server.go::RouteConfig.GetTarget").is_some());

    // Standalone function
    assert!(graph.get_entity("cmd/gateway/server.go::NewProxyServer").is_some());
    assert_eq!(graph.get_entity("cmd/gateway/server.go::NewProxyServer").unwrap().entity_type, EntityType::Function);

    // Imports
    assert!(graph.get_entity("go::context").is_some());
    assert!(graph.get_entity("go::net/http").is_some());
}

// =============================================================================
// 5. C & C++ SYSTEMS LANGUAGE PARSER TESTS (.c, .h, .cpp, .hpp)
// =============================================================================
#[test]
fn test_comprehensive_c_and_cpp_parser() {
    let mut graph = CodeGraph::new();

    // 5.1 C Header & Source (.h, .c)
    let c_header = r#"
        #include <stdint.h>
        #include <stdbool.h>
        #define MAX_BUFFER_SIZE 8192
        #define PROTOCOL_VERSION 2

        typedef struct PacketHeader {
            uint32_t magic;
            uint16_t length;
        } PacketHeader_t;

        struct Connection {
            int socket_fd;
            bool is_active;
        };

        void init_connection(struct Connection *conn);
    "#;
    parsers::parse_c("include/network/packet.h", c_header, &mut graph);

    assert!(graph.get_entity("include/network/packet.h::macro::MAX_BUFFER_SIZE").is_some());
    assert!(graph.get_entity("include/network/packet.h::macro::PROTOCOL_VERSION").is_some());
    assert!(graph.get_entity("include/network/packet.h::struct::PacketHeader").is_some());
    assert!(graph.get_entity("include/network/packet.h::struct::Connection").is_some());
    assert!(graph.get_entity("c::stdint.h").is_some());

    let c_source = r#"
        #include "packet.h"
        void init_connection(struct Connection *conn) {
            conn->socket_fd = -1;
            conn->is_active = false;
        }
    "#;
    parsers::parse_c("src/network/packet.c", c_source, &mut graph);
    assert!(graph.get_entity("src/network/packet.c::init_connection").is_some());

    // 5.2 C++ Header & Implementation (.hpp, .cpp)
    let cpp_code = r#"
        #include <vector>
        #include <memory>
        #include "engine.hpp"

        namespace telecom::switching {

        template <typename T>
        class MessageQueue {
        public:
            void push(T item) {}
            T pop() {}
        };

        class PacketSwitch : public BaseSwitch {
        public:
            PacketSwitch(int id);
            virtual ~PacketSwitch();
            void routePacket(const Packet& pkt);
        };

        struct SwitchMetrics {
            uint64_t packets_forwarded;
            uint64_t packets_dropped;
        };

        } // namespace telecom::switching

        PacketSwitch::PacketSwitch(int id) {
        }

        void PacketSwitch::routePacket(const Packet& pkt) {
        }
    "#;
    parsers::parse_cpp("src/switching/PacketSwitch.cpp", cpp_code, &mut graph);

    assert!(graph.get_entity("src/switching/PacketSwitch.cpp::ns::telecom::switching").is_some());
    assert!(graph.get_entity("src/switching/PacketSwitch.cpp::MessageQueue").is_some());
    assert!(graph.get_entity("src/switching/PacketSwitch.cpp::PacketSwitch").is_some());
    assert_eq!(graph.get_entity("src/switching/PacketSwitch.cpp::PacketSwitch").unwrap().entity_type, EntityType::Class);
    assert!(graph.get_entity("src/switching/PacketSwitch.cpp::SwitchMetrics").is_some());

    // Out-of-line method definitions
    assert!(graph.get_entity("src/switching/PacketSwitch.cpp::PacketSwitch::PacketSwitch").is_some());
    assert!(graph.get_entity("src/switching/PacketSwitch.cpp::PacketSwitch::routePacket").is_some());
    assert!(graph.get_entity("cpp::vector").is_some());
}

// =============================================================================
// 6. JAVA LANGUAGE PARSER TESTS (.java)
// =============================================================================
#[test]
fn test_comprehensive_java_parser() {
    let mut graph = CodeGraph::new();
    let java_code = r#"
        package com.telco.order.service;

        import java.util.List;
        import java.util.Optional;
        import org.springframework.stereotype.Service;

        public interface OrderProcessor {
            boolean process(String orderId);
        }

        public enum ProvisioningStatus {
            INITIAL, IN_FLIGHT, COMPLETED, FAILED
        }

        public record OrderDetail(String orderId, double totalAmount) {}

        @Service
        public class TelecomOrderService implements OrderProcessor {
            private final OrderRepository repository;

            public TelecomOrderService(OrderRepository repository) {
                this.repository = repository;
            }

            @Override
            public boolean process(String orderId) {
                return true;
            }

            public Optional<OrderDetail> findOrder(String orderId) {
                return Optional.empty();
            }
        }
    "#;

    parsers::parse_java("src/main/java/com/telco/order/service/TelecomOrderService.java", java_code, &mut graph);

    let prefix = "src/main/java/com/telco/order/service/TelecomOrderService.java";
    assert!(graph.get_entity(&format!("{}::pkg::com.telco.order.service", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::OrderProcessor", prefix)).is_some());
    assert_eq!(graph.get_entity(&format!("{}::OrderProcessor", prefix)).unwrap().entity_type, EntityType::Interface);
    assert!(graph.get_entity(&format!("{}::ProvisioningStatus", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::OrderDetail", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::TelecomOrderService", prefix)).is_some());
    assert_eq!(graph.get_entity(&format!("{}::TelecomOrderService", prefix)).unwrap().entity_type, EntityType::Class);

    assert!(graph.get_entity(&format!("{}::TelecomOrderService.process", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::TelecomOrderService.findOrder", prefix)).is_some());
    assert!(graph.get_entity("java::java.util.List").is_some());
}

// =============================================================================
// 7. SALESFORCE ECOSYSTEM: APEX CLASS, TRIGGER, SOQL, AURA, VF (.cls, .trigger, .soql, .cmp, .page)
// =============================================================================
#[test]
fn test_comprehensive_salesforce_code_parser() {
    let mut graph = CodeGraph::new();

    // 7.1 Apex Class (.cls)
    let apex_class = r#"
        @RestResource(urlMapping='/api/v2/orders/*')
        public with sharing class CPQ_OrderRestResource {
            @HttpGet
            public static OrderContext getOrder() {
                List<Account> accs = [SELECT Id, Name FROM Account WHERE Active__c = 'Yes'];
                return null;
            }

            @AuraEnabled
            public static void syncOrder(String orderId) {
                List<OrderItem> items = [SELECT Id, Product2Id, Quantity FROM OrderItem WHERE OrderId = :orderId];
            }

            public class RequestContext {
                public String orderId;
            }
        }
    "#;
    parsers::parse_apex_class("force-app/main/default/classes/CPQ_OrderRestResource.cls", apex_class, &mut graph);

    let cls_id = "force-app/main/default/classes/CPQ_OrderRestResource.cls::CPQ_OrderRestResource";
    assert!(graph.get_entity(cls_id).is_some());
    assert!(graph.get_entity("force-app/main/default/classes/CPQ_OrderRestResource.cls::RequestContext").is_some());
    assert!(graph.get_entity(&format!("{}.getOrder", cls_id)).is_some());
    assert!(graph.get_entity(&format!("{}.syncOrder", cls_id)).is_some());

    // SObject linkages from SOQL
    assert!(graph.get_entity("sobject::Account").is_some());
    assert!(graph.get_entity("sobject::OrderItem").is_some());

    // 7.2 Apex Trigger (.trigger)
    let apex_trigger = r#"
        trigger AssetMigrationTrigger on Asset (before insert, before update, after update) {
            if (Trigger.isBefore) {
                // validation logic
            }
        }
    "#;
    parsers::parse_apex_trigger("force-app/main/default/triggers/AssetMigrationTrigger.trigger", apex_trigger, &mut graph);
    assert!(graph.get_entity("force-app/main/default/triggers/AssetMigrationTrigger.trigger::trigger::AssetMigrationTrigger").is_some());
    assert!(graph.get_entity("sobject::Asset").is_some());

    // 7.3 Standalone SOQL (.soql)
    let soql_file = r#"
        SELECT Id, Name, Account.Name,
            (SELECT Id, LastName, Email FROM Contacts WHERE Email != null)
        FROM Account
        WHERE AnnualRevenue > 5000000 AND Industry = 'Telecommunications'
    "#;
    parsers::parse_soql("queries/high_value_accounts.soql", soql_file, &mut graph);
    assert!(graph.get_entity("sobject::Account").is_some());

    // 7.4 Aura Component (.cmp)
    let aura_cmp = r#"
        <aura:component controller="CPQ_OrderRestResource" implements="flexipage:availableForAllPageTypes">
            <aura:attribute name="orderNumber" type="String"/>
            <aura:attribute name="amount" type="Decimal"/>
            <div>Component markup</div>
        </aura:component>
    "#;
    parsers::parse_aura("force-app/main/default/aura/OrderViewer/OrderViewer.cmp", aura_cmp, &mut graph);
    let aura_id = "force-app/main/default/aura/OrderViewer/OrderViewer.cmp::component::OrderViewer";
    assert!(graph.get_entity(aura_id).is_some());
    assert!(graph.get_entity("apex::CPQ_OrderRestResource").is_some());
    assert!(graph.get_entity(&format!("{}::attr::orderNumber", aura_id)).is_some());

    // 7.5 Visualforce Page (.page)
    let vf_page = r#"
        <apex:page controller="CPQ_OrderRestResource" extensions="OrderExtController,AuditExtController" showHeader="true">
            <h1>Order Invoice</h1>
        </apex:page>
    "#;
    parsers::parse_aura("force-app/main/default/pages/OrderInvoice.page", vf_page, &mut graph);
    let page_id = "force-app/main/default/pages/OrderInvoice.page::component::OrderInvoice";
    assert!(graph.get_entity(page_id).is_some());
    assert!(graph.get_entity("apex::OrderExtController").is_some());
    assert!(graph.get_entity("apex::AuditExtController").is_some());
}

// =============================================================================
// 8. SALESFORCE METADATA: OBJECTS, FIELDS, CUSTOM METADATA (.object-meta.xml, .field-meta.xml, .md-meta.xml)
// =============================================================================
#[test]
fn test_comprehensive_salesforce_metadata_parser() {
    let mut graph = CodeGraph::new();

    // 8.1 Custom Object (.object-meta.xml)
    let obj_meta = r#"<?xml version="1.0" encoding="UTF-8"?>
    <CustomObject xmlns="http://soap.sforce.com/2006/04/metadata">
        <label>Customer Provisioning Order</label>
        <sharingModel>ReadWrite</sharingModel>
    </CustomObject>
    "#;
    parsers::parse_salesforce_metadata("force-app/main/default/objects/ProvisioningOrder__c/ProvisioningOrder__c.object-meta.xml", obj_meta, &mut graph);
    assert!(graph.get_entity("sobject::ProvisioningOrder__c").is_some());

    // 8.2 Custom Field with referenceTo (.field-meta.xml)
    let field_meta = r#"<?xml version="1.0" encoding="UTF-8"?>
    <CustomField xmlns="http://soap.sforce.com/2006/04/metadata">
        <fullName>Account__c</fullName>
        <type>Lookup</type>
        <referenceTo>Account</referenceTo>
        <relationshipName>ProvisioningOrders</relationshipName>
    </CustomField>
    "#;
    parsers::parse_salesforce_metadata("force-app/main/default/objects/ProvisioningOrder__c/fields/Account__c.field-meta.xml", field_meta, &mut graph);
    let field_id = "force-app/main/default/objects/ProvisioningOrder__c/fields/Account__c.field-meta.xml::field::Account__c";
    assert!(graph.get_entity(field_id).is_some());
    assert_eq!(graph.get_entity(field_id).unwrap().metadata.get("referenceTo").unwrap(), "Account");
    assert!(graph.get_entity("sobject::ProvisioningOrder__c.Account__c").is_some());

    // Check relationship edge
    let field_neighbors = graph.neighbors(field_id, petgraph::Direction::Outgoing);
    assert!(field_neighbors.iter().any(|(e, k)| **k == EdgeKind::DependsOn && e.id == "sobject::Account"));

    // 8.3 Custom Metadata Type instance (.md-meta.xml)
    let cmdt_meta = r#"<?xml version="1.0" encoding="UTF-8"?>
    <CustomMetadata xmlns="http://soap.sforce.com/2006/04/metadata">
        <label>Enterprise Tier Gold</label>
        <values><field>DiscountPercent__c</field><value>15.0</value></values>
    </CustomMetadata>
    "#;
    parsers::parse_salesforce_metadata("force-app/main/default/customMetadata/DiscountTier__mdt.EnterpriseGold.md-meta.xml", cmdt_meta, &mut graph);
    assert!(graph.get_entity("cmdt::DiscountTier__mdt.EnterpriseGold").is_some());
}

// =============================================================================
// 9. OMNISTUDIO METADATA: INTEGRATION PROCEDURES, SCRIPTS, CARDS (.oip, .os, .ouc)
// =============================================================================
#[test]
fn test_comprehensive_omnistudio_parser() {
    let mut graph = CodeGraph::new();

    // 9.1 OmniIntegrationProcedure (.oip-meta.xml)
    let oip_content = r#"<?xml version="1.0" encoding="UTF-8"?>
    <OmniIntegrationProcedure xmlns="http://soap.sforce.com/2006/04/metadata">
        <name>B2B_FetchOrderDetails</name>
        <description>Orchestrates customer order query and transformation</description>
        <omniProcessElements>
            <name>RemoteCalloutStep</name>
            <level>0.0</level>
            <propertySetConfig>{&quot;remoteClass&quot;:&quot;CPQ_BillingHandler&quot;,&quot;remoteMethod&quot;:&quot;invokeBillingApi&quot;}</propertySetConfig>
        </omniProcessElements>
        <omniProcessElements>
            <name>TransformDRStep</name>
            <level>1.0</level>
            <propertySetConfig>{&quot;bundleName&quot;:&quot;DR_TransformOrderResponse&quot;}</propertySetConfig>
        </omniProcessElements>
    </OmniIntegrationProcedure>
    "#;
    parsers::parse_omnistudio_metadata("omnistudio/OmniIntegrationProcedure/B2B_FetchOrderDetails.oip-meta.xml", oip_content, &mut graph);

    let ip_id = "omni_ip::B2B_FetchOrderDetails";
    assert!(graph.get_entity(ip_id).is_some());
    assert!(graph.get_entity(&format!("{}::element::RemoteCalloutStep", ip_id)).is_some());
    assert!(graph.get_entity(&format!("{}::element::TransformDRStep", ip_id)).is_some());
    assert!(graph.get_entity("apex::CPQ_BillingHandler").is_some());
    assert!(graph.get_entity("omni_dr::DR_TransformOrderResponse").is_some());

    // 9.2 OmniScript (.os-meta.xml)
    let os_content = r#"<?xml version="1.0" encoding="UTF-8"?>
    <OmniScript xmlns="http://soap.sforce.com/2006/04/metadata">
        <name>CustomerSelfService_Checkout</name>
        <description>Guided flow for B2B ordering</description>
    </OmniScript>
    "#;
    parsers::parse_omnistudio_metadata("omnistudio/OmniScript/CustomerSelfService_Checkout.os-meta.xml", os_content, &mut graph);
    assert!(graph.get_entity("omni_script::CustomerSelfService_Checkout").is_some());

    // 9.3 OmniUiCard (.ouc-meta.xml)
    let ouc_content = r#"<?xml version="1.0" encoding="UTF-8"?>
    <OmniUiCard xmlns="http://soap.sforce.com/2006/04/metadata">
        <name>SubscriberSummaryCard</name>
        <description>FlexCard for active SIM subscriptions</description>
    </OmniUiCard>
    "#;
    parsers::parse_omnistudio_metadata("omnistudio/OmniUiCard/SubscriberSummaryCard.ouc-meta.xml", ouc_content, &mut graph);
    assert!(graph.get_entity("omni_card::SubscriberSummaryCard").is_some());
}

// =============================================================================
// 10. MULTI-DIALECT SQL & PL/SQL PARSER TESTS (Oracle, Postgres, MySQL, SQLite)
// =============================================================================
#[test]
fn test_comprehensive_sql_and_plsql_dialects() {
    let mut graph = CodeGraph::new();

    // 10.1 Oracle PL/SQL & Siebel Catalist CRM DDL
    let oracle_sql = r#"
-- =============================================================================
-- Table: SIEBEL.S_ASSET
-- Columns: 429 | Recorded Rows: 19,684,236
-- =============================================================================
CREATE TABLE SIEBEL.S_ASSET (
    ROW_ID VARCHAR2(15 CHAR) NOT NULL,
    PAR_ROW_ID VARCHAR2(15 CHAR),
    ASSET_NUM VARCHAR2(100 CHAR) NOT NULL,
    BU_ID VARCHAR2(15 CHAR) DEFAULT '0-R9NH' NOT NULL,
    OWNER_ACCNT_ID VARCHAR2(15 CHAR) NOT NULL,
    PROD_ID VARCHAR2(15 CHAR) NOT NULL,
    STATUS_CD VARCHAR2(30 CHAR)
);

CREATE UNIQUE INDEX SIEBEL.S_ASSET_P1 ON SIEBEL.S_ASSET (ROW_ID);
CREATE INDEX SIEBEL.S_ASSET_M5 ON SIEBEL.S_ASSET (BU_ID, ASSET_NUM);

COMMENT ON COLUMN SIEBEL.S_ASSET.ASSET_NUM IS 'Serial/Asset unique tracking identifier';

CREATE OR REPLACE PACKAGE SIEBEL.PKG_CATALIST_SYNC AS
    PROCEDURE sync_installed_base(p_batch_id IN VARCHAR2);
    FUNCTION validate_msisdn(p_msisdn IN VARCHAR2) RETURN BOOLEAN;
END PKG_CATALIST_SYNC;

CREATE SEQUENCE SIEBEL.SEQ_ASSET_NUM START WITH 1000;
CREATE OR REPLACE TRIGGER SIEBEL.TRG_ASSET_AUDIT BEFORE UPDATE ON SIEBEL.S_ASSET BEGIN NULL; END;
    "#;
    parsers::parse_plsql("catalist/ddl/s_asset.sql", oracle_sql, &mut graph);

    // Verify table, rows, indexes
    let asset_tbl = graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET").expect("S_ASSET must exist");
    assert_eq!(asset_tbl.metadata.get("recorded_rows").map(|s| s.as_str()), Some("19684236"));
    assert!(asset_tbl.metadata.get("indexed_columns").unwrap().contains("ROW_ID"));
    assert!(asset_tbl.metadata.get("indexed_columns").unwrap().contains("BU_ID"));

    // Verify columns & comments
    let asset_num_col = graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET.ASSET_NUM").expect("ASSET_NUM must exist");
    assert_eq!(asset_num_col.docstring.as_deref(), Some("Serial/Asset unique tracking identifier"));

    // Verify implicit Siebel FK heuristics (BU_ID -> S_BU, OWNER_ACCNT_ID -> S_ORG_EXT, PROD_ID -> S_PROD_INT, PAR_ROW_ID -> S_ASSET)
    let bu_col = graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET.BU_ID").unwrap();
    assert_eq!(bu_col.metadata.get("references_table").unwrap(), "S_BU");
    let accnt_col = graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET.OWNER_ACCNT_ID").unwrap();
    assert_eq!(accnt_col.metadata.get("references_table").unwrap(), "S_ORG_EXT");
    let prod_col = graph.get_entity("catalist/ddl/s_asset.sql::S_ASSET.PROD_ID").unwrap();
    assert_eq!(prod_col.metadata.get("references_table").unwrap(), "S_PROD_INT");

    // Package, Sequence, Trigger
    assert!(graph.get_entity("catalist/ddl/s_asset.sql::pkg::SIEBEL.PKG_CATALIST_SYNC").is_some());
    assert!(graph.get_entity("catalist/ddl/s_asset.sql::SIEBEL.PKG_CATALIST_SYNC.sync_installed_base").is_some());
    assert!(graph.get_entity("catalist/ddl/s_asset.sql::SIEBEL.PKG_CATALIST_SYNC.validate_msisdn").is_some());
    assert!(graph.get_entity("catalist/ddl/s_asset.sql::SIEBEL.SEQ_ASSET_NUM").is_some());
    assert!(graph.get_entity("catalist/ddl/s_asset.sql::SIEBEL.TRG_ASSET_AUDIT").is_some());

    // 10.2 PostgreSQL Dialect DDL
    let postgres_sql = r#"
        CREATE TABLE IF NOT EXISTS sf_contact (
            id CHAR(18) PRIMARY KEY,
            account_id CHAR(18) REFERENCES sf_account(id),
            email VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_sf_contact_email ON sf_contact(email);
    "#;
    parsers::parse_plsql("database/postgres_contacts.sql", postgres_sql, &mut graph);
    assert!(graph.get_entity("database/postgres_contacts.sql::sf_contact").is_some());
    assert!(graph.get_entity("database/postgres_contacts.sql::sf_contact.email").is_some());

    // 10.3 MySQL Dialect DDL
    let mysql_sql = r#"
        CREATE TABLE IF NOT EXISTS `telco_cdr` (
            `cdr_id` BIGINT AUTO_INCREMENT PRIMARY KEY,
            `caller_msisdn` VARCHAR(30) NOT NULL,
            `duration_sec` INT DEFAULT 0
        );
    "#;
    parsers::parse_plsql("database/mysql_cdr.sql", mysql_sql, &mut graph);
    assert!(graph.get_entity("database/mysql_cdr.sql::telco_cdr").is_some());
    assert!(graph.get_entity("database/mysql_cdr.sql::telco_cdr.caller_msisdn").is_some());

    // 10.4 SQLite Dialect DDL
    let sqlite_sql = r#"
        CREATE TABLE IF NOT EXISTS app_state (
            key TEXT PRIMARY KEY,
            val TEXT NOT NULL
        );
    "#;
    parsers::parse_plsql("database/sqlite_state.sql", sqlite_sql, &mut graph);
    assert!(graph.get_entity("database/sqlite_state.sql::app_state").is_some());
}

// =============================================================================
// 11. ERLANG TELECOM & REBAR3 PARSER TESTS (.erl, .hrl, .app, .app.src, rebar.config)
// =============================================================================
#[test]
fn test_comprehensive_erlang_and_rebar_parser() {
    let mut graph = CodeGraph::new();

    // 11.1 Erlang Module (.erl)
    let erl_code = r#"
        -module(sip_call_worker).
        -behaviour(gen_server).

        -include("sip_session.hrl").
        -include_lib("kernel/include/file.hrl").

        -record(worker_state, {session_id, caller, callee, state = idle}).
        -define(MAX_CALL_DURATION, 7200).
        -type call_status() :: active | terminated | on_hold.

        %% API
        -export([start_link/1, terminate_call/2]).
        %% gen_server callbacks
        -export([init/1, handle_call/3, handle_cast/2, terminate/2]).

        start_link(SessionId) ->
            gen_server:start_link(?MODULE, [SessionId], []).

        terminate_call(Pid, Reason) ->
            gen_server:call(Pid, {terminate, Reason}).

        init([SessionId]) ->
            {ok, #worker_state{session_id = SessionId}}.

        handle_call({terminate, Reason}, _From, State) ->
            lager:info("Terminating session ~p: ~p", [State#worker_state.session_id, Reason]),
            {reply, ok, State}.

        handle_cast(_Msg, State) ->
            {noreply, State}.

        terminate(_Reason, _State) ->
            ok.
    "#;
    parsers::parse_erlang_module("apps/sip/src/sip_call_worker.erl", erl_code, &mut graph);

    let prefix = "apps/sip/src/sip_call_worker.erl";
    assert!(graph.get_entity(&format!("{}::sip_call_worker", prefix)).is_some());
    assert!(graph.get_entity("otp::gen_server").is_some());
    let mod_neighbors = graph.neighbors(&format!("{}::sip_call_worker", prefix), petgraph::Direction::Outgoing);
    assert!(mod_neighbors.iter().any(|(e, k)| **k == EdgeKind::Implements && e.id == "otp::gen_server"));
    assert!(graph.get_entity(&format!("{}::record::worker_state", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::macro::MAX_CALL_DURATION", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::type::call_status", prefix)).is_some());

    // Functions with arity calculation
    assert!(graph.get_entity(&format!("{}::start_link/1", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::terminate_call/2", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::init/1", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::handle_call/3", prefix)).is_some());
    assert!(graph.get_entity(&format!("{}::terminate/2", prefix)).is_some());

    // Cross-module calls
    assert!(graph.get_entity("erlang::gen_server").is_some());
    assert!(graph.get_entity("erlang::lager").is_some());
    let call_neighbors = graph.neighbors(&format!("{}::sip_call_worker", prefix), petgraph::Direction::Outgoing);
    assert!(call_neighbors.iter().any(|(e, k)| **k == EdgeKind::Calls && e.id == "erlang::gen_server"));

    // 11.2 Erlang Header (.hrl)
    let hrl_code = r#"
        -record(sip_session, {session_id, sdp, codec, start_time}).
        -define(DEFAULT_CODEC, "G711A").
    "#;
    parsers::parse_erlang_module("apps/sip/include/sip_session.hrl", hrl_code, &mut graph);
    assert!(graph.get_entity("apps/sip/include/sip_session.hrl::record::sip_session").is_some());
    assert!(graph.get_entity("apps/sip/include/sip_session.hrl::macro::DEFAULT_CODEC").is_some());

    // 11.3 Erlang Application Descriptor (.app.src)
    let app_code = r#"
        {application, sip_engine,
         [{description, "High Performance SIP Routing Engine"},
          {vsn, "2.4.0"},
          {modules, [sip_call_worker, sip_router]},
          {registered, [sip_router_sup]},
          {applications, [kernel, stdlib, lager, cowboy]},
          {mod, {sip_engine_app, []}}
         ]}.
    "#;
    parsers::parse_erlang_app("apps/sip/src/sip_engine.app.src", app_code, &mut graph);
    assert!(graph.get_entity("apps/sip/src/sip_engine.app.src::application::sip_engine").is_some());

    // 11.4 Rebar3 Build Config (rebar.config)
    let rebar_code = r#"
        {erl_opts, [debug_info]}.
        {deps, [
            {cowboy, "2.9.0"},
            {jsx, "3.1.0"},
            {lager, {git, "https://github.com/erlang-lager/lager.git", {tag, "3.9.2"}}}
        ]}.
    "#;
    parsers::parse_rebar_config("rebar.config", rebar_code, &mut graph);
    assert!(graph.get_entity("rebar.config::config").is_some());
    assert!(graph.get_entity("rebar.config::dep::cowboy").is_some());
    assert!(graph.get_entity("rebar.config::dep::jsx").is_some());
    assert!(graph.get_entity("rebar.config::dep::lager").is_some());
}

// =============================================================================
// 12. APIGEE API MANAGEMENT PARSER TESTS (Proxies, Endpoints, Policies)
// =============================================================================
#[test]
fn test_comprehensive_apigee_parser() {
    let mut graph = CodeGraph::new();

    // 12.1 Root APIProxy Manifest
    let apiproxy_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <APIProxy revision="3" name="telco-customer-v1">
        <DisplayName>Telco Customer Management API</DisplayName>
        <Description>REST interface for SIM and Profile queries</Description>
        <TargetEndpoints>
            <TargetEndpoint>default</TargetEndpoint>
        </TargetEndpoints>
        <ProxyEndpoints>
            <ProxyEndpoint>default</ProxyEndpoint>
        </ProxyEndpoints>
        <Policies>
            <Policy>VAK-VerifyKey</Policy>
            <Policy>SA-SpikeArrest</Policy>
        </Policies>
    </APIProxy>
    "#;
    parsers::parse_apigee("apiproxy/telco-customer-v1.xml", apiproxy_xml, &mut graph);
    assert!(graph.get_entity("apiproxy::telco-customer-v1").is_some());

    // 12.2 ProxyEndpoint XML
    let proxy_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <ProxyEndpoint name="default">
        <HTTPProxyConnection>
            <BasePath>/v1/customers</BasePath>
        </HTTPProxyConnection>
        <RouteRule name="default">
            <TargetEndpoint>target-billing-backend</TargetEndpoint>
        </RouteRule>
        <PreFlow name="PreFlow">
            <Request>
                <Step><Name>VAK-VerifyKey</Name></Step>
                <Step><Name>Q-Quota500PerMin</Name></Step>
            </Request>
        </PreFlow>
    </ProxyEndpoint>
    "#;
    parsers::parse_apigee("apiproxy/proxies/default.xml", proxy_xml, &mut graph);
    assert!(graph.get_entity("apiproxy::proxy::default").is_some());
    assert!(graph.get_entity("apiproxy::target::target-billing-backend").is_some());
    assert!(graph.get_entity("apiproxy::policy::VAK-VerifyKey").is_some());
    assert!(graph.get_entity("apiproxy::policy::Q-Quota500PerMin").is_some());

    // 12.3 Policy Definitions
    let vak_policy = r#"<?xml version="1.0" encoding="UTF-8"?>
    <VerifyAPIKey async="false" continueOnError="false" enabled="true" name="VAK-VerifyKey">
        <APIKey ref="request.header.x-api-key"/>
    </VerifyAPIKey>
    "#;
    parsers::parse_apigee("apiproxy/policies/VAK-VerifyKey.xml", vak_policy, &mut graph);
    assert!(graph.get_entity("apiproxy::policy::VAK-VerifyKey").is_some());

    let sa_policy = r#"<?xml version="1.0" encoding="UTF-8"?>
    <SpikeArrest async="false" continueOnError="false" enabled="true" name="SA-SpikeArrest">
        <Rate>100ps</Rate>
    </SpikeArrest>
    "#;
    parsers::parse_apigee("apiproxy/policies/SA-SpikeArrest.xml", sa_policy, &mut graph);
    assert!(graph.get_entity("apiproxy::policy::SA-SpikeArrest").is_some());
}

// =============================================================================
// 13. XML, XSD SCHEMAS, MAVEN POM, AND HTML WEB MARKUP
// =============================================================================
#[test]
fn test_comprehensive_xml_xsd_pom_and_html_parser() {
    let mut graph = CodeGraph::new();

    // 13.1 Maven POM XML (pom.xml)
    let pom_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <project xmlns="http://maven.apache.org/POM/4.0.0">
        <modelVersion>4.0.0</modelVersion>
        <groupId>com.telco.bss</groupId>
        <artifactId>order-management</artifactId>
        <version>3.1.0-RELEASE</version>
        <dependencies>
            <dependency id="spring-boot-starter-web">
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-starter-web</artifactId>
            </dependency>
        </dependencies>
    </project>
    "#;
    parsers::parse_xml("pom.xml", pom_xml, &mut graph);
    assert!(graph.get_entity("pom.xml::dependency#spring-boot-starter-web::8").is_some());

    // 13.2 Generic XML / XSD Schema
    let xsd_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
        <xs:element name="SubscriberRecord" type="xs:string" id="elem_subscriber_record"/>
        <xs:complexType name="SubscriberDetails" id="type_subscriber_details">
            <xs:sequence>
                <xs:element name="MSISDN" type="xs:string" id="elem_msisdn"/>
            </xs:sequence>
        </xs:complexType>
    </xs:schema>
    "#;
    parsers::parse_xml("schemas/subscriber.xsd", xsd_xml, &mut graph);
    assert!(graph.get_entity("schemas/subscriber.xsd::xs:element#SubscriberRecord::3").is_some());
    assert!(graph.get_entity("schemas/subscriber.xsd::xs:complexType#SubscriberDetails::4").is_some());

    // 13.3 Semantic HTML Web Markup (.html)
    let html_content = r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <title>Telco Self-Service Portal</title>
        <script src="/static/bundle.js"></script>
    </head>
    <body>
        <header class="top-nav">Header</header>
        <main id="portal-root">
            <form id="sim-activation-form" action="/activate">
                <input type="text" name="iccid" />
            </form>
            <c-asset-hierarchy-view></c-asset-hierarchy-view>
        </main>
    </body>
    </html>
    "#;
    parsers::parse_html("public/portal.html", html_content, &mut graph);
    assert!(graph.get_entity("public/portal.html::title::5").is_some());
    assert!(graph.get_entity("public/portal.html::script::6").is_some());
    assert!(graph.get_entity("public/portal.html::header.top-nav::9").is_some());
    assert!(graph.get_entity("public/portal.html::main#portal-root::10").is_some());
    assert!(graph.get_entity("public/portal.html::form#sim-activation-form::11").is_some());
    assert!(graph.get_entity("public/portal.html::c-asset-hierarchy-view::14").is_some());
}

// =============================================================================
// 14. MAKEFILE & BUILD AUTOMATION (.mk, Makefile, GNUmakefile)
// =============================================================================
#[test]
fn test_comprehensive_makefile_parser() {
    let mut graph = CodeGraph::new();
    let makefile_content = r#"
# Core Build Pipeline
CC := gcc
CFLAGS := -O3 -Wall

all: compile test lint

compile:
	$(CC) $(CFLAGS) -c src/main.c

test: compile
	./run_tests.sh

lint:
	clang-tidy src/*.c

clean:
	rm -rf *.o build/

.PHONY: all compile test lint clean
    "#;

    parsers::parse_makefile("Makefile", makefile_content, &mut graph);

    assert!(graph.get_entity("Makefile::target::all").is_some());
    assert!(graph.get_entity("Makefile::target::compile").is_some());
    assert!(graph.get_entity("Makefile::target::test").is_some());
    assert!(graph.get_entity("Makefile::target::lint").is_some());
    assert!(graph.get_entity("Makefile::target::clean").is_some());
    assert!(graph.get_entity("Makefile::target::.PHONY").is_some());

    // Check prerequisites edges
    let all_deps = graph.neighbors("Makefile::target::all", petgraph::Direction::Outgoing);
    let all_dep_ids: Vec<String> = all_deps.iter().filter(|(_, k)| **k == EdgeKind::DependsOn).map(|(e, _)| e.id.clone()).collect();
    assert!(all_dep_ids.contains(&"Makefile::target::compile".to_string()));
    assert!(all_dep_ids.contains(&"Makefile::target::test".to_string()));
    assert!(all_dep_ids.contains(&"Makefile::target::lint".to_string()));

    let test_deps = graph.neighbors("Makefile::target::test", petgraph::Direction::Outgoing);
    let test_dep_ids: Vec<String> = test_deps.iter().filter(|(_, k)| **k == EdgeKind::DependsOn).map(|(e, _)| e.id.clone()).collect();
    assert!(test_dep_ids.contains(&"Makefile::target::compile".to_string()));
}

// =============================================================================
// 15. SHEBANG DETECTION & SCRIPT ROUTING (Bash, Python, Node, Bun, Deno, Escript)
// =============================================================================
#[test]
fn test_comprehensive_shebang_detection_and_routing() {
    // 15.1 Direct shebang tests
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env bash\necho 1"), Some("bash"));
    assert_eq!(Indexer::detect_shebang(b"#!/bin/bash\necho 1"), Some("bash"));
    assert_eq!(Indexer::detect_shebang(b"#!/bin/sh\necho 1"), Some("bash"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env zsh\necho 1"), Some("bash"));

    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env python3\nimport sys"), Some("python"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/python\nimport sys"), Some("python"));

    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env node\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env bun\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env deno\nconsole.log(1)"), Some("typescript"));
    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env ts-node\nconsole.log(1)"), Some("typescript"));

    assert_eq!(Indexer::detect_shebang(b"#!/usr/bin/env escript\nmain(_) -> ok."), Some("erlang"));
    assert_eq!(Indexer::detect_shebang(b"// normal file without shebang"), None);

    // 15.2 Extension routing validation
    let extensions = [
        ("rs", true), ("py", true), ("ts", true), ("tsx", true), ("js", true), ("jsx", true),
        ("mjs", true), ("cjs", true), ("go", true), ("c", true), ("h", true), ("cpp", true),
        ("cc", true), ("cxx", true), ("hpp", true), ("hxx", true), ("java", true), ("cls", true),
        ("trigger", true), ("soql", true), ("cmp", true), ("app", true), ("evt", true), ("intf", true),
        ("page", true), ("component", true), ("xml", true), ("xsd", true), ("pom", true),
        ("html", true), ("htm", true), ("sql", true), ("pls", true), ("pks", true), ("pkb", true),
        ("prc", true), ("fnc", true), ("trg", true), ("sh", true), ("bash", true), ("zsh", true),
        ("mk", true), ("erl", true), ("hrl", true), ("escript", true),
        ("unknown_xyz", false), ("bin", false), ("exe", false), ("so", false),
    ];

    for (ext, expected) in extensions {
        let file_name = format!("sample.{}", ext);
        assert_eq!(Indexer::is_supported_file(&file_name, ext), expected, "Extension .{} support check failed", ext);
    }
}

// =============================================================================
// 16. END-TO-END REPOSITORY RECOGNITION ACROSS ALL LANGUAGES & DIALECTS
// =============================================================================
#[tokio::test]
async fn test_end_to_end_indexing_across_all_languages_and_dialects() {
    let temp_repo = std::env::temp_dir().join(format!(
        "rumin_all_langs_test_{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir_all(&temp_repo).unwrap();

    let files = [
        ("src/main.rs", "pub fn main() {}"),
        ("services/api.py", "class ApiService:\n    def handle(self):\n        pass"),
        ("src/types.ts", "export interface UserDto { id: string; }"),
        ("src/App.tsx", "export const App = () => <div>Hello</div>;"),
        ("lib/legacy.js", "function legacyCalc() { return 1; }"),
        ("lib/util.mjs", "export function mjsUtil() {}"),
        ("pkg/server.go", "package server\nfunc Listen() {}"),
        ("include/core.h", "struct CoreConfig { int id; };"),
        ("src/core.c", "#include \"core.h\"\nvoid run_core() {}"),
        ("src/engine.cpp", "class Engine { public: void start() {} };"),
        ("src/App.java", "package com.telco;\npublic class App { public void exec() {} }"),
        ("force-app/main/default/classes/AccountService.cls", "public class AccountService { public static void updateAccount() {} }"),
        ("force-app/main/default/triggers/OrderTrigger.trigger", "trigger OrderTrigger on Order (before insert) {}"),
        ("queries/orders.soql", "SELECT Id, Name FROM Order"),
        ("force-app/main/default/aura/Comp.cmp", "<aura:component controller=\"AccountService\"></aura:component>"),
        ("force-app/main/default/pages/Invoice.page", "<apex:page controller=\"AccountService\"></apex:page>"),
        ("force-app/main/default/objects/Asset/Asset.object-meta.xml", "<CustomObject><label>Asset</label></CustomObject>"),
        ("force-app/main/default/objects/Asset/fields/Serial__c.field-meta.xml", "<CustomField><fullName>Serial__c</fullName><type>Text</type></CustomField>"),
        ("force-app/main/default/customMetadata/Config__mdt.Default.md-meta.xml", "<CustomMetadata><label>Default</label></CustomMetadata>"),
        ("omnistudio/OmniIntegrationProcedure/FetchData.oip-meta.xml", "<OmniIntegrationProcedure><name>FetchData</name></OmniIntegrationProcedure>"),
        ("catalist/ddl/s_asset.sql", "-- Columns: 10 | Recorded Rows: 5,000,000\nCREATE TABLE SIEBEL.S_ASSET (ROW_ID VARCHAR2(15) NOT NULL, BU_ID VARCHAR2(15) NOT NULL);"),
        ("apps/telecom/src/worker.erl", "-module(worker).\n-export([start/0]).\nstart() -> ok."),
        ("apps/telecom/include/record.hrl", "-record(telecom_cdr, {id, msisdn})."),
        ("apps/telecom/src/telecom.app.src", "{application, telecom, [{vsn, \"1.0\"}, {modules, [worker]}]}."),
        ("rebar.config", "{deps, [cowboy]}."),
        ("apiproxy/telco.xml", "<APIProxy name=\"telco-proxy\"></APIProxy>"),
        ("pom.xml", "<project><artifactId>telco-service</artifactId></project>"),
        ("schemas/msg.xsd", "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"><xs:element name=\"msg\"/></xs:schema>"),
        ("public/index.html", "<!DOCTYPE html><html><head><title>App</title></head><body><main id=\"root\"></main></body></html>"),
        ("Makefile", "all: build\nbuild:\n\techo 1"),
        ("scripts/deploy.sh", "#!/bin/bash\necho deploying"),
        ("vb/BillingService.vb", "Public Class BillingService\nPublic Sub ProcessBilling() End Sub\nEnd Class"),
        ("vba/ModExport.bas", "Attribute VB_Name = \"ModExport\"\nPublic Sub ExportSheet()\nDim xl As Object\nSet xl = CreateObject(\"Excel.Application\")\nEnd Sub"),
        ("scripts/admin.vbs", "Sub RunAdminTask() End Sub"),
    ];

    for (rel_path, content) in files {
        let abs_path = temp_repo.join(rel_path);
        fs::create_dir_all(abs_path.parent().unwrap()).unwrap();
        fs::write(&abs_path, content).unwrap();
    }

    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new(&temp_repo.join("worker.py"));
    let indexer = Indexer::new(&temp_repo);

    let indexed_count = indexer.index_repository(&mut graph, &sidecar).await.unwrap();
    assert_eq!(indexed_count, files.len(), "All 34 files across languages must be indexed");

    // 1. Verify Rust
    assert!(graph.find_node_by_name_or_id("main").is_some());

    // 2. Verify Python
    assert!(graph.find_node_by_name_or_id("ApiService").is_some());

    // 3. Verify TypeScript / TSX
    assert!(graph.find_node_by_name_or_id("UserDto").is_some());
    assert!(graph.find_node_by_name_or_id("App").is_some());

    // 4. Verify Go
    assert!(graph.find_node_by_name_or_id("Listen").is_some());

    // 5. Verify C / C++
    assert!(graph.find_node_by_name_or_id("CoreConfig").is_some());
    assert!(graph.find_node_by_name_or_id("run_core").is_some());
    assert!(graph.find_node_by_name_or_id("Engine").is_some());

    // 6. Verify Java
    assert!(graph.find_node_by_name_or_id("App").is_some());

    // 7. Verify Salesforce Apex, Trigger, SOQL, Aura, Page
    assert!(graph.find_node_by_name_or_id("AccountService").is_some());
    assert!(graph.find_node_by_name_or_id("OrderTrigger").is_some());
    assert!(graph.find_node_by_name_or_id("Comp").is_some());
    assert!(graph.find_node_by_name_or_id("Invoice").is_some());

    // 8. Verify SObject and Field Metadata
    assert!(graph.find_node_by_name_or_id("Asset").is_some());
    assert!(graph.find_node_by_name_or_id("Serial__c").is_some());

    // 9. Verify OmniStudio
    assert!(graph.find_node_by_name_or_id("FetchData").is_some());

    // 10. Verify SQL / PLSQL
    assert!(graph.find_node_by_name_or_id("S_ASSET").is_some());

    // 11. Verify Erlang & Rebar3
    assert!(graph.find_node_by_name_or_id("worker").is_some());
    assert!(graph.find_node_by_name_or_id("cowboy").is_some());

    // 12. Verify Apigee
    assert!(graph.find_node_by_name_or_id("telco-proxy").is_some());

    // 13. Verify Makefile
    assert!(graph.find_node_by_name_or_id("all").is_some());
    assert!(graph.find_node_by_name_or_id("build").is_some());

    // 14. Verify Visual Basic (.vb), VBA (.bas), VBScript (.vbs), and COM
    assert!(graph.find_node_by_name_or_id("BillingService").is_some());
    assert!(graph.find_node_by_name_or_id("ProcessBilling").is_some());
    assert!(graph.find_node_by_name_or_id("ModExport").is_some());
    assert!(graph.find_node_by_name_or_id("ExportSheet").is_some());
    assert!(graph.find_node_by_name_or_id("RunAdminTask").is_some());
    assert!(graph.get_entity("com::Excel.Application").is_some());

    // 15. Hippocampal Engine Associative Search across Multi-Lingual Graph
    let hippocampal = HippocampalEngine::new(0.20);
    let tokens = HippocampalEngine::tokenize("AccountService");
    let seeds = hippocampal.find_seed_nodes(&graph, &tokens);
    let ppr = hippocampal.personalized_pagerank(&graph, &seeds);

    let top_ids: Vec<&str> = ppr.iter().take(5).map(|(id, _)| id.as_str()).collect();
    assert!(top_ids.iter().any(|id| id.contains("AccountService")), "AccountService must be surfaced in top PPR: {:?}", top_ids);

    let _ = fs::remove_dir_all(&temp_repo);
}
