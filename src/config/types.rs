//! Configuration types.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::supervisor::PolicyLevel;

use super::{StopConfig, WorktreeConfig};

/// AI provider kind.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Gemini,
    Claude,
}

/// Configuration for the AI supervisor client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Provider to use (gemini or claude).
    #[serde(default)]
    pub provider: ProviderKind,
    /// Model to use for supervision.
    #[serde(default = "default_model")]
    pub model: String,
    /// Maximum tokens in response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Base URL for the API.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Environment variable name for the API key.
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
}

fn default_model() -> String {
    "gemini-3-flash".to_string()
}

fn default_max_tokens() -> u32 {
    65536
}

fn default_base_url() -> String {
    "http://host.docker.internal:8045/v1beta".to_string()
}

fn default_api_key_env() -> String {
    "GEMINI_API_KEY".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::default(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            base_url: default_base_url(),
            api_key_env: default_api_key_env(),
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
    /// Show detailed activity output.
    #[serde(default)]
    pub show_activity: bool,
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
            ai_supervisor: true,
            stop: StopConfig::default(),
            worktree: WorktreeConfig::default(),
            show_activity: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_config_defaults() {
        let config = AiConfig::default();
        assert_eq!(config.provider, ProviderKind::Gemini);
        assert_eq!(config.model, "gemini-3-flash");
        assert_eq!(config.max_tokens, 65536);
        assert_eq!(config.base_url, "http://host.docker.internal:8045/v1beta");
        assert_eq!(config.api_key_env, "GEMINI_API_KEY");
    }

    #[test]
    fn test_ai_config_deserialize_gemini() {
        let toml = r#"
            provider = "gemini"
            model = "gemini-3-flash"
            max_tokens = 512
            base_url = "http://localhost:8045/v1beta"
            api_key_env = "GEMINI_API_KEY"
        "#;
        let config: AiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.provider, ProviderKind::Gemini);
        assert_eq!(config.model, "gemini-3-flash");
        assert_eq!(config.max_tokens, 512);
    }

    #[test]
    fn test_ai_config_deserialize_claude() {
        let toml = r#"
            provider = "claude"
            model = "claude-sonnet-4-20250514"
            max_tokens = 2048
            base_url = "https://api.anthropic.com"
            api_key_env = "ANTHROPIC_API_KEY"
        "#;
        let config: AiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.provider, ProviderKind::Claude);
        assert_eq!(config.model, "claude-sonnet-4-20250514");
        assert_eq!(config.base_url, "https://api.anthropic.com");
    }

    #[test]
    fn test_supervisor_config_show_activity_default_false() {
        let config = SupervisorConfig::default();
        assert!(!config.show_activity);
    }
}
