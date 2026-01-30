//! `PreToolUse` hook handler.

use serde::{Deserialize, Serialize};

/// Decision for a `PreToolUse` hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

/// Inner content of a `PreToolUse` hook response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseOutput {
    pub hook_event_name: String,
    pub permission_decision: PermissionDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
}

/// Response from a `PreToolUse` hook wrapped in hookSpecificOutput.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseResponse {
    pub hook_specific_output: PreToolUseOutput,
}

impl PreToolUseResponse {
    #[must_use]
    pub fn allow() -> Self {
        Self {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: PermissionDecision::Allow,
                permission_decision_reason: None,
                updated_input: None,
            },
        }
    }

    #[must_use]
    pub fn allow_with_reason(reason: impl Into<String>) -> Self {
        Self {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: PermissionDecision::Allow,
                permission_decision_reason: Some(reason.into()),
                updated_input: None,
            },
        }
    }

    #[must_use]
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: PermissionDecision::Deny,
                permission_decision_reason: Some(reason.into()),
                updated_input: None,
            },
        }
    }

    #[must_use]
    pub fn ask(reason: impl Into<String>) -> Self {
        Self {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: PermissionDecision::Ask,
                permission_decision_reason: Some(reason.into()),
                updated_input: None,
            },
        }
    }

    /// Allow the tool call with modified input parameters.
    #[must_use]
    pub fn allow_with_modification(updated_input: serde_json::Value) -> Self {
        Self {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: PermissionDecision::Allow,
                permission_decision_reason: Some("Input modified by supervisor".to_string()),
                updated_input: Some(updated_input),
            },
        }
    }

    /// Get the permission decision.
    #[must_use]
    pub fn decision(&self) -> PermissionDecision {
        self.hook_specific_output.permission_decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_response_format() {
        let response = PreToolUseResponse::allow();
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("hookSpecificOutput"));
        assert!(json.contains("\"permissionDecision\":\"allow\""));
        assert!(json.contains("\"hookEventName\":\"PreToolUse\""));
    }

    #[test]
    fn test_deny_response_format() {
        let response = PreToolUseResponse::deny("Blocked command");
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"permissionDecision\":\"deny\""));
        assert!(json.contains("\"permissionDecisionReason\":\"Blocked command\""));
    }

    #[test]
    fn test_ask_response_format() {
        let response = PreToolUseResponse::ask("Requires approval");
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"permissionDecision\":\"ask\""));
    }

    #[test]
    fn test_allow_with_modification_response_format() {
        let modified_input = serde_json::json!({ "command": "ls -la" });
        let response = PreToolUseResponse::allow_with_modification(modified_input);
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"permissionDecision\":\"allow\""));
        assert!(json.contains("\"updatedInput\""));
        assert!(json.contains("\"command\":\"ls -la\""));
    }
}
