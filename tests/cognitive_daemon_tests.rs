use ai_rumin::consolidate::MemoryConsolidator;
use ai_rumin::db::MemoryDb;
use ai_rumin::episodic::EpisodicEngine;
use ai_rumin::graph::CodeGraph;
use ai_rumin::hippocampal::HippocampalEngine;
use ai_rumin::hook::CircuitBreaker;
use ai_rumin::session::SessionManager;
use ai_rumin::trace::TraceIngestor;
use ai_rumin::types::{CodeEntity, ContextBudget, EdgeKind, EntityType};
use std::fs;
use std::sync::Arc;

#[test]
fn test_episodic_record_and_search() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_ep_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let engine = EpisodicEngine::new(db);

    let ep1 = engine.record_episode(
        "Fix Token Deserialization in Bincode",
        "Resolved bincode deserialize_any error by switching to compact JSON bytes serialization.",
        true,
        &["CodeGraph::save_binary_checkpoint".to_string()],
        &["src/graph.rs".to_string()],
        &["bug-fix".to_string(), "bincode".to_string(), "serialization".to_string()],
    ).unwrap();

    assert_eq!(ep1.id, "ep_1");
    assert!(ep1.success);

    let ep2 = engine.record_episode(
        "Apigee Route Target Missing",
        "Encountered warning on unrouted proxy endpoint, added default route rule.",
        false,
        &["apiproxy::proxy::default".to_string()],
        &["apiproxy/proxies/default.xml".to_string()],
        &["apigee".to_string(), "route-rule".to_string()],
    ).unwrap();

    assert_eq!(ep2.id, "ep_2");
    assert!(!ep2.success);

    // Search query
    let matches = engine.search_episodes("bincode deserialization", 10).unwrap();
    assert!(!matches.is_empty());
    assert_eq!(matches[0].id, "ep_1");

    let apigee_matches = engine.search_episodes("apigee route", 10).unwrap();
    assert!(!apigee_matches.is_empty());
    assert_eq!(apigee_matches[0].id, "ep_2");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_episodic_graph_cross_linking() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_cross_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let engine = EpisodicEngine::new(db);

    // Record lesson bound to AccountService
    engine.record_episode(
        "Prevent SOQL Injection in AccountService",
        "Enforced static bind variables in SOQL queries inside AccountService.",
        true,
        &["AccountService::getActiveAccounts".to_string()],
        &["src/AccountService.cls".to_string()],
        &["security".to_string(), "soql".to_string()],
    ).unwrap();

    let mut graph = CodeGraph::new();
    let code_sym = CodeEntity::new("AccountService::getActiveAccounts", "getActiveAccounts", EntityType::Method, "src/AccountService.cls", 1, 10);
    graph.add_entity(code_sym);

    // Cross-link episodes into graph
    let bound = engine.bind_episodes_to_graph(&mut graph).unwrap();
    assert_eq!(bound, 1);
    assert!(graph.get_entity("episode::ep_1").is_some());

    // Personalized PageRank spreading: querying for AccountService should activate episode::ep_1
    let ppr = HippocampalEngine::default();
    let budget = ContextBudget::default();
    let result = ppr.query(&graph, "getActiveAccounts injection security", &budget);

    assert!(result.top_symbols.iter().any(|sym| sym.id.starts_with("episode::ep_1")));
    assert!(result.formatted_context.contains("Episode [Prevent SOQL Injection in AccountService]"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_session_lifecycle() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_sess_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let mgr = SessionManager::new(db);

    let sess = mgr.create_session("Auth Feature Refactor", None).unwrap();
    assert!(sess.id.starts_with("sess_"));
    assert_eq!(sess.name, "Auth Feature Refactor");

    let note1 = mgr.add_working_note(&sess.id, "Investigating Token refresh logic").unwrap();
    assert_eq!(note1.session_id, sess.id);

    let note2 = mgr.add_working_note(&sess.id, "Updated JWT secret key in config").unwrap();
    assert_eq!(note2.session_id, sess.id);

    let notes = mgr.get_working_notes(&sess.id).unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].content, "Investigating Token refresh logic");

    let list = mgr.list_sessions().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, sess.id);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_topological_consolidation() {
    let mut graph = CodeGraph::new();

    let a = CodeEntity::new("controller", "Controller", EntityType::Class, "src/ctrl.rs", 1, 10);
    let b = CodeEntity::new("service", "Service", EntityType::Class, "src/svc.rs", 1, 10);
    let c = CodeEntity::new("repository", "Repo", EntityType::Class, "src/repo.rs", 1, 10);
    let dead_temp = CodeEntity::new("temp::scratch", "scratch", EntityType::Unknown, ".tmp/scratch.rs", 1, 5);

    graph.add_entity(a);
    graph.add_entity(b);
    graph.add_entity(c);
    graph.add_entity(dead_temp);

    // controller -> service -> repository
    graph.add_edge("controller", "service", EdgeKind::Calls);
    graph.add_edge("service", "repository", EdgeKind::Calls);

    let report = MemoryConsolidator::consolidate(&mut graph).unwrap();
    assert_eq!(report.pruned_nodes, 1); // dead_temp pruned
    assert!(report.shortcut_edges_created >= 1); // shortcut edge between controller and repository

    assert!(graph.get_entity("temp::scratch").is_none());
    assert!(graph.get_entity("controller").is_some());
    assert!(graph.get_entity("repository").is_some());
}

#[test]
fn test_trace_ingestion() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_tr_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let trace_file = temp_dir.join("sample_trace.jsonl");
    let trace_content = r#"{"type":"tool_call","action":"read_file","file_path":"src/indexer.rs"}
{"type":"tool_call","action":"replace_file","file_path":"src/graph.rs"}
{"type":"step","action":"run_command","target":"cargo check"}
"#;
    fs::write(&trace_file, trace_content).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let ingestor = TraceIngestor::new(db);

    let report = ingestor.ingest_file(&trace_file, Some("sess_test123")).unwrap();
    assert_eq!(report.steps_ingested, 3);
    assert_eq!(report.unique_files_touched, 3);
    assert_eq!(report.session_id.as_deref(), Some("sess_test123"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_circuit_breaker_hooks() {
    // 1. Slice check
    let pass = CircuitBreaker::check_slice(40, Some(50));
    assert!(pass.allowed);
    assert!(pass.reason.is_none());

    let fail = CircuitBreaker::check_slice(120, Some(50));
    assert!(!fail.allowed);
    assert!(fail.reason.unwrap().contains("[Token Circuit Breaker]"));

    // 2. Command check
    let cmd_pass = CircuitBreaker::check_command("cargo test --all");
    assert!(cmd_pass.allowed);

    let cmd_fail = CircuitBreaker::check_command("rm -rf / --no-preserve-root");
    assert!(!cmd_fail.allowed);
    assert!(cmd_fail.reason.unwrap().contains("Forbidden hazardous"));
}

#[test]
fn test_episodic_deletion_collision_resilience() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_ep_resil_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let db = Arc::new(MemoryDb::open(&temp_dir).unwrap());
    let engine = EpisodicEngine::new(db.clone());

    let ep1 = engine.record_episode("Ep1", "First", true, &[], &[], &[]).unwrap();
    let _ep2 = engine.record_episode("Ep2", "Second", true, &[], &[], &[]).unwrap();
    let ep3 = engine.record_episode("Ep3", "Third", true, &[], &[], &[]).unwrap();

    assert_eq!(ep1.id, "ep_1");
    assert_eq!(ep3.id, "ep_3");

    // Delete ep_2 from database
    db.conn().execute("DELETE FROM episodes WHERE id = 'ep_2'", []).unwrap();

    // In the past, COUNT(*) would be 2 and next insert would generate ep_3, colliding!
    // Now it should generate ep_4 without collision.
    let ep4 = engine.record_episode("Ep4", "Fourth", true, &[], &[], &[]).unwrap();
    assert_eq!(ep4.id, "ep_4");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_consolidation_preserves_legitimate_entities() {
    let mut graph = CodeGraph::new();

    // A legitimate function in src/service.rs
    let leg_entity = CodeEntity::new("svc::process", "process", EntityType::Function, "src/service.rs", 1, 10);
    // A temporary entity inside the same file
    let temp_entity = CodeEntity::new("temp::macro_expand", "macro", EntityType::Function, "src/service.rs", 12, 15);

    graph.add_entity(leg_entity);
    graph.add_entity(temp_entity);

    assert_eq!(graph.node_count(), 2);

    let report = MemoryConsolidator::consolidate(&mut graph).unwrap();
    assert_eq!(report.pruned_nodes, 1);

    // Legitimate entity in src/service.rs MUST be preserved!
    assert!(graph.get_entity("svc::process").is_some());
    // Temporary entity must be pruned
    assert!(graph.get_entity("temp::macro_expand").is_none());
}
