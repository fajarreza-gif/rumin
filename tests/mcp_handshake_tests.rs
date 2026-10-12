use ai_rumin::graph::CodeGraph;
use ai_rumin::mcp::MCPServer;
use ai_rumin::types::{CodeEntity, EntityType};
use serde_json::json;
use std::fs;

#[tokio::test]
async fn test_mcp_handshake_and_checkpoint_auto_load() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_test_mcp_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();
    let cp_dir = temp_dir.join(".rumin/checkpoints");
    fs::create_dir_all(&cp_dir).unwrap();
    let default_cp = cp_dir.join("default.bin");

    // Pre-populate a checkpoint with 100 nodes
    let mut initial_graph = CodeGraph::new();
    for i in 0..100 {
        let ent = CodeEntity::new(
            &format!("sym_{}", i),
            &format!("func_{}", i),
            EntityType::Function,
            &format!("src/file_{}.rs", i),
            1,
            10,
        );
        initial_graph.add_entity(ent);
    }
    initial_graph.save_binary_checkpoint(&default_cp).unwrap();

    let sidecar_script = temp_dir.join("worker.py");
    let server = MCPServer::new(temp_dir.clone(), sidecar_script);

    // Test JSON-RPC initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    });

    let resp = server.handle_request_for_test(init_req).await;
    assert!(resp.is_some());
    let r = resp.unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2024-11-05");

    // Test ping
    let ping_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping"
    });
    let ping_resp = server.handle_request_for_test(ping_req).await;
    assert!(ping_resp.is_some());
    assert_eq!(ping_resp.unwrap()["result"], json!({}));

    // Test notification (no id) - MUST return None (no response)
    let notif_req = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let notif_resp = server.handle_request_for_test(notif_req).await;
    assert!(notif_resp.is_none(), "Notifications must never receive a response");

    let unknown_notif = json!({
        "jsonrpc": "2.0",
        "method": "some/unknown/notification"
    });
    let unk_resp = server.handle_request_for_test(unknown_notif).await;
    assert!(unk_resp.is_none(), "Unknown notifications must also never receive a response");

    let _ = fs::remove_dir_all(&temp_dir);
}
