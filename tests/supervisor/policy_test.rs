//! Integration tests for policy engine.

use claude_supervisor::supervisor::{Blocklist, PolicyDecision, PolicyEngine, PolicyLevel};
use serde_json::json;

#[test]
fn policy_level_default() {
    let level = PolicyLevel::default();
    assert_eq!(level, PolicyLevel::Permissive);
}

#[test]
fn policy_level_serialization() {
    let levels = [
        PolicyLevel::Permissive,
        PolicyLevel::Moderate,
        PolicyLevel::Strict,
    ];

    for level in levels {
        let json = serde_json::to_string(&level).expect("Should serialize");
        let deserialized: PolicyLevel = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(level, deserialized);
    }
}

#[test]
fn policy_decision_serialization() {
    let decisions = [
        PolicyDecision::Allow,
        PolicyDecision::Deny("Test denial".to_string()),
        PolicyDecision::Escalate("Test escalation".to_string()),
    ];

    for decision in decisions {
        let json = serde_json::to_string(&decision).expect("Should serialize");
        let _: PolicyDecision = serde_json::from_str(&json).expect("Should deserialize");
    }
}

#[test]
fn policy_engine_permissive_allows_unknown_tools() {
    let engine = PolicyEngine::new(PolicyLevel::Permissive);

    let decision = engine.evaluate("SomeUnknownTool", &json!({}));
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn policy_engine_moderate_escalates_unknown_tools() {
    let engine = PolicyEngine::new(PolicyLevel::Moderate);

    let decision = engine.evaluate("SomeUnknownTool", &json!({}));
    assert!(matches!(decision, PolicyDecision::Escalate(_)));
}

#[test]
fn policy_engine_strict_escalates_unknown_tools() {
    let engine = PolicyEngine::new(PolicyLevel::Strict);

    let decision = engine.evaluate("SomeUnknownTool", &json!({}));
    assert!(matches!(decision, PolicyDecision::Escalate(_)));
}

#[test]
fn policy_engine_explicit_allow() {
    let mut engine = PolicyEngine::new(PolicyLevel::Strict);
    engine.allow_tool("TrustedTool");

    let decision = engine.evaluate("TrustedTool", &json!({}));
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn policy_engine_explicit_deny() {
    let mut engine = PolicyEngine::new(PolicyLevel::Permissive);
    engine.deny_tool("ForbiddenTool");

    let decision = engine.evaluate("ForbiddenTool", &json!({}));
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn policy_engine_bash_blocklist_integration() {
    let engine = PolicyEngine::new(PolicyLevel::Permissive);

    // Dangerous commands should be blocked
    let decision = engine.evaluate("Bash", &json!({ "command": "rm -rf /" }));
    assert!(matches!(decision, PolicyDecision::Deny(_)));

    // Safe commands should be allowed
    let decision = engine.evaluate("Bash", &json!({ "command": "ls -la" }));
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn policy_engine_write_sensitive_path_protection() {
    let engine = PolicyEngine::new(PolicyLevel::Permissive);

    // Sensitive paths should be blocked
    let decision = engine.evaluate("Write", &json!({ "file_path": "/etc/passwd" }));
    assert!(matches!(decision, PolicyDecision::Deny(_)));

    let decision = engine.evaluate("Write", &json!({ "file_path": "~/.ssh/authorized_keys" }));
    assert!(matches!(decision, PolicyDecision::Deny(_)));

    let decision = engine.evaluate("Write", &json!({ "file_path": "/project/.env" }));
    assert!(matches!(decision, PolicyDecision::Deny(_)));

    // Safe paths should be allowed
    let decision = engine.evaluate(
        "Write",
        &json!({ "file_path": "/home/user/project/src/main.rs" }),
    );
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn policy_engine_with_custom_blocklist() {
    let blocklist = Blocklist::new(); // Empty blocklist
    let engine = PolicyEngine::with_blocklist(PolicyLevel::Permissive, blocklist);

    // With empty blocklist, dangerous commands should be allowed
    let decision = engine.evaluate("Bash", &json!({ "command": "rm -rf /" }));
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn policy_engine_accessors() {
    let engine = PolicyEngine::new(PolicyLevel::Moderate);

    assert_eq!(engine.level(), PolicyLevel::Moderate);
    assert!(!engine.blocklist().is_empty());
}
