//! Policy engine for evaluating tool calls.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Policy strictness level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyLevel {
    #[default]
    Permissive,
    Moderate,
    Strict,
}

/// Decision from policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny(String),
    Escalate(String),
}

/// Policy engine for evaluating tool calls against configured rules.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    level: PolicyLevel,
    allowed_tools: HashSet<String>,
    denied_tools: HashSet<String>,
}

impl PolicyEngine {
    #[must_use]
    pub fn new(level: PolicyLevel) -> Self {
        Self {
            level,
            allowed_tools: HashSet::new(),
            denied_tools: HashSet::new(),
        }
    }

    /// Evaluate a tool call against the policy.
    #[must_use]
    pub fn evaluate(&self, tool_name: &str, _tool_input: &serde_json::Value) -> PolicyDecision {
        if self.denied_tools.contains(tool_name) {
            return PolicyDecision::Deny(format!("Tool '{tool_name}' is explicitly denied"));
        }
        if self.allowed_tools.contains(tool_name) {
            return PolicyDecision::Allow;
        }
        match self.level {
            PolicyLevel::Permissive => PolicyDecision::Allow,
            PolicyLevel::Moderate => {
                PolicyDecision::Escalate(format!("Tool '{tool_name}' requires supervisor approval"))
            }
            PolicyLevel::Strict => PolicyDecision::Escalate(format!(
                "Strict mode: Tool '{tool_name}' requires supervisor approval"
            )),
        }
    }

    /// Add a tool to the allowed list.
    pub fn allow_tool(&mut self, tool: impl Into<String>) {
        self.allowed_tools.insert(tool.into());
    }

    /// Add a tool to the denied list.
    pub fn deny_tool(&mut self, tool: impl Into<String>) {
        self.denied_tools.insert(tool.into());
    }
}
