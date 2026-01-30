//! Configuration file loader.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::supervisor::PolicyLevel;

/// Policy configuration loaded from TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    /// Global policy level.
    pub level: PolicyLevel,
    /// Auto-continue without user prompts.
    pub auto_continue: bool,
    /// Bash command policies.
    pub bash: BashPolicy,
    /// File operation policies.
    pub files: FilesPolicy,
    /// Tool-specific policies.
    pub tools: ToolsPolicy,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            level: PolicyLevel::Permissive,
            auto_continue: false,
            bash: BashPolicy::default(),
            files: FilesPolicy::default(),
            tools: ToolsPolicy::default(),
        }
    }
}

/// Bash command policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BashPolicy {
    /// Block destructive commands (rm -rf, etc.).
    pub block_destructive: bool,
    /// Block network exfiltration (curl | sh, etc.).
    pub block_network_exfil: bool,
    /// Block privilege escalation (sudo, su).
    pub block_privilege_escalation: bool,
    /// Additional blocked command patterns.
    pub blocked_patterns: Vec<String>,
}

impl Default for BashPolicy {
    fn default() -> Self {
        Self {
            block_destructive: true,
            block_network_exfil: true,
            block_privilege_escalation: true,
            blocked_patterns: Vec::new(),
        }
    }
}

/// File operation policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesPolicy {
    /// Sensitive paths to block writes to.
    pub sensitive_paths: Vec<String>,
    /// Allow writes to .env files.
    pub allow_env_files: bool,
    /// Allow writes to SSH directory.
    pub allow_ssh_dir: bool,
}

impl Default for FilesPolicy {
    fn default() -> Self {
        Self {
            sensitive_paths: vec![
                "/etc/passwd".to_string(),
                "/etc/shadow".to_string(),
                "/etc/sudoers".to_string(),
                ".ssh/".to_string(),
                ".aws/".to_string(),
                ".gnupg/".to_string(),
                ".env".to_string(),
            ],
            allow_env_files: false,
            allow_ssh_dir: false,
        }
    }
}

/// Tool-specific policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolsPolicy {
    /// Tools to always allow.
    pub allowed: HashSet<String>,
    /// Tools to always deny.
    pub denied: HashSet<String>,
    /// Tools that require escalation.
    pub escalate: HashSet<String>,
}

impl Default for ToolsPolicy {
    fn default() -> Self {
        Self {
            allowed: ["Read", "Glob", "Grep"]
                .into_iter()
                .map(String::from)
                .collect(),
            denied: HashSet::new(),
            escalate: HashSet::new(),
        }
    }
}

/// Configuration loader that searches multiple locations.
#[derive(Debug)]
pub struct ConfigLoader {
    /// Search paths in order of priority.
    search_paths: Vec<PathBuf>,
}

impl ConfigLoader {
    /// Create a new config loader with default search paths.
    #[must_use]
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        // 1. Current directory: .claude-supervisor.toml
        search_paths.push(PathBuf::from(".claude-supervisor.toml"));

        // 2. User config directory: ~/.config/claude-supervisor/config.toml
        if let Some(config_dir) = dirs::config_dir() {
            search_paths.push(config_dir.join("claude-supervisor").join("config.toml"));
        }

        Self { search_paths }
    }

    /// Create a config loader with a specific config file path.
    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            search_paths: vec![path],
        }
    }

    /// Load configuration from the first available file, or return defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if a config file exists but cannot be parsed.
    pub fn load(&self) -> Result<PolicyConfig, ConfigError> {
        for path in &self.search_paths {
            if path.exists() {
                tracing::debug!(path = %path.display(), "Loading config file");
                return Self::load_from_path(path);
            }
        }

        tracing::debug!("No config file found, using defaults");
        Ok(PolicyConfig::default())
    }

    /// Load configuration from a specific path.
    fn load_from_path(path: &PathBuf) -> Result<PolicyConfig, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::ReadError {
            path: path.clone(),
            source: e,
        })?;

        toml::from_str(&content).map_err(|e| ConfigError::ParseError {
            path: path.clone(),
            source: e,
        })
    }

    /// Get the search paths for debugging.
    #[must_use]
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Find the first config file that exists.
    #[must_use]
    pub fn find_config_file(&self) -> Option<PathBuf> {
        self.search_paths.iter().find(|p| p.exists()).cloned()
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse config file {path}: {source}")]
    ParseError {
        path: PathBuf,
        source: toml::de::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_config() {
        let config = PolicyConfig::default();
        assert_eq!(config.level, PolicyLevel::Permissive);
        assert!(!config.auto_continue);
        assert!(config.bash.block_destructive);
        assert!(config.tools.allowed.contains("Read"));
    }

    #[test]
    fn test_config_loader_default_paths() {
        let loader = ConfigLoader::new();
        assert!(!loader.search_paths().is_empty());
        assert!(loader.search_paths()[0].ends_with(".claude-supervisor.toml"));
    }

    #[test]
    fn test_config_loader_returns_defaults_when_no_file() {
        let loader = ConfigLoader::with_path(PathBuf::from("/nonexistent/path.toml"));
        let config = loader.load().unwrap();
        assert_eq!(config.level, PolicyLevel::Permissive);
    }

    #[test]
    fn test_parse_toml_config() {
        let toml_str = r#"
            level = "strict"
            auto_continue = true

            [bash]
            block_destructive = true
            block_network_exfil = false

            [files]
            allow_env_files = true

            [tools]
            allowed = ["Read", "Write"]
            denied = ["Bash"]
        "#;

        let config: PolicyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.level, PolicyLevel::Strict);
        assert!(config.auto_continue);
        assert!(config.bash.block_destructive);
        assert!(!config.bash.block_network_exfil);
        assert!(config.files.allow_env_files);
        assert!(config.tools.allowed.contains("Read"));
        assert!(config.tools.denied.contains("Bash"));
    }
}
