//! Integration tests for multi-session supervisor.

use claude_supervisor::supervisor::{
    MultiSessionError, MultiSessionSupervisor, PolicyEngine, PolicyLevel, SupervisorResult,
};

#[tokio::test]
async fn test_multi_session_full_lifecycle() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(2, policy);

    // Verify initial state
    assert_eq!(supervisor.max_sessions(), 2);
    assert_eq!(supervisor.active_count(), 0);
    assert!(!supervisor.has_pending());

    // Spawn sessions
    let id1 = supervisor
        .spawn_session("Integration test 1".to_string())
        .await
        .unwrap();
    let _id2 = supervisor
        .spawn_session("Integration test 2".to_string())
        .await
        .unwrap();

    assert_eq!(supervisor.active_count(), 2);
    assert!(supervisor.has_pending());

    // Verify session metadata
    let meta1 = supervisor.get_session(&id1).unwrap();
    assert_eq!(meta1.task, "Integration test 1");
    assert!(!meta1.is_cancelled());

    // Wait for completion
    let results = supervisor.wait_all().await;
    assert_eq!(results.len(), 2);
    assert_eq!(supervisor.active_count(), 0);
    assert!(!supervisor.has_pending());

    // Verify stats
    let stats = supervisor.stats();
    assert!(stats.sessions_completed + stats.sessions_failed == 2);
}

#[tokio::test]
async fn test_multi_session_concurrent_limit() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(1, policy);

    // Spawn one session (at limit)
    let _id = supervisor.spawn_session("Task".to_string()).await.unwrap();

    // Try to spawn another (should fail with try_spawn)
    let result = supervisor.try_spawn_session("Another task");
    assert!(matches!(
        result,
        Err(MultiSessionError::MaxSessionsReached { limit: 1 })
    ));
}

#[tokio::test]
async fn test_multi_session_stop_and_wait() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let id = supervisor
        .spawn_session("Long running task".to_string())
        .await
        .unwrap();

    // Stop the session
    supervisor.stop_session(&id).unwrap();

    // Wait for completion
    let results = supervisor.wait_all().await;
    assert_eq!(results.len(), 1);

    // Session should have been cancelled
    let result = &results[0];
    assert!(matches!(result.result, Ok(SupervisorResult::Cancelled)));
}

#[tokio::test]
async fn test_multi_session_shared_policy() {
    let mut policy = PolicyEngine::new(PolicyLevel::Strict);
    policy.allow_tool("Read");
    policy.deny_tool("Bash");

    let supervisor = MultiSessionSupervisor::new(3, policy);

    // All sessions share the same policy
    let shared_policy = supervisor.policy();
    // Policy should be accessible
    assert!(std::sync::Arc::strong_count(&shared_policy) >= 1);
}

#[tokio::test]
async fn test_spawn_and_wait_all_convenience() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(5, policy);

    let tasks = vec![
        "Task A".to_string(),
        "Task B".to_string(),
        "Task C".to_string(),
    ];

    let results = supervisor.spawn_and_wait_all(tasks).await.unwrap();

    assert_eq!(results.len(), 3);

    let task_names: Vec<&str> = results.iter().map(|r| r.task.as_str()).collect();
    assert!(task_names.contains(&"Task A"));
    assert!(task_names.contains(&"Task B"));
    assert!(task_names.contains(&"Task C"));
}

#[tokio::test]
async fn test_aggregated_stats_accuracy() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(5, policy);

    // Spawn 3 sessions
    for i in 1..=3 {
        supervisor.spawn_session(format!("Task {i}")).await.unwrap();
    }

    // Wait for all
    let results = supervisor.wait_all().await;
    assert_eq!(results.len(), 3);

    // Verify stats
    let stats = supervisor.stats();
    assert_eq!(stats.sessions_completed, 3);
    assert_eq!(stats.sessions_failed, 0);
}
