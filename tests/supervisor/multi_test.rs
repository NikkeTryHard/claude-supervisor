use claude_supervisor::supervisor::{
    AggregatedStats, MultiSessionError, MultiSessionSupervisor, PolicyEngine, PolicyLevel,
    SessionMeta,
};

#[test]
fn test_multi_session_error_display() {
    let err = MultiSessionError::MaxSessionsReached { limit: 3 };
    assert_eq!(err.to_string(), "Maximum sessions reached: 3");
}

#[test]
fn test_multi_session_error_session_not_found() {
    let err = MultiSessionError::SessionNotFound {
        id: "test-123".to_string(),
    };
    assert!(err.to_string().contains("test-123"));
}

#[test]
fn test_multi_session_supervisor_new() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let supervisor = MultiSessionSupervisor::new(3, policy);

    assert_eq!(supervisor.max_sessions(), 3);
    assert_eq!(supervisor.active_count(), 0);
    assert!(!supervisor.has_pending());
}

#[test]
fn test_aggregated_stats_default() {
    let stats = AggregatedStats::default();
    assert_eq!(stats.sessions_completed, 0);
    assert_eq!(stats.sessions_failed, 0);
    assert_eq!(stats.total_tool_calls, 0);
}

#[test]
fn test_session_meta_cancellation() {
    let meta = SessionMeta::new("test-id".to_string(), "test task".to_string());

    assert!(!meta.is_cancelled());
    assert_eq!(meta.id, "test-id");
    assert_eq!(meta.task, "test task");

    meta.cancel();
    assert!(meta.is_cancelled());
}

#[tokio::test]
async fn test_spawn_session_returns_id() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let id = supervisor
        .spawn_session("Test task".to_string())
        .await
        .unwrap();

    assert!(!id.is_empty());
    assert_eq!(supervisor.active_count(), 1);
}

#[tokio::test]
async fn test_spawn_session_respects_limit() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(2, policy);

    // Spawn two sessions (at limit)
    let _id1 = supervisor
        .spawn_session("Task 1".to_string())
        .await
        .unwrap();
    let _id2 = supervisor
        .spawn_session("Task 2".to_string())
        .await
        .unwrap();

    // Third should fail with try_spawn (non-blocking)
    let result = supervisor.try_spawn_session("Task 3");
    assert!(matches!(
        result,
        Err(MultiSessionError::MaxSessionsReached { limit: 2 })
    ));
}

#[tokio::test]
async fn test_stop_session() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let id = supervisor
        .spawn_session("Long task".to_string())
        .await
        .unwrap();

    // Stop the session
    supervisor.stop_session(&id).unwrap();

    // Session should be marked as cancelled
    let meta = supervisor.get_session(&id).unwrap();
    assert!(meta.is_cancelled());
}

#[tokio::test]
async fn test_stop_nonexistent_session() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let supervisor = MultiSessionSupervisor::new(3, policy);

    let result = supervisor.stop_session("nonexistent");
    assert!(matches!(
        result,
        Err(MultiSessionError::SessionNotFound { .. })
    ));
}

#[tokio::test]
async fn test_stop_all_sessions() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(5, policy);

    let id1 = supervisor
        .spawn_session("Task 1".to_string())
        .await
        .unwrap();
    let id2 = supervisor
        .spawn_session("Task 2".to_string())
        .await
        .unwrap();

    supervisor.stop_all();

    assert!(supervisor.get_session(&id1).unwrap().is_cancelled());
    assert!(supervisor.get_session(&id2).unwrap().is_cancelled());
}
