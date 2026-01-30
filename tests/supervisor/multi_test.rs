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
