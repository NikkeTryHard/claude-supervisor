//! Supervisor module tests.

mod blocklist_test;
mod policy_test;
mod runner_test;

/// Verify all public supervisor types are exported from the library.
#[test]
fn test_all_supervisor_types_exported() {
    use claude_supervisor::supervisor::{
        Blocklist, BlocklistError, BlocklistRule, PolicyDecision, PolicyEngine, PolicyLevel,
        RuleCategory, SessionState, SessionStateMachine, Supervisor, SupervisorError,
        SupervisorResult,
    };

    // Verify types are constructible
    let _ = PolicyEngine::new(PolicyLevel::Permissive);
    let _ = Blocklist::new();
    let _ = SessionStateMachine::new();

    // Verify Supervisor can be created with a channel
    let (_tx, rx) = tokio::sync::mpsc::channel(1);
    let _ = Supervisor::new(PolicyEngine::new(PolicyLevel::Permissive), rx);

    // Verify error types exist
    let _: fn() -> SupervisorError = || SupervisorError::NoStdout;
    let _: fn(&str) -> Result<BlocklistRule, BlocklistError> =
        |pattern| BlocklistRule::new(RuleCategory::Destructive, pattern, "test");

    // Verify enum variants
    let _ = PolicyDecision::Allow;
    let _ = SessionState::Idle;
    let _ = SupervisorResult::ProcessExited;
}
