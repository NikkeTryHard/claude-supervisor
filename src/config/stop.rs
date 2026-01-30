//! Stop hook configuration.

use serde::{Deserialize, Serialize};

/// Configuration for the Stop hook handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopConfig {
    /// Maximum number of iterations before allowing stop.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Whether to force continue on stop events.
    #[serde(default)]
    pub force_continue: bool,

    /// Phrases that indicate the task is complete.
    #[serde(default = "default_completion_phrases")]
    pub completion_phrases: Vec<String>,

    /// Phrases that indicate the task is incomplete.
    #[serde(default = "default_incomplete_phrases")]
    pub incomplete_phrases: Vec<String>,
}

fn default_max_iterations() -> u32 {
    50
}

fn default_completion_phrases() -> Vec<String> {
    vec![
        "task is complete".to_string(),
        "successfully completed".to_string(),
        "all done".to_string(),
        "finished successfully".to_string(),
        "completed all tasks".to_string(),
    ]
}

fn default_incomplete_phrases() -> Vec<String> {
    vec![
        "now i'll".to_string(),
        "next step".to_string(),
        "let me also".to_string(),
        "i'll now".to_string(),
        "next, i".to_string(),
        "moving on to".to_string(),
    ]
}

impl Default for StopConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            force_continue: false,
            completion_phrases: default_completion_phrases(),
            incomplete_phrases: default_incomplete_phrases(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_config_default() {
        let config = StopConfig::default();
        assert_eq!(config.max_iterations, 50);
        assert!(!config.force_continue);
        assert!(!config.completion_phrases.is_empty());
        assert!(!config.incomplete_phrases.is_empty());
    }

    #[test]
    fn test_stop_config_deserialize_defaults() {
        let json = "{}";
        let config: StopConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_iterations, 50);
        assert!(!config.force_continue);
    }

    #[test]
    fn test_stop_config_deserialize_custom() {
        let json = r#"{
            "max_iterations": 100,
            "force_continue": true,
            "completion_phrases": ["done"],
            "incomplete_phrases": ["not done"]
        }"#;
        let config: StopConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_iterations, 100);
        assert!(config.force_continue);
        assert_eq!(config.completion_phrases, vec!["done"]);
        assert_eq!(config.incomplete_phrases, vec!["not done"]);
    }
}
