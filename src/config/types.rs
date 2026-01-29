//! Configuration types.

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
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default)]
    pub ai_supervisor: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            policy: PolicyLevel::Permissive,
            auto_continue: false,
            allowed_tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
            denied_tools: Vec::new(),
            ai_supervisor: false,
        }
    }
}
