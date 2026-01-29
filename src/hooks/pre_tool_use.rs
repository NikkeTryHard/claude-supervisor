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

/// Response from a `PreToolUse` hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseResponse {
    pub permission_decision: PermissionDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
}

impl PreToolUseResponse {
    #[must_use]
    pub fn allow() -> Self {
        Self {
            permission_decision: PermissionDecision::Allow,
            reason: None,
            updated_input: None,
        }
    }

    #[must_use]
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            permission_decision: PermissionDecision::Deny,
            reason: Some(reason.into()),
            updated_input: None,
        }
    }

    #[must_use]
    pub fn ask() -> Self {
        Self {
            permission_decision: PermissionDecision::Ask,
            reason: None,
            updated_input: None,
        }
    }
}
