//! Claude API client wrapper for supervisor decisions.

use clust::messages::{ClaudeModel, MaxTokens, Message, MessagesRequestBody, SystemPrompt};
use clust::{ApiKey, Client};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::AiConfig;

use super::SUPERVISOR_SYSTEM_PROMPT;

/// Parse a model string into a `ClaudeModel` enum variant.
///
/// Falls back to `ClaudeModel::Claude35Sonnet20240620` if the string doesn't match
/// any known model.
fn parse_model(model_str: &str) -> ClaudeModel {
    match model_str {
        "claude-3-opus-20240229" => ClaudeModel::Claude3Opus20240229,
        "claude-3-sonnet-20240229" => ClaudeModel::Claude3Sonnet20240229,
        "claude-3-haiku-20240307" => ClaudeModel::Claude3Haiku20240307,
        "claude-3-5-sonnet-20240620" => ClaudeModel::Claude35Sonnet20240620,
        // Default to Claude 3.5 Sonnet for any unrecognized model string
        _ => {
            tracing::warn!(
                model = %model_str,
                fallback = "claude-3-5-sonnet-20240620",
                "Unrecognized model string, using fallback"
            );
            ClaudeModel::Claude35Sonnet20240620
        }
    }
}

/// Decision from the AI supervisor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "decision", rename_all = "UPPERCASE")]
pub enum SupervisorDecision {
    /// Allow the tool call to proceed.
    Allow { reason: String },
    /// Deny the tool call.
    Deny { reason: String },
    /// Allow with corrective guidance.
    Guide { reason: String, guidance: String },
}

/// Errors from AI client operations.
#[derive(Error, Debug)]
pub enum AiError {
    #[error("API key not configured")]
    MissingApiKey,
    #[error("API request failed: {0}")]
    RequestFailed(String),
    #[error("Failed to parse response: {0}")]
    ParseError(String),
    #[error("AI supervisor request timed out")]
    Timeout,
}

/// Client for making AI supervisor decisions.
#[derive(Debug, Clone)]
pub struct AiClient {
    api_key: String,
    config: AiConfig,
}

impl AiClient {
    /// Create a new client with the given API key and config.
    #[must_use]
    pub fn new(api_key: String, config: AiConfig) -> Self {
        Self { api_key, config }
    }

    /// Create client from environment variables.
    ///
    /// # Errors
    ///
    /// Returns `AiError::MissingApiKey` if the `ANTHROPIC_API_KEY` environment
    /// variable is not set.
    pub fn from_env() -> Result<Self, AiError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| AiError::MissingApiKey)?;
        Ok(Self {
            api_key,
            config: AiConfig::default(),
        })
    }

    /// Create client from environment variables with custom config.
    ///
    /// # Errors
    ///
    /// Returns `AiError::MissingApiKey` if the `ANTHROPIC_API_KEY` environment
    /// variable is not set.
    pub fn from_env_with_config(config: AiConfig) -> Result<Self, AiError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| AiError::MissingApiKey)?;
        Ok(Self { api_key, config })
    }

    /// Check if the client is configured with an API key.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Get the configured model.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Ask the AI supervisor whether to allow a tool call.
    ///
    /// # Errors
    ///
    /// Returns `AiError::RequestFailed` if the API request fails.
    /// Returns `AiError::ParseError` if the response cannot be parsed.
    pub async fn ask_supervisor(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        context: &str,
    ) -> Result<SupervisorDecision, AiError> {
        let client = Client::from_api_key(ApiKey::new(&self.api_key));

        // Parse the configured model string into a ClaudeModel enum
        let model = parse_model(&self.config.model);

        let max_tokens = MaxTokens::new(self.config.max_tokens, model)
            .map_err(|e| AiError::RequestFailed(format!("Invalid max_tokens: {e}")))?;

        let user_message = format!(
            "Context: {context}\n\nTool: {tool_name}\nInput: {}",
            serde_json::to_string_pretty(tool_input).unwrap_or_else(|_| tool_input.to_string())
        );

        let request = MessagesRequestBody {
            model,
            messages: vec![Message::user(user_message)],
            max_tokens,
            system: Some(SystemPrompt::new(SUPERVISOR_SYSTEM_PROMPT)),
            ..Default::default()
        };

        let response = client
            .create_a_message(request)
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        // Extract text from the response
        let text = response
            .content
            .flatten_into_text()
            .map_err(|e| AiError::ParseError(format!("No text in response: {e}")))?;

        // Parse the JSON decision from the response
        extract_decision(text)
    }
}

/// Extract a `SupervisorDecision` from the AI response text.
///
/// Looks for JSON in the response and parses it.
fn extract_decision(text: &str) -> Result<SupervisorDecision, AiError> {
    // Try to find JSON in the response
    // Look for patterns like {"decision": ...}
    let json_start = text
        .find('{')
        .ok_or_else(|| AiError::ParseError(format!("No JSON object found in response: {text}")))?;

    // Find the matching closing brace
    let mut depth = 0;
    let mut json_end = json_start;
    for (i, c) in text[json_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    json_end = json_start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    let json_str = &text[json_start..json_end];
    serde_json::from_str(json_str)
        .map_err(|e| AiError::ParseError(format!("Failed to parse decision JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_allow_decision() {
        let json = r#"{"decision": "ALLOW", "reason": "Safe operation"}"#;
        let decision: SupervisorDecision = serde_json::from_str(json).unwrap();
        assert!(matches!(decision, SupervisorDecision::Allow { .. }));
    }

    #[test]
    fn test_parse_deny_decision() {
        let json = r#"{"decision": "DENY", "reason": "Risky operation"}"#;
        let decision: SupervisorDecision = serde_json::from_str(json).unwrap();
        assert!(matches!(decision, SupervisorDecision::Deny { .. }));
    }

    #[test]
    fn test_parse_guide_decision() {
        let json =
            r#"{"decision": "GUIDE", "reason": "Needs adjustment", "guidance": "Use safer path"}"#;
        let decision: SupervisorDecision = serde_json::from_str(json).unwrap();
        assert!(matches!(decision, SupervisorDecision::Guide { .. }));
    }

    #[test]
    fn test_extract_decision_simple() {
        let text = r#"{"decision": "ALLOW", "reason": "Safe"}"#;
        let decision = extract_decision(text).unwrap();
        assert!(matches!(decision, SupervisorDecision::Allow { .. }));
    }

    #[test]
    fn test_extract_decision_with_surrounding_text() {
        let text = r#"Here is my decision: {"decision": "DENY", "reason": "Dangerous"} That's it."#;
        let decision = extract_decision(text).unwrap();
        assert!(matches!(decision, SupervisorDecision::Deny { .. }));
    }

    #[test]
    fn test_extract_decision_no_json() {
        let text = "No JSON here";
        let result = extract_decision(text);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_env_missing_key() {
        // Temporarily unset the env var
        let original = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");

        let result = AiClient::from_env();
        assert!(matches!(result, Err(AiError::MissingApiKey)));

        // Restore if it was set
        if let Some(key) = original {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        }
    }
}
