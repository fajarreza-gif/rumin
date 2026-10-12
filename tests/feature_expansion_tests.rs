use ai_rumin::bench::BenchRunner;
use ai_rumin::config::RuminConfig;
use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::Indexer;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{CodeEntity, EdgeKind, EntityType};
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn test_rumin_init_project() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_init_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let rumin_dir = RuminConfig::init_project(&temp_dir).unwrap();
    assert!(rumin_dir.exists());
    assert!(temp_dir.join(".rumin/config.json").exists());
    assert!(temp_dir.join(".ruminignore").exists());

    let config = RuminConfig::load(&temp_dir);
    assert_eq!(config.token_budget, 1500);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_binary_checkpoint_roundtrip() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_cp_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();
    let cp_path = temp_dir.join("test.bin");

    let mut graph = CodeGraph::new();
    let mut e1 = CodeEntity::new("service::auth", "authenticate", EntityType::Function, "src/auth.rs", 10, 25);
    e1.signature = Some("pub fn authenticate(token: &str) -> bool".to_string());
    let e2 = CodeEntity::new("db::users", "users", EntityType::Table, "migrations.sql", 1, 5);

    graph.add_entity(e1);
    graph.add_entity(e2);
    graph.add_edge("service::auth", "db::users", EdgeKind::DependsOn);

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);

    graph.save_binary_checkpoint(&cp_path).unwrap();
    assert!(cp_path.exists());

    let loaded = CodeGraph::load_binary_checkpoint(&cp_path).unwrap();
    assert_eq!(loaded.node_count(), 2);
    assert_eq!(loaded.edge_count(), 1);

    let auth_ent = loaded.get_entity("service::auth").unwrap();
    assert_eq!(auth_ent.name, "authenticate");
    assert_eq!(auth_ent.signature.as_deref(), Some("pub fn authenticate(token: &str) -> bool"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_graph_diff_detection() {
    let mut base_graph = CodeGraph::new();
    let e1 = CodeEntity::new("mod::api", "api", EntityType::Module, "src/api.rs", 1, 10);
    let e2 = CodeEntity::new("mod::db", "db", EntityType::Module, "src/db.rs", 1, 10);
    base_graph.add_entity(e1);
    base_graph.add_entity(e2);
    base_graph.add_edge("mod::api", "mod::db", EdgeKind::DependsOn);

    let mut new_graph = CodeGraph::new();
    let mut e1_mod = CodeEntity::new("mod::api", "api", EntityType::Module, "src/api.rs", 1, 15);
    e1_mod.signature = Some("version 2".to_string());
    let e3 = CodeEntity::new("mod::cache", "cache", EntityType::Module, "src/cache.rs", 1, 10);
    new_graph.add_entity(e1_mod);
    new_graph.add_entity(e3);
    new_graph.add_edge("mod::api", "mod::cache", EdgeKind::DependsOn);

    let diff = base_graph.diff(&new_graph);

    // e3 added
    assert_eq!(diff.added_entities.len(), 1);
    assert_eq!(diff.added_entities[0].id, "mod::cache");

    // e2 removed
    assert_eq!(diff.removed_entities.len(), 1);
    assert_eq!(diff.removed_entities[0], "mod::db");

    // e1 modified
    assert_eq!(diff.modified_entities.len(), 1);
    assert_eq!(diff.modified_entities[0].id, "mod::api");

    // edges
    assert_eq!(diff.added_edges.len(), 1);
    assert_eq!(diff.removed_edges.len(), 1);
}

#[test]
fn test_blast_radius_hazard_report() {
    let mut graph = CodeGraph::new();

    let leaf = CodeEntity::new("core::security", "validate_token", EntityType::Function, "src/sec.rs", 1, 10);
    let caller1 = CodeEntity::new("api::handler", "handle_request", EntityType::Function, "src/api.rs", 1, 10);
    let caller2 = CodeEntity::new("admin::handler", "admin_request", EntityType::Function, "src/admin.rs", 1, 10);
    let gateway = CodeEntity::new("gateway::router", "dispatch", EntityType::Function, "src/router.rs", 1, 10);

    graph.add_entity(leaf);
    graph.add_entity(caller1);
    graph.add_entity(caller2);
    graph.add_entity(gateway);

    // gateway -> caller1 -> leaf
    // caller2 -> leaf
    graph.add_edge("api::handler", "core::security", EdgeKind::Calls);
    graph.add_edge("admin::handler", "core::security", EdgeKind::Calls);
    graph.add_edge("gateway::router", "api::handler", EdgeKind::Calls);

    let report = graph.blast_radius(&["core::security".to_string()]);
    assert_eq!(report.total_impacted, 3);
    assert_eq!(report.high_risk.len(), 2); // api::handler and admin::handler (hop 1)
    assert_eq!(report.medium_risk.len(), 1); // gateway::router (hop 2)
    assert_eq!(report.low_risk.len(), 0);

    // Verify file path expansion as hazard root
    let file_report = graph.blast_radius(&["src/sec.rs".to_string()]);
    assert_eq!(file_report.total_impacted, 3);
    assert_eq!(file_report.high_risk.len(), 2);
}

#[test]
fn test_graph_verification() {
    let mut graph = CodeGraph::new();

    // Proxy with no target route rule
    let orphan_proxy = CodeEntity::new("apiproxy::proxy::unrouted", "unrouted", EntityType::Interface, "proxies/unrouted.xml", 1, 1);
    graph.add_entity(orphan_proxy);

    let issues = graph.verify();
    assert!(!issues.is_empty());
    assert!(issues.iter().any(|i| i.symbol_id == "apiproxy::proxy::unrouted" && i.message.contains("no route rule")));
}

#[test]
fn test_architectural_audit() {
    let mut graph = CodeGraph::new();

    let ui = CodeEntity::new("ui::login_page", "LoginPage", EntityType::Tag, "web/login.html", 1, 10);
    let table = CodeEntity::new("db::passwords", "passwords", EntityType::Table, "schema.sql", 1, 10);
    graph.add_entity(ui);
    graph.add_entity(table);

    // Forbidden direct presentation to DB reference
    graph.add_edge("ui::login_page", "db::passwords", EdgeKind::References);

    let violations = graph.audit();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule_name, "presentation_cannot_access_db");
    assert_eq!(violations[0].severity, "error");
}

#[tokio::test]
async fn test_bench_runner_execution() {
    let sidecar = SidecarClient::new("sidecar/worker.py");
    let report = BenchRunner::run_benchmark(&PathBuf::from("tests/fixtures"), &sidecar).await.unwrap();

    assert!(report.total_files_indexed > 0);
    assert!(report.total_nodes > 0);
    assert!(report.ppr_queries_executed > 0);
    assert!(report.ppr_throughput_qps > 0.0);
}

#[tokio::test]
async fn test_incremental_sync_manifest() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_sync_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let f1 = temp_dir.join("calc.ts");
    fs::write(&f1, "export function add(a: number, b: number): number { return a + b; }\n").unwrap();

    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new("sidecar/worker.py");
    let indexer = Indexer::new(&temp_dir);

    // 1. Initial sync
    let stats1 = indexer.sync_repository(&mut graph, &sidecar, false).await.unwrap();
    assert_eq!(stats1.added, 1);
    assert_eq!(stats1.modified, 0);
    assert_eq!(stats1.deleted, 0);

    // 2. Unchanged sync
    let stats2 = indexer.sync_repository(&mut graph, &sidecar, false).await.unwrap();
    assert_eq!(stats2.added, 0);
    assert_eq!(stats2.modified, 0);
    assert_eq!(stats2.deleted, 0);
    assert_eq!(stats2.unchanged, 1);

    // 3. Modified sync
    fs::write(&f1, "export function add(a: number, b: number): number { return a + b; }\nexport function sub(a: number, b: number): number { return a - b; }\n").unwrap();
    let stats3 = indexer.sync_repository(&mut graph, &sidecar, false).await.unwrap();
    assert_eq!(stats3.modified, 1);

    // 4. Deleted sync
    fs::remove_file(&f1).unwrap();
    let stats4 = indexer.sync_repository(&mut graph, &sidecar, false).await.unwrap();
    assert_eq!(stats4.deleted, 1);

    let _ = fs::remove_dir_all(&temp_dir);
}
