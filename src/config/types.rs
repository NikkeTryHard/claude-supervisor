//! Configuration types.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::supervisor::PolicyLevel;

use super::{StopConfig, WorktreeConfig};

/// Configuration for the AI supervisor client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Model to use for supervision.
    #[serde(default = "default_model")]
    pub model: String,
    /// Maximum tokens in response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_model() -> String {
    "claude-3-5-sonnet-20240620".to_string()
}

fn default_max_tokens() -> u32 {
    1024
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            max_tokens: default_max_tokens(),
        }
    }
}

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
    #[serde(default)]
    pub stop: StopConfig,
    #[serde(default)]
    pub worktree: WorktreeConfig,
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
            stop: StopConfig::default(),
            worktree: WorktreeConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_config_defaults() {
        let config = AiConfig::default();
        assert_eq!(config.model, "claude-3-5-sonnet-20240620");
        assert_eq!(config.max_tokens, 1024);
    }

    #[test]
    fn test_ai_config_deserialize() {
        let toml = r#"
            model = "claude-haiku-4-20250514"
            max_tokens = 512
        "#;
        let config: AiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.model, "claude-haiku-4-20250514");
        assert_eq!(config.max_tokens, 512);
    }
}
