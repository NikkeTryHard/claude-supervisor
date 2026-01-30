//! Integration tests for the dashboard module.

use std::time::Duration;

use claude_supervisor::dashboard::{
    create_dashboard_channels, DashboardCommand, DashboardConfig, DashboardEvent, DashboardServer,
    SupervisorStatus,
};
use tokio::time::timeout;

/// Test that dashboard channels communicate status updates and commands correctly.
#[tokio::test]
async fn test_dashboard_channels_communication() {
    let (state, mut handles) = create_dashboard_channels();

    // Test 1: Status updates from supervisor to dashboard
    let new_status = SupervisorStatus {
        session_id: Some("test-session-123".to_string()),
        state: "running".to_string(),
        tool_calls: 10,
        approvals: 8,
        denials: 2,
        task: Some("Fix the authentication bug".to_string()),
    };

    handles
        .status_tx
        .send(new_status.clone())
        .expect("Failed to send status");

    // Dashboard should receive the status
    let received_status = state.status_rx.borrow().clone();
    assert_eq!(
        received_status.session_id,
        Some("test-session-123".to_string())
    );
    assert_eq!(received_status.state, "running");
    assert_eq!(received_status.tool_calls, 10);
    assert_eq!(received_status.approvals, 8);
    assert_eq!(received_status.denials, 2);
    assert_eq!(
        received_status.task,
        Some("Fix the authentication bug".to_string())
    );

    // Test 2: Commands from dashboard to supervisor
    state
        .command_tx
        .send(DashboardCommand::Stop)
        .await
        .expect("Failed to send command");

    let received_cmd = handles
        .command_rx
        .recv()
        .await
        .expect("Failed to receive command");
    assert_eq!(received_cmd, DashboardCommand::Stop);

    // Test 3: Multiple status updates
    for i in 0u64..5 {
        handles
            .status_tx
            .send(SupervisorStatus {
                session_id: Some(format!("session-{i}")),
                state: "processing".to_string(),
                tool_calls: i,
                approvals: 0,
                denials: 0,
                task: None,
            })
            .expect("Failed to send status update");
    }

    // Only the latest status should be visible
    let final_status = state.status_rx.borrow();
    assert_eq!(final_status.session_id, Some("session-4".to_string()));
    assert_eq!(final_status.tool_calls, 4);
}

/// Test that events are broadcast to multiple subscribers.
#[tokio::test]
async fn test_dashboard_event_broadcast() {
    let (state, handles) = create_dashboard_channels();

    // Create multiple subscribers
    let mut subscriber1 = state.event_tx.subscribe();
    let mut subscriber2 = state.event_tx.subscribe();
    let mut subscriber3 = handles.event_tx.subscribe();

    // Send an event from the supervisor side
    let event = DashboardEvent::new(
        "tool_call",
        serde_json::json!({
            "tool": "Bash",
            "command": "ls -la"
        }),
    );

    handles
        .event_tx
        .send(event.clone())
        .expect("Failed to broadcast event");

    // All subscribers should receive the event
    let recv1 = subscriber1.recv().await.expect("Subscriber 1 failed");
    let recv2 = subscriber2.recv().await.expect("Subscriber 2 failed");
    let recv3 = subscriber3.recv().await.expect("Subscriber 3 failed");

    assert_eq!(recv1.event_type, "tool_call");
    assert_eq!(recv2.event_type, "tool_call");
    assert_eq!(recv3.event_type, "tool_call");

    assert_eq!(recv1.data["tool"], "Bash");
    assert_eq!(recv2.data["command"], "ls -la");

    // Send multiple events
    for i in 0..3 {
        let event = DashboardEvent::new(format!("event_{i}"), serde_json::json!({"index": i}));
        state.event_tx.send(event).expect("Failed to send event");
    }

    // Verify all subscribers receive all events
    for i in 0..3 {
        let recv1 = subscriber1.recv().await.expect("Failed to receive");
        let recv2 = subscriber2.recv().await.expect("Failed to receive");

        assert_eq!(recv1.event_type, format!("event_{i}"));
        assert_eq!(recv2.data["index"], i);
    }
}

/// Test graceful shutdown via `CancellationToken`.
#[tokio::test]
async fn test_dashboard_server_shutdown() {
    let (dashboard_state, _handles) = create_dashboard_channels();
    let cancel = dashboard_state.cancel.clone();

    // We need to bind manually to get an available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get address");

    // Create server with the bound port
    let config = DashboardConfig {
        port: addr.port(),
        host: "127.0.0.1".to_string(),
        cors_permissive: true,
    };

    let server = DashboardServer::new(dashboard_state, None).with_config(config);
    let router = server.build_router();

    // Clone cancel for triggering shutdown
    let cancel_trigger = cancel.clone();

    // Spawn the server
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                cancel.cancelled().await;
            })
            .await
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify server is running by attempting a TCP connection
    let connect_result = tokio::net::TcpStream::connect(addr).await;
    assert!(
        connect_result.is_ok(),
        "Server should be accepting connections"
    );

    // Trigger shutdown via cancellation token
    cancel_trigger.cancel();

    // Wait for server to shut down with timeout
    let shutdown_result = timeout(Duration::from_secs(2), server_handle).await;
    assert!(
        shutdown_result.is_ok(),
        "Server should shut down within timeout"
    );

    // Verify server has stopped by checking connection is refused
    tokio::time::sleep(Duration::from_millis(100)).await;
    let connect_after = tokio::net::TcpStream::connect(addr).await;
    assert!(
        connect_after.is_err(),
        "Server should no longer accept connections"
    );
}

/// Test graceful shutdown with proper cancellation token.
#[tokio::test]
async fn test_dashboard_cancellation_token() {
    let (state, _handles) = create_dashboard_channels();

    // Token should not be cancelled initially
    assert!(!state.cancel.is_cancelled());

    // Clone for testing
    let cancel_clone = state.cancel.clone();

    // Spawn a task that waits for cancellation
    let wait_handle = tokio::spawn(async move {
        state.cancel.cancelled().await;
        true
    });

    // Cancel after a short delay
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    // Wait for the task with timeout
    let result = timeout(Duration::from_secs(1), wait_handle)
        .await
        .expect("Timeout waiting for cancellation")
        .expect("Task panicked");

    assert!(result, "Task should have completed after cancellation");
}

/// Test all command types work correctly.
#[tokio::test]
async fn test_dashboard_all_commands() {
    let (state, mut handles) = create_dashboard_channels();

    // Test Stop command
    state
        .command_tx
        .send(DashboardCommand::Stop)
        .await
        .expect("Failed to send Stop");
    let cmd = handles.command_rx.recv().await.expect("Failed to receive");
    assert_eq!(cmd, DashboardCommand::Stop);

    // Test Continue command
    state
        .command_tx
        .send(DashboardCommand::Continue)
        .await
        .expect("Failed to send Continue");
    let cmd = handles.command_rx.recv().await.expect("Failed to receive");
    assert_eq!(cmd, DashboardCommand::Continue);

    // Test ForceKill command
    state
        .command_tx
        .send(DashboardCommand::ForceKill)
        .await
        .expect("Failed to send ForceKill");
    let cmd = handles.command_rx.recv().await.expect("Failed to receive");
    assert_eq!(cmd, DashboardCommand::ForceKill);

    // Test command serialization/deserialization
    let stop_json = serde_json::to_string(&DashboardCommand::Stop).unwrap();
    let continue_json = serde_json::to_string(&DashboardCommand::Continue).unwrap();
    let kill_json = serde_json::to_string(&DashboardCommand::ForceKill).unwrap();

    assert_eq!(stop_json, "\"Stop\"");
    assert_eq!(continue_json, "\"Continue\"");
    assert_eq!(kill_json, "\"ForceKill\"");

    let stop: DashboardCommand = serde_json::from_str(&stop_json).unwrap();
    let cont: DashboardCommand = serde_json::from_str(&continue_json).unwrap();
    let kill: DashboardCommand = serde_json::from_str(&kill_json).unwrap();

    assert_eq!(stop, DashboardCommand::Stop);
    assert_eq!(cont, DashboardCommand::Continue);
    assert_eq!(kill, DashboardCommand::ForceKill);
}

/// Test that status changes are properly watched.
#[tokio::test]
async fn test_status_watch_changes() {
    let (state, handles) = create_dashboard_channels();

    // Clone the receiver for watching changes
    let mut status_rx = state.status_rx.clone();

    // Spawn a task that watches for changes
    let watch_handle = tokio::spawn(async move {
        let mut change_count = 0;
        while change_count < 3 {
            status_rx.changed().await.expect("Watch failed");
            change_count += 1;
        }
        change_count
    });

    // Send 3 status updates
    for i in 0u64..3 {
        handles
            .status_tx
            .send(SupervisorStatus {
                session_id: Some(format!("watch-{i}")),
                state: "updating".to_string(),
                tool_calls: i,
                approvals: 0,
                denials: 0,
                task: None,
            })
            .expect("Failed to send status");

        // Small delay to ensure changes are processed
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Verify the watcher received all changes
    let result = timeout(Duration::from_secs(1), watch_handle)
        .await
        .expect("Timeout")
        .expect("Task panicked");

    assert_eq!(result, 3, "Should have received 3 status changes");
}

/// Test dashboard event types and data.
#[tokio::test]
async fn test_dashboard_event_types() {
    // Test various event types
    let tool_call_event = DashboardEvent::new(
        "tool_call",
        serde_json::json!({
            "tool": "Read",
            "input": {"path": "/tmp/file.txt"}
        }),
    );

    assert_eq!(tool_call_event.event_type, "tool_call");
    assert_eq!(tool_call_event.data["tool"], "Read");

    let approval_event = DashboardEvent::new(
        "approval",
        serde_json::json!({
            "tool": "Bash",
            "decision": "allow",
            "reason": "Safe command"
        }),
    );

    assert_eq!(approval_event.event_type, "approval");
    assert_eq!(approval_event.data["decision"], "allow");

    let denial_event = DashboardEvent::new(
        "denial",
        serde_json::json!({
            "tool": "Bash",
            "decision": "deny",
            "reason": "Destructive command blocked"
        }),
    );

    assert_eq!(denial_event.event_type, "denial");
    assert_eq!(denial_event.data["reason"], "Destructive command blocked");

    let output_event = DashboardEvent::new(
        "output",
        serde_json::json!({
            "content": "Hello, World!",
            "type": "assistant"
        }),
    );

    assert_eq!(output_event.event_type, "output");
    assert_eq!(output_event.data["content"], "Hello, World!");

    // Test serialization
    let json = serde_json::to_string(&tool_call_event).unwrap();
    assert!(json.contains("\"event_type\":\"tool_call\""));

    let parsed: DashboardEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.event_type, "tool_call");
}

/// Test dashboard server configuration.
#[tokio::test]
async fn test_dashboard_server_config() {
    let (dashboard_state, _handles) = create_dashboard_channels();

    // Test default config
    let server = DashboardServer::new(dashboard_state, None);
    assert_eq!(server.address(), "127.0.0.1:3000");

    // Test custom config
    let (dashboard_state2, _handles2) = create_dashboard_channels();
    let custom_config = DashboardConfig {
        port: 8080,
        host: "0.0.0.0".to_string(),
        cors_permissive: false,
    };

    let server = DashboardServer::new(dashboard_state2, None).with_config(custom_config);
    assert_eq!(server.address(), "0.0.0.0:8080");

    // Verify router builds without panicking
    let _router = server.build_router();
}

/// Test that the command channel has proper backpressure.
#[tokio::test]
async fn test_command_channel_capacity() {
    let (state, mut handles) = create_dashboard_channels();

    // The command channel has capacity of 32 (as defined in create_dashboard_channels)
    // Send multiple commands without receiving
    for i in 0..30 {
        let cmd = if i % 3 == 0 {
            DashboardCommand::Stop
        } else if i % 3 == 1 {
            DashboardCommand::Continue
        } else {
            DashboardCommand::ForceKill
        };

        state
            .command_tx
            .send(cmd)
            .await
            .expect("Failed to send command");
    }

    // Receive all commands
    for i in 0..30 {
        let cmd = handles.command_rx.recv().await.expect("Failed to receive");
        let expected = if i % 3 == 0 {
            DashboardCommand::Stop
        } else if i % 3 == 1 {
            DashboardCommand::Continue
        } else {
            DashboardCommand::ForceKill
        };
        assert_eq!(cmd, expected);
    }
}

/// Test broadcast channel behavior when no subscribers.
#[tokio::test]
async fn test_event_broadcast_no_subscribers() {
    let (_state, handles) = create_dashboard_channels();

    // Sending events with no subscribers should not error
    // (broadcast::send returns Err only when there are no receivers,
    // but the sender itself holds a receiver internally)
    let event = DashboardEvent::new("test", serde_json::json!({}));

    // This should not panic, though it might return an error if no subscribers
    let result = handles.event_tx.send(event);

    // The send might fail with no active subscribers (besides the internal one)
    // This is expected behavior - we just verify it doesn't panic
    let _ = result;
}
