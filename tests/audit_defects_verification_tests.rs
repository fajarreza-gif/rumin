use ai_rumin::db::MemoryDb;
use ai_rumin::episodic::EpisodicEngine;
use ai_rumin::graph::CodeGraph;
use ai_rumin::hippocampal::HippocampalEngine;
use ai_rumin::hook::CircuitBreaker;
use ai_rumin::indexer::{FileManifest, Indexer};
use ai_rumin::mcp::MCPServer;
use ai_rumin::parsers;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::trace::TraceIngestor;
use ai_rumin::types::{CodeEntity, ContextBudget, EdgeKind, EntityType};
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;

#[test]
fn test_def_01_and_02_bincode_checkpoint_and_diff_with_metadata() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_cp_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();
    let cp_file = temp_dir.join("checkpoint.bin");

    let mut graph = CodeGraph::new();
    let mut entity1 = CodeEntity::new("mod1::func1", "func1", EntityType::Function, "src/mod1.rs", 1, 15);
    entity1.metadata.insert("arity".to_string(), "2".to_string());
    entity1.metadata.insert("visibility".to_string(), "pub".to_string());
    graph.add_entity(entity1);

    let mut entity2 = CodeEntity::new("mod1::StructA", "StructA", EntityType::Struct, "src/mod1.rs", 16, 30);
    entity2.metadata.insert("derives".to_string(), "Debug, Clone".to_string());
    graph.add_entity(entity2);
    graph.add_edge("mod1::StructA", "mod1::func1", EdgeKind::Contains);

    // Save binary checkpoint (atomic write DEF-02)
    assert!(graph.save_binary_checkpoint(&cp_file).is_ok());
    assert!(cp_file.exists());

    // Load binary checkpoint (bincode deserialize_any resilience DEF-01)
    let loaded = CodeGraph::load_binary_checkpoint(&cp_file).expect("Must load binary checkpoint without deserialize_any error");
    assert_eq!(loaded.node_count(), 2);
    assert_eq!(loaded.edge_count(), 1);

    let ent1 = loaded.get_entity("mod1::func1").unwrap();
    assert_eq!(ent1.metadata.get("arity"), Some(&"2".to_string()));
    assert_eq!(ent1.metadata.get("visibility"), Some(&"pub".to_string()));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_def_07_and_3_3_synthetic_node_protection_and_relocation() {
    let mut graph = CodeGraph::new();

    // Add synthetic canonical nodes
    let sobj = CodeEntity::new("sobject::Account", "Account", EntityType::Table, "src/Account.cls", 1, 1);
    graph.add_entity(sobj);

    let erl_ext = CodeEntity::new("erlang::lists", "lists", EntityType::Module, "src/worker.erl", 1, 1);
    graph.add_entity(erl_ext);

    // Add regular client file entity
    let client_ent = CodeEntity::new("src/Account.cls::MyClass", "MyClass", EntityType::Class, "src/Account.cls", 1, 50);
    graph.add_entity(client_ent);

    assert_eq!(graph.node_count(), 3);

    // Removing client file must NOT delete sobject:: or erlang:: synthetic nodes (DEF-07)
    graph.remove_file("src/Account.cls");
    assert!(graph.get_entity("sobject::Account").is_some());
    assert!(graph.get_entity("erlang::lists").is_some());
    assert!(graph.get_entity("src/Account.cls::MyClass").is_none());

    // Relocating an entity cleans up old file association (DEF-3.3)
    let moved_ent = CodeEntity::new("sym::moved", "moved", EntityType::Function, "src/old_path.rs", 1, 10);
    graph.add_entity(moved_ent);
    assert!(graph.get_entity("sym::moved").is_some());

    let updated_loc = CodeEntity::new("sym::moved", "moved", EntityType::Function, "src/new_path.rs", 1, 10);
    graph.add_entity(updated_loc);

    // Removing old path must NOT remove the moved entity
    graph.remove_file("src/old_path.rs");
    assert!(graph.get_entity("sym::moved").is_some());
}

#[test]
fn test_def_06_and_17_sqlite_mutex_and_episodic_seq() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_ep_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let engine = EpisodicEngine::new(db.clone());

    // Record episodes
    let syms = vec!["sym_a".to_string()];
    let ep1 = engine.record_episode("Fix issue 1", "fixed bug", true, &syms, &[], &[]).unwrap();
    let syms2 = vec!["sym_b".to_string()];
    let ep2 = engine.record_episode("Fix issue 2", "fixed bug 2", true, &syms2, &[], &[]).unwrap();
    let syms3 = vec!["sym_c".to_string()];
    let ep3 = engine.record_episode("Fix issue 3", "fixed bug 3", true, &syms3, &[], &[]).unwrap();

    assert_eq!(ep1.id, "ep_1");
    assert_eq!(ep2.id, "ep_2");
    assert_eq!(ep3.id, "ep_3");

    // Delete ep_3 (tail record)
    db.conn().execute("DELETE FROM episodes WHERE id = 'ep_3';", []).unwrap();

    // Next recorded episode MUST be ep_4, not reusing ep_3 (DEF-17)
    let syms4 = vec!["sym_d".to_string()];
    let ep4 = engine.record_episode("Fix issue 4", "fixed bug 4", true, &syms4, &[], &[]).unwrap();
    assert_eq!(ep4.id, "ep_4");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_def_14_and_15_and_5_3_hippocampal_budget_and_root_and_episodes() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_hippo_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let code_file = src_dir.join("main.rs");
    fs::write(&code_file, "fn target_symbol() {\n    let x = 42;\n    println!(\"{}\", x);\n}\n").unwrap();

    let mut graph = CodeGraph::new();
    let entity = CodeEntity::new("src/main.rs::target_symbol", "target_symbol", EntityType::Function, "src/main.rs", 1, 4);
    graph.add_entity(entity);

    let mut ep_entity = CodeEntity::new("episode::ep_1", "Test Episode", EntityType::Rule, ".rumin/episodes/ep_1.md", 1, 1);
    ep_entity.docstring = Some("Full markdown content for the episode solution".to_string());
    graph.add_entity(ep_entity);

    let engine = HippocampalEngine::default();

    // 1. Strict token limit enforcement (DEF-14)
    let strict_budget = ContextBudget {
        total_token_limit: 30, // Extremely small token budget
        max_symbols: 10,
        include_signatures: true,
        expand_bodies: true,
        ..Default::default()
    };
    let res = engine.query_with_root(&graph, "target_symbol", &strict_budget, Some(&temp_dir));
    assert!(res.estimated_tokens <= strict_budget.total_token_limit + 50);

    // 2. Working directory independence with repo_root (DEF-15)
    let full_budget = ContextBudget {
        total_token_limit: 1000,
        max_symbols: 10,
        include_signatures: true,
        expand_bodies: true,
        ..Default::default()
    };
    let res2 = engine.query_with_root(&graph, "target_symbol", &full_budget, Some(&temp_dir));
    assert!(res2.formatted_context.contains("let x = 42;"));

    // 3. Episodic docstring expansion without phantom file panic (DEF-5.3)
    let res3 = engine.query_with_root(&graph, "Test Episode", &full_budget, Some(&temp_dir));
    assert!(res3.formatted_context.contains("Full markdown content for the episode solution"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_def_16_and_6_2_trace_utf8_char_boundary_safety() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_trace_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let ingestor = TraceIngestor::new(db);

    // Construct a log file where byte index 80 cuts directly through a 4-byte UTF-8 emoji
    let prefix = "A".repeat(78);
    let log_line = format!("{}🤖🚀📦 Testing multibyte UTF-8 characters past the 80 byte slice", prefix);
    let log_path = temp_dir.join("test.log");
    fs::write(&log_path, format!("{}\nSecond line with Japanese: こんにちは世界！\n", log_line)).unwrap();

    // Must not panic on char boundary (DEF-16)
    let report = ingestor.ingest_file(&log_path, None).expect("Ingestion must succeed without UTF-8 panic");
    assert_eq!(report.steps_ingested, 2);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_def_08_09_10_11_multi_language_parsers_imports_and_methods() {
    let mut graph = CodeGraph::new();

    // 1. Go: imports & receiver methods (DEF-10, DEF-11)
    let go_code = r#"
package service

import (
    "fmt"
    "net/http"
)

type Server struct {}

func (s *Server) Start() {}
"#;
    parsers::parse_go("service/server.go", go_code, &mut graph);
    assert!(graph.get_entity("service/server.go::file").is_some());
    assert!(graph.get_entity("go::fmt").is_some());
    assert!(graph.get_entity("service/server.go::Server.Start").is_some());
    assert!(graph.has_edge("service/server.go::file", "go::fmt"));
    assert!(graph.has_edge("service/server.go::Server", "service/server.go::Server.Start"));

    // 2. C & C++: #include & methods (DEF-10, DEF-11)
    let c_code = r#"
#include <stdio.h>
#include "my_header.h"
struct Node { int val; };
"#;
    parsers::parse_c("src/node.c", c_code, &mut graph);
    assert!(graph.get_entity("src/node.c::file").is_some());
    assert!(graph.get_entity("c::stdio.h").is_some());
    assert!(graph.has_edge("src/node.c::file", "c::stdio.h"));

    let cpp_code = r#"
#include <iostream>
class Engine {
public:
    void Run();
};
void Engine::Run() {}
"#;
    parsers::parse_cpp("src/engine.cpp", cpp_code, &mut graph);
    assert!(graph.get_entity("src/engine.cpp::file").is_some());
    assert!(graph.get_entity("cpp::iostream").is_some());
    assert!(graph.has_edge("src/engine.cpp::file", "cpp::iostream"));
    assert!(graph.has_edge("src/engine.cpp::Engine", "src/engine.cpp::Engine::Run"));

    // 3. Rust: file entity, imports, and cross-file trait resolution (DEF-08, DEF-09)
    let rust_code = r#"
use std::sync::Arc;
pub struct Worker;
impl Display for Worker {}
"#;
    parsers::parse_rust("src/worker.rs", rust_code, &mut graph);
    assert!(graph.get_entity("src/worker.rs::file").is_some());
    assert!(graph.get_entity("rust::std::sync::Arc").is_some());
    assert!(graph.has_edge("src/worker.rs::file", "rust::std::sync::Arc"));
    assert!(graph.has_edge("src/worker.rs::Worker", "rust::trait::Display"));

    // 4. Python: file entity & imports (DEF-09)
    let py_code = r#"
import os
from collections import defaultdict

class App:
    def run(self):
        pass
"#;
    parsers::parse_python("app.py", py_code, &mut graph);
    assert!(graph.get_entity("app.py::file").is_some());
    assert!(graph.get_entity("py::os").is_some());
    assert!(graph.has_edge("app.py::file", "py::os"));
    assert!(graph.has_edge("app.py::App", "app.py::App.run"));

    // 5. TypeScript: imports & class methods (DEF-10, DEF-11)
    let ts_code = r#"
import { Router } from "express";

export class ApiHandler {
    handleRequest() {
        return 200;
    }
}
"#;
    parsers::parse_typescript("src/api.ts", ts_code, &mut graph);
    assert!(graph.get_entity("src/api.ts::file").is_some());
    assert!(graph.get_entity("ts::express").is_some());
    assert!(graph.has_edge("src/api.ts::file", "ts::express"));
    assert!(graph.has_edge("src/api.ts::ApiHandler", "src/api.ts::ApiHandler.handleRequest"));

    // 6. Java: imports & class methods (DEF-10, DEF-11)
    let java_code = r#"
package com.example;
import java.util.List;

public class Manager {
    public void processItems() {}
}
"#;
    parsers::parse_java("src/Manager.java", java_code, &mut graph);
    assert!(graph.get_entity("src/Manager.java::file").is_some());
    assert!(graph.get_entity("java::java.util.List").is_some());
    assert!(graph.has_edge("src/Manager.java::file", "java::java.util.List"));
    assert!(graph.has_edge("src/Manager.java::Manager", "src/Manager.java::processItems"));

    // 7. PL/SQL: unquoting and package reset on END (DEF-11)
    let plsql_code = r#"
CREATE OR REPLACE PACKAGE "PKG_AUTH" AS
    PROCEDURE "LOGIN";
END "PKG_AUTH";

CREATE OR REPLACE PROCEDURE STANDALONE_PROC AS
BEGIN
    NULL;
END;
"#;
    parsers::parse_plsql("auth.sql", plsql_code, &mut graph);
    assert!(graph.get_entity("auth.sql::pkg::PKG_AUTH").is_some());
    assert!(graph.get_entity("auth.sql::PKG_AUTH.LOGIN").is_some());
    assert!(graph.get_entity("auth.sql::STANDALONE_PROC").is_some());
    assert!(graph.has_edge("auth.sql::pkg::PKG_AUTH", "auth.sql::PKG_AUTH.LOGIN"));
}

#[tokio::test]
async fn test_def_12_and_18_indexer_ruminignore_and_plsql_fallback() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_idx_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir.join("src")).unwrap();
    fs::create_dir_all(&temp_dir.join("ignored_folder")).unwrap();

    // Setup .ruminignore (DEF-18)
    fs::write(temp_dir.join(".ruminignore"), "ignored_folder/\n*.secret\n").unwrap();

    fs::write(temp_dir.join("src/valid.rs"), "pub fn ok() {}").unwrap();
    fs::write(temp_dir.join("src/test.secret"), "password123").unwrap();
    fs::write(temp_dir.join("ignored_folder/file.rs"), "pub fn ignored() {}").unwrap();

    // PL/SQL file (DEF-12)
    fs::write(temp_dir.join("src/test.pls"), "CREATE OR REPLACE PROCEDURE NATIVE_PROC IS BEGIN NULL; END;").unwrap();

    let indexer = Indexer::new(&temp_dir);
    let manifest = FileManifest::default();
    let delta = indexer.compute_delta(&manifest, false, None).unwrap();

    let added_paths: Vec<&str> = delta.added.iter().map(|s| s.0.as_str()).collect();
    assert!(added_paths.contains(&"src/valid.rs"));
    assert!(added_paths.contains(&"src/test.pls"));
    assert!(!added_paths.contains(&"src/test.secret"));
    assert!(!added_paths.iter().any(|p| p.starts_with("ignored_folder/")));

    // Index repository with dummy sidecar (fallback to native PL/SQL parser)
    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new(temp_dir.join("non_existent_sidecar.py"));
    let _ = indexer.index_repository(&mut graph, &sidecar).await;

    assert!(graph.get_entity("src/test.pls::NATIVE_PROC").is_some());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_def_6_6_non_destructive_git_hook_installation() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_hook_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let hooks_dir = temp_dir.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let hook_path = hooks_dir.join("pre-commit");

    // Write existing pre-commit hook (e.g. husky or linter)
    fs::write(&hook_path, "#!/bin/sh\necho 'running existing linter'\nnpm run lint || exit 1\n").unwrap();

    let installed_path = CircuitBreaker::install_git_hook(&temp_dir).unwrap();
    assert_eq!(installed_path, hook_path);

    // Verify backup created
    let backup_path = hooks_dir.join("pre-commit.rumin.bak");
    assert!(backup_path.exists());
    let backup_content = fs::read_to_string(&backup_path).unwrap();
    assert!(backup_content.contains("npm run lint"));

    // Verify hook now contains both original linter AND ai-rumin safety check
    let updated_content = fs::read_to_string(&hook_path).unwrap();
    assert!(updated_content.contains("npm run lint"));
    assert!(updated_content.contains("ai-rumin verify"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_def_05_and_6_7_mcp_switch_repo_watcher_and_consolidate() {
    let temp_dir1 = std::env::temp_dir().join(format!("rumin_test_mcp1_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let temp_dir2 = std::env::temp_dir().join(format!("rumin_test_mcp2_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir1).unwrap();
    fs::create_dir_all(&temp_dir2).unwrap();

    let server = Arc::new(MCPServer::new(temp_dir1.clone(), temp_dir1.join("sidecar.py")));

    // 1. Switch repository (DEF-05)
    let switch_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "rumin_switch_repo",
            "arguments": {
                "repo_path": temp_dir2.to_string_lossy().to_string()
            }
        }
    });
    let resp = server.handle_request_for_test(switch_req).await.unwrap();
    let text = resp.get("result").and_then(|r| r.get("content")).unwrap()[0].get("text").unwrap().as_str().unwrap();
    let val: Value = serde_json::from_str(text).unwrap();
    assert_eq!(val["status"], "switched");

    // 2. Consolidate memory and verify checkpoint save (DEF-6.7)
    let consolidate_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "rumin_consolidate",
            "arguments": {}
        }
    });
    let c_resp = server.handle_request_for_test(consolidate_req).await.unwrap();
    assert!(c_resp.get("error").is_none());

    let cp_path = temp_dir2.join(".rumin/checkpoints/default.bin");
    assert!(cp_path.exists(), "Consolidation must save checkpoint to default.bin in switched repo");

    let _ = fs::remove_dir_all(&temp_dir1);
    let _ = fs::remove_dir_all(&temp_dir2);
}

#[tokio::test]
async fn test_xml_and_metadata_memory_bloat_prevention() {
    let mut graph = CodeGraph::new();

    // 1. XML with anonymous tags vs identified tags
    let xml_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
    <apiVersion>58.0</apiVersion>
    <status>Active</status>
    <bean id="myService" class="com.example.Service" />
    <item name="namedItem" value="123" />
    <label>Some Label</label>
</root>
"#;
    parsers::parse_xml("config/app.xml", xml_content, &mut graph);

    // Only bean#myService and item#namedItem should be indexed
    assert_eq!(graph.node_count(), 2);
    assert!(graph.get_entity("config/app.xml::bean#myService::5").is_some());
    assert!(graph.get_entity("config/app.xml::item#namedItem::6").is_some());
    assert!(graph.get_entity("config/app.xml::apiVersion::3").is_none());
    assert!(graph.get_entity("config/app.xml::status::4").is_none());
    assert!(graph.get_entity("config/app.xml::label::7").is_none());

    // 2. Salesforce DX companion metadata files vs schemas
    let temp_dir = std::env::temp_dir().join(format!("rumin_xml_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let cls_meta = temp_dir.join("MyClass.cls-meta.xml");
    fs::write(&cls_meta, "<ApexClass><apiVersion>58.0</apiVersion><status>Active</status></ApexClass>").unwrap();

    let field_trans_meta = temp_dir.join("Account-es.fieldTranslation-meta.xml");
    fs::write(&field_trans_meta, "<CustomFieldTranslation><label>Cuenta</label></CustomFieldTranslation>").unwrap();

    let obj_meta = temp_dir.join("Account.object-meta.xml");
    fs::write(&obj_meta, "<CustomObject xmlns=\"http://soap.sforce.com/2006/04/metadata\"><label>Account</label></CustomObject>").unwrap();

    let sidecar = SidecarClient::new("sidecar/worker.py");

    let mut sf_graph = CodeGraph::new();
    let res_cls = Indexer::index_single_file(&temp_dir, &cls_meta, &mut sf_graph, &sidecar).await.unwrap();
    assert!(res_cls, "cls-meta.xml must be digested as Salesforce metadata");
    assert!(sf_graph.get_entity("salesforce::cls::MyClass").is_some());

    let res_trans = Indexer::index_single_file(&temp_dir, &field_trans_meta, &mut sf_graph, &sidecar).await.unwrap();
    assert!(res_trans, "fieldTranslation-meta.xml must be digested as Salesforce metadata");
    assert!(sf_graph.get_entity("salesforce::fieldtranslation::Account-es").is_some());

    let res_obj = Indexer::index_single_file(&temp_dir, &obj_meta, &mut sf_graph, &sidecar).await.unwrap();
    assert!(res_obj, "object-meta.xml must be indexed as SObject schema");
    assert!(sf_graph.get_entity("sobject::Account").is_some());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_rumin_diff_bounded_jsonrpc_frame_summary() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_diff_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(temp_dir.join(".rumin/checkpoints")).unwrap();

    let cp_path = temp_dir.join(".rumin/checkpoints/default.bin");

    // 1. Create base checkpoint with 500 entities
    let mut base_graph = CodeGraph::new();
    for i in 0..500 {
        let ent = CodeEntity::new(
            &format!("base_func_{}", i),
            &format!("func_{}", i),
            EntityType::Function,
            "src/base.rs",
            i,
            i,
        );
        base_graph.add_entity(ent);
    }
    base_graph.save_binary_checkpoint(&cp_path).unwrap();

    // 2. Setup server with live graph having 1,000 added entities
    let server = MCPServer::new(temp_dir.clone(), std::path::PathBuf::from("sidecar/worker.py"));
    {
        let mut live_graph = base_graph.clone();
        for i in 0..1000 {
            let ent = CodeEntity::new(
                &format!("new_func_{}", i),
                &format!("new_func_{}", i),
                EntityType::Function,
                "src/new.rs",
                i,
                i,
            );
            live_graph.add_entity(ent);
        }
        server.graph_swap().store(Arc::new(live_graph));
    }

    // 3. Call rumin_diff via JSON-RPC
    let diff_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "rumin_diff",
            "arguments": {
                "checkpoint_path": cp_path.to_string_lossy().to_string()
            }
        }
    });

    let resp = server.handle_request_for_test(diff_req).await.unwrap();
    let text = resp.get("result").and_then(|r| r.get("content")).unwrap()[0].get("text").unwrap().as_str().unwrap();

    // Assert summary structure
    assert!(text.contains("=== TOPOLOGICAL GRAPH DIFF ==="));
    assert!(text.contains("+ Added Entities: 1000"));
    assert!(text.contains("Sample Added Entities (top 20):"));
    assert!(text.contains("... and 980 more added entities"));

    // Critical assertion: frame length MUST remain compact (< 10 KB) to prevent JSON-RPC frame limit reset
    assert!(text.len() < 10_000, "Diff output was {} bytes, expected < 10,000 bytes", text.len());

    let _ = fs::remove_dir_all(&temp_dir);
}


