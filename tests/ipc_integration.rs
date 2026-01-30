//! Integration tests for IPC round-trip communication.

use std::time::Duration;

use claude_supervisor::ipc::{EscalationRequest, EscalationResponse, IpcClient, IpcServer};
use serde_json::json;

/// Test full IPC communication between client and server.
#[tokio::test]
async fn ipc_round_trip_allow() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("ipc-test-allow-{}.sock", std::process::id()));

    // Start server that allows all requests
    let server = IpcServer::new(&socket_path);
    let handle = server
        .start(|_req| async { EscalationResponse::Allow })
        .expect("Failed to start server");

    // Wait for server to be ready
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Create client and send request
    let client = IpcClient::with_path(&socket_path);
    assert!(client.is_supervisor_running());

    let request = EscalationRequest {
        session_id: "test-session".to_string(),
        tool_name: "Read".to_string(),
        tool_input: json!({"path": "/tmp/test.txt"}),
        reason: "Test escalation".to_string(),
    };

    let response = client.escalate(&request).await.expect("Escalation failed");
    assert!(matches!(response, EscalationResponse::Allow));

    handle.shutdown();
}

/// Test IPC with deny response.
#[tokio::test]
async fn ipc_round_trip_deny() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("ipc-test-deny-{}.sock", std::process::id()));

    // Start server that denies Bash commands
    let server = IpcServer::new(&socket_path);
    let handle = server
        .start(|req| async move {
            if req.tool_name == "Bash" {
                EscalationResponse::Deny {
                    reason: "Bash commands not allowed".to_string(),
                }
            } else {
                EscalationResponse::Allow
            }
        })
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let client = IpcClient::with_path(&socket_path);

    // Test denied request
    let request = EscalationRequest {
        session_id: "test-session".to_string(),
        tool_name: "Bash".to_string(),
        tool_input: json!({"command": "rm -rf /"}),
        reason: "Dangerous command".to_string(),
    };

    let response = client.escalate(&request).await.expect("Escalation failed");
    match response {
        EscalationResponse::Deny { reason } => {
            assert_eq!(reason, "Bash commands not allowed");
        }
        _ => panic!("Expected Deny response"),
    }

    handle.shutdown();
}

/// Test IPC with modify response.
#[tokio::test]
async fn ipc_round_trip_modify() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("ipc-test-modify-{}.sock", std::process::id()));

    // Start server that modifies input
    let server = IpcServer::new(&socket_path);
    let handle = server
        .start(|_req| async {
            EscalationResponse::Modify {
                updated_input: json!({"command": "ls -la"}),
            }
        })
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let client = IpcClient::with_path(&socket_path);

    let request = EscalationRequest {
        session_id: "test-session".to_string(),
        tool_name: "Bash".to_string(),
        tool_input: json!({"command": "ls"}),
        reason: "Add flags".to_string(),
    };

    let response = client.escalate(&request).await.expect("Escalation failed");
    match response {
        EscalationResponse::Modify { updated_input } => {
            assert_eq!(updated_input, json!({"command": "ls -la"}));
        }
        _ => panic!("Expected Modify response"),
    }

    handle.shutdown();
}

/// Test multiple concurrent clients.
#[tokio::test]
async fn ipc_multiple_clients() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("ipc-test-multi-{}.sock", std::process::id()));

    let server = IpcServer::new(&socket_path);
    let handle = server
        .start(|req| async move {
            // Echo back the session_id in the reason
            EscalationResponse::Deny {
                reason: format!("Received from {}", req.session_id),
            }
        })
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Spawn multiple concurrent clients
    let mut handles = Vec::new();
    for i in 0..5 {
        let path = socket_path.clone();
        handles.push(tokio::spawn(async move {
            let client = IpcClient::with_path(&path);
            let request = EscalationRequest {
                session_id: format!("session-{i}"),
                tool_name: "Test".to_string(),
                tool_input: json!({}),
                reason: "Concurrent test".to_string(),
            };
            client.escalate(&request).await
        }));
    }

    // Wait for all clients to complete
    for (i, h) in handles.into_iter().enumerate() {
        let response = h.await.expect("Task panicked").expect("Escalation failed");
        match response {
            EscalationResponse::Deny { reason } => {
                assert!(reason.contains(&format!("session-{i}")));
            }
            _ => panic!("Expected Deny response"),
        }
    }

    handle.shutdown();
}

/// Test client timeout behavior.
#[tokio::test]
async fn ipc_client_timeout() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("ipc-test-timeout-{}.sock", std::process::id()));

    // Start server that delays response
    let server = IpcServer::new(&socket_path);
    let handle = server
        .start(|_req| async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            EscalationResponse::Allow
        })
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Client with short timeout
    let client = IpcClient::with_path(&socket_path).with_timeout(Duration::from_millis(100));

    let request = EscalationRequest {
        session_id: "test".to_string(),
        tool_name: "Test".to_string(),
        tool_input: json!({}),
        reason: "Timeout test".to_string(),
    };

    let result = client.escalate(&request).await;
    assert!(result.is_err());

    handle.shutdown();
}

/// Test server handle cleanup on drop.
#[tokio::test]
async fn ipc_server_cleanup_on_drop() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("ipc-test-cleanup-{}.sock", std::process::id()));

    {
        let server = IpcServer::new(&socket_path);
        let _handle = server
            .start(|_| async { EscalationResponse::Allow })
            .expect("Failed to start server");

        assert!(socket_path.exists());
    }

    // Give time for cleanup
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(!socket_path.exists());
}
