use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::{FileManifest, Indexer};
use ai_rumin::mcp::MCPServer;
use ai_rumin::sidecar::SidecarClient;
use ai_rumin::types::{CodeEntity, EntityType};
use serde_json::json;
use std::fs;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_arc_swap_rcu_concurrent_readers() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_rcu_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let server = Arc::new(MCPServer::new(temp_dir.clone(), temp_dir.join("sidecar.py")));

    // Spawn 10 concurrent readers calling rumin_query and rumin_digest
    let mut handles = Vec::new();
    for i in 0..10 {
        let server_clone = server.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..50 {
                let req = json!({
                    "jsonrpc": "2.0",
                    "id": i * 100 + j,
                    "method": "tools/call",
                    "params": {
                        "name": if j % 2 == 0 { "rumin_query" } else { "rumin_digest" },
                        "arguments": { "query": "auth handler" }
                    }
                });
                let resp = server_clone.handle_request_for_test(req).await;
                assert!(resp.is_some());
            }
        }));
    }

    // Simultaneously, write new snapshots to graph_swap
    for k in 0..10 {
        let mut g = CodeGraph::new();
        for node in 0..(k * 5 + 5) {
            g.add_entity(CodeEntity::new(&format!("sym_{}", node), &format!("node_{}", node), EntityType::Function, "src/lib.rs", 1, 10));
        }
        server.graph_swap().store(Arc::new(g));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    for h in handles {
        h.await.unwrap();
    }

    let final_graph = server.graph();
    assert!(final_graph.node_count() >= 50);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_rumin_switch_repo() {
    let base_temp = std::env::temp_dir().join(format!("rumin_switch_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let repo_a = base_temp.join("repo_a");
    let repo_b = base_temp.join("repo_b");
    fs::create_dir_all(&repo_a).unwrap();
    fs::create_dir_all(&repo_b).unwrap();

    // In repo_b, create a checkpoint
    let cp_dir_b = repo_b.join(".rumin/checkpoints");
    fs::create_dir_all(&cp_dir_b).unwrap();
    let mut graph_b = CodeGraph::new();
    graph_b.add_entity(CodeEntity::new("repo_b::core", "core", EntityType::Module, "src/core.rs", 1, 10));
    graph_b.save_binary_checkpoint(&cp_dir_b.join("default.bin")).unwrap();

    let server = MCPServer::new(repo_a.clone(), repo_a.join("sidecar.py"));

    let switch_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "rumin_switch_repo",
            "arguments": {
                "repo_path": repo_b.to_str().unwrap()
            }
        }
    });

    let resp = server.handle_request_for_test(switch_req).await;
    assert!(resp.is_some());
    let r = resp.unwrap();
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("switched"));
    assert!(text.contains("repo_b"));

    // Verify the server's graph now contains the node from repo_b
    let g = server.graph();
    assert_eq!(g.node_count(), 1);
    assert!(g.get_entity("repo_b::core").is_some());

    let _ = fs::remove_dir_all(&base_temp);
}

#[tokio::test]
async fn test_cancellation_token_abort() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_cancel_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    // Create 10 dummy files
    for i in 0..10 {
        fs::write(temp_dir.join(format!("file_{}.rs", i)), format!("pub fn f_{}() {{}}", i)).unwrap();
    }

    let cancel_token = CancellationToken::new();
    cancel_token.cancel(); // Pre-cancelled

    let indexer = Indexer::new(&temp_dir);
    let manifest = FileManifest::default();
    let result = indexer.compute_delta(&manifest, false, Some(&cancel_token));

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cancelled"));

    let mut graph = CodeGraph::new();
    let sidecar = SidecarClient::new(temp_dir.join("sidecar.py"));
    let sync_res = indexer.sync_repository_cancellable(&mut graph, &sidecar, false, Some(&cancel_token)).await;
    assert!(sync_res.is_err());

    let _ = fs::remove_dir_all(&temp_dir);
}
