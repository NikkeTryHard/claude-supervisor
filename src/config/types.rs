//! Configuration types.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::supervisor::PolicyLevel;

/// Configuration for the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    #[serde(default)]
    pub policy: PolicyLevel,
    #[serde(default)]
    pub auto_continue: bool,
    #[serde(default)]
    pub allowed_tools: HashSet<String>,
    #[serde(default)]
    pub denied_tools: HashSet<String>,
    #[serde(default)]
    pub ai_supervisor: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            policy: PolicyLevel::Permissive,
            auto_continue: false,
            allowed_tools: ["Read", "Glob", "Grep"]
                .into_iter()
                .map(String::from)
                .collect(),
            denied_tools: HashSet::new(),
            ai_supervisor: false,
        }
    }
}
