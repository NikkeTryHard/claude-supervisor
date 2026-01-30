//! Integration tests for the audit logging module.

use std::path::PathBuf;

use claude_supervisor::audit::{
    AuditEvent, AuditLog, AuditSession, Decision, EventType, SessionMetrics,
};
use tempfile::TempDir;
use uuid::Uuid;

/// Helper to create a unique database path in a temp directory.
fn temp_db_path(temp_dir: &TempDir, name: &str) -> PathBuf {
    temp_dir
        .path()
        .join(format!("{}-{}.db", name, std::process::id()))
}

/// Test that the audit log file is created when opening.
#[tokio::test]
async fn test_audit_log_file_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_db_path(&temp_dir, "audit-creation");

    // File should not exist yet
    assert!(!db_path.exists());

    // Open the audit log
    let log = AuditLog::open(&db_path)
        .await
        .expect("Failed to open audit log");

    // File should now exist
    assert!(db_path.exists());

    // Verify the path is correct
    assert_eq!(log.path(), Some(db_path.as_path()));
}

/// Test that nested directories are created when opening audit log.
#[tokio::test]
async fn test_audit_log_nested_directory_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let nested_path = temp_dir
        .path()
        .join("deeply")
        .join("nested")
        .join("directory")
        .join("structure")
        .join("audit.db");

    // Parent directories should not exist
    assert!(!nested_path.parent().unwrap().exists());

    // Open the audit log - should create all parent directories
    let log = AuditLog::open(&nested_path)
        .await
        .expect("Failed to open audit log with nested path");

    // File and all parent directories should now exist
    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());
    assert_eq!(log.path(), Some(nested_path.as_path()));

    // Verify we can actually use the database
    let session = AuditSession::new("Test task in nested dir");
    log.log_session_start(&session)
        .await
        .expect("Failed to log session start");
}

/// Test full session lifecycle: start, log events, log metrics, end, verify.
#[tokio::test]
async fn test_full_session_lifecycle() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_db_path(&temp_dir, "audit-lifecycle");

    let log = AuditLog::open(&db_path)
        .await
        .expect("Failed to open audit log");

    // 1. Start a session
    let session = AuditSession::new("Integration test task");
    let session_id = session.id;

    log.log_session_start(&session)
        .await
        .expect("Failed to log session start");

    // 2. Log some events
    let events_to_log = vec![
        AuditEvent::builder(session_id, EventType::SessionStart)
            .reason("Session started")
            .build(),
        AuditEvent::builder(session_id, EventType::ToolUse)
            .tool_name("Read")
            .tool_input(serde_json::json!({"path": "/tmp/test.txt"}))
            .decision(Decision::Allow)
            .reason("Read is always allowed")
            .build(),
        AuditEvent::builder(session_id, EventType::ToolUse)
            .tool_name("Bash")
            .tool_input(serde_json::json!({"command": "ls -la"}))
            .decision(Decision::Allow)
            .reason("Safe command")
            .build(),
        AuditEvent::builder(session_id, EventType::PolicyDecision)
            .tool_name("Bash")
            .tool_input(serde_json::json!({"command": "rm -rf /"}))
            .decision(Decision::Deny)
            .reason("Destructive command blocked")
            .build(),
        AuditEvent::builder(session_id, EventType::AiEscalation)
            .tool_name("Write")
            .decision(Decision::Escalate)
            .reason("Uncertain - escalated to AI")
            .build(),
    ];

    for event in &events_to_log {
        log.log_event(event).await.expect("Failed to log event");
    }

    // 3. Log metrics
    let mut metrics = SessionMetrics::new(session_id);
    metrics.add_tokens(5000, 2500);
    metrics.record_api_call();
    metrics.record_api_call();
    metrics.record_cache_hit();
    metrics.calculate_cost();

    log.log_metrics(&metrics)
        .await
        .expect("Failed to log metrics");

    // 4. End the session
    log.log_session_end(session_id, "Success - all tests passed")
        .await
        .expect("Failed to log session end");

    // 5. Verify event counts
    let total_events = log.count_events().await.expect("Failed to count events");
    assert_eq!(total_events, 5, "Expected 5 events");

    let allow_count = log
        .count_by_decision(Decision::Allow)
        .await
        .expect("Failed to count Allow");
    assert_eq!(allow_count, 2, "Expected 2 Allow decisions");

    let deny_count = log
        .count_by_decision(Decision::Deny)
        .await
        .expect("Failed to count Deny");
    assert_eq!(deny_count, 1, "Expected 1 Deny decision");

    let escalate_count = log
        .count_by_decision(Decision::Escalate)
        .await
        .expect("Failed to count Escalate");
    assert_eq!(escalate_count, 1, "Expected 1 Escalate decision");

    // 6. Verify events can be retrieved
    let retrieved_events = log
        .get_events(session_id, 100)
        .await
        .expect("Failed to get events");
    assert_eq!(retrieved_events.len(), 5);

    // Verify event details (most recent first due to ORDER BY timestamp DESC)
    // Check that we have the expected tool names
    let tool_names: Vec<_> = retrieved_events
        .iter()
        .filter_map(|e| e.tool_name.as_ref())
        .collect();
    assert!(tool_names.contains(&&"Read".to_string()));
    assert!(tool_names.contains(&&"Bash".to_string()));
    assert!(tool_names.contains(&&"Write".to_string()));

    // 7. Verify metrics can be retrieved
    let retrieved_metrics = log
        .get_metrics(session_id)
        .await
        .expect("Failed to get metrics")
        .expect("Metrics should exist");

    assert_eq!(retrieved_metrics.session_id, session_id);
    assert_eq!(retrieved_metrics.input_tokens, 5000);
    assert_eq!(retrieved_metrics.output_tokens, 2500);
    assert_eq!(retrieved_metrics.api_calls, 2);
    assert_eq!(retrieved_metrics.cache_hits, 1);
    assert!(retrieved_metrics.estimated_cost_cents > 0);
}

/// Test that multiple sessions are properly isolated.
#[tokio::test]
async fn test_multiple_sessions() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_db_path(&temp_dir, "audit-multi-session");

    let log = AuditLog::open(&db_path)
        .await
        .expect("Failed to open audit log");

    // Create multiple sessions
    let session1 = AuditSession::new("Task 1: Fix authentication bug");
    let session2 = AuditSession::new("Task 2: Refactor database layer");
    let session3 = AuditSession::new("Task 3: Add new feature");

    // Log all session starts
    log.log_session_start(&session1).await.unwrap();
    log.log_session_start(&session2).await.unwrap();
    log.log_session_start(&session3).await.unwrap();

    // Log different numbers of events for each session
    // Session 1: 3 events (2 Allow, 1 Deny)
    for i in 0..2 {
        let event = AuditEvent::builder(session1.id, EventType::ToolUse)
            .tool_name(format!("Tool1_{i}"))
            .decision(Decision::Allow)
            .build();
        log.log_event(&event).await.unwrap();
    }
    let event = AuditEvent::builder(session1.id, EventType::PolicyDecision)
        .decision(Decision::Deny)
        .reason("Blocked in session 1")
        .build();
    log.log_event(&event).await.unwrap();

    // Session 2: 5 events (all Allow)
    for i in 0..5 {
        let event = AuditEvent::builder(session2.id, EventType::ToolUse)
            .tool_name(format!("Tool2_{i}"))
            .decision(Decision::Allow)
            .build();
        log.log_event(&event).await.unwrap();
    }

    // Session 3: 2 events (1 Escalate, 1 Allow)
    let event = AuditEvent::builder(session3.id, EventType::AiEscalation)
        .decision(Decision::Escalate)
        .build();
    log.log_event(&event).await.unwrap();
    let event = AuditEvent::builder(session3.id, EventType::ToolUse)
        .decision(Decision::Allow)
        .build();
    log.log_event(&event).await.unwrap();

    // Log metrics for each session
    let mut metrics1 = SessionMetrics::new(session1.id);
    metrics1.add_tokens(1000, 500);
    log.log_metrics(&metrics1).await.unwrap();

    let mut metrics2 = SessionMetrics::new(session2.id);
    metrics2.add_tokens(2000, 1000);
    log.log_metrics(&metrics2).await.unwrap();

    let mut metrics3 = SessionMetrics::new(session3.id);
    metrics3.add_tokens(500, 250);
    log.log_metrics(&metrics3).await.unwrap();

    // End sessions
    log.log_session_end(session1.id, "Completed").await.unwrap();
    log.log_session_end(session2.id, "Success").await.unwrap();
    log.log_session_end(session3.id, "Done").await.unwrap();

    // Verify session isolation - each session should have its own events
    let events1 = log.get_events(session1.id, 100).await.unwrap();
    let events2 = log.get_events(session2.id, 100).await.unwrap();
    let events3 = log.get_events(session3.id, 100).await.unwrap();

    assert_eq!(events1.len(), 3, "Session 1 should have 3 events");
    assert_eq!(events2.len(), 5, "Session 2 should have 5 events");
    assert_eq!(events3.len(), 2, "Session 3 should have 2 events");

    // Verify all events belong to their respective sessions
    for event in &events1 {
        assert_eq!(event.session_id, session1.id);
    }
    for event in &events2 {
        assert_eq!(event.session_id, session2.id);
    }
    for event in &events3 {
        assert_eq!(event.session_id, session3.id);
    }

    // Verify metrics isolation
    let m1 = log.get_metrics(session1.id).await.unwrap().unwrap();
    let m2 = log.get_metrics(session2.id).await.unwrap().unwrap();
    let m3 = log.get_metrics(session3.id).await.unwrap().unwrap();

    assert_eq!(m1.input_tokens, 1000);
    assert_eq!(m2.input_tokens, 2000);
    assert_eq!(m3.input_tokens, 500);

    // Verify total counts across all sessions
    let total_events = log.count_events().await.unwrap();
    assert_eq!(total_events, 10, "Total events should be 3 + 5 + 2 = 10");

    // Count by decision across all sessions
    let allow_count = log.count_by_decision(Decision::Allow).await.unwrap();
    assert_eq!(allow_count, 8, "Total Allow should be 2 + 5 + 1 = 8");

    let deny_count = log.count_by_decision(Decision::Deny).await.unwrap();
    assert_eq!(deny_count, 1, "Total Deny should be 1");

    let escalate_count = log.count_by_decision(Decision::Escalate).await.unwrap();
    assert_eq!(escalate_count, 1, "Total Escalate should be 1");

    // Verify querying non-existent session returns empty
    let non_existent_id = Uuid::new_v4();
    let empty_events = log.get_events(non_existent_id, 100).await.unwrap();
    assert!(empty_events.is_empty());

    let no_metrics = log.get_metrics(non_existent_id).await.unwrap();
    assert!(no_metrics.is_none());
}

/// Test that in-memory database works correctly.
#[tokio::test]
async fn test_in_memory_database() {
    let log = AuditLog::open_in_memory()
        .await
        .expect("Failed to open in-memory database");

    // Verify path is None for in-memory
    assert!(log.path().is_none());

    // Verify basic operations work
    let session = AuditSession::new("In-memory test");
    log.log_session_start(&session).await.unwrap();

    let event = AuditEvent::builder(session.id, EventType::ToolUse)
        .tool_name("Test")
        .decision(Decision::Allow)
        .build();
    log.log_event(&event).await.unwrap();

    let count = log.count_events().await.unwrap();
    assert_eq!(count, 1);
}

/// Test concurrent access to audit log.
#[tokio::test]
async fn test_concurrent_logging() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_db_path(&temp_dir, "audit-concurrent");

    let log = AuditLog::open(&db_path)
        .await
        .expect("Failed to open audit log");

    let session = AuditSession::new("Concurrent test");
    log.log_session_start(&session).await.unwrap();

    // Clone the log for concurrent access (it uses Arc internally)
    let log_clone = log.clone();
    let session_id = session.id;

    // Spawn multiple tasks that log events concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let log = log_clone.clone();
        handles.push(tokio::spawn(async move {
            let event = AuditEvent::builder(session_id, EventType::ToolUse)
                .tool_name(format!("ConcurrentTool{i}"))
                .decision(Decision::Allow)
                .build();
            log.log_event(&event).await
        }));
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle
            .await
            .unwrap()
            .expect("Failed to log event concurrently");
    }

    // Verify all events were logged
    let count = log.count_events().await.unwrap();
    assert_eq!(count, 10);

    let events = log.get_events(session_id, 100).await.unwrap();
    assert_eq!(events.len(), 10);
}
