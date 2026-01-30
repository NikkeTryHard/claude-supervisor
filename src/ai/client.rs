//! Multi-provider AI client for supervisor decisions.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::{AiConfig, ProviderKind};

use super::SUPERVISOR_SYSTEM_PROMPT;

/// Connection timeout for HTTP requests.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Overall request timeout for HTTP requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of retries for transient failures.
const MAX_RETRIES: u32 = 3;

/// Build an HTTP client with proper timeout configuration.
fn build_http_client() -> Client {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("Failed to build HTTP client")
}

/// Determine if a request should be retried based on status code and attempt count.
fn should_retry(status_code: u16, attempt: u32) -> bool {
    if attempt >= MAX_RETRIES {
        return false;
    }
    // Retry on 5xx server errors
    (500..600).contains(&status_code)
}

/// Calculate exponential backoff duration for retry attempts.
fn calculate_backoff(attempt: u32) -> Duration {
    // Exponential backoff: 1s, 2s, 4s
    Duration::from_secs(1 << attempt)
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
    #[error("API key not configured (env: {0})")]
    MissingApiKey(String),
    #[error("API request failed: {0}")]
    RequestFailed(String),
    #[error("Failed to parse response: {0}")]
    ParseError(String),
    #[error("AI supervisor request timed out")]
    Timeout,
}

/// Trait for AI providers.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Generate a response from the AI provider.
    async fn generate(&self, system: &str, user: &str) -> Result<String, AiError>;
}

/// Gemini API provider.
#[derive(Debug, Clone)]
pub struct GeminiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl GeminiProvider {
    /// Create a new Gemini provider.
    #[must_use]
    pub fn new(base_url: String, api_key: String, model: String, max_tokens: u32) -> Self {
        Self {
            client: build_http_client(),
            base_url,
            api_key,
            model,
            max_tokens,
        }
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
    async fn generate(&self, system: &str, user: &str) -> Result<String, AiError> {
        let url = format!(
            "{}/models/{}:generateContent",
            self.base_url.trim_end_matches('/'),
            self.model
        );

        let body = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": user }]
            }],
            "systemInstruction": {
                "parts": [{ "text": system }]
            },
            "generationConfig": {
                "maxOutputTokens": self.max_tokens
            }
        });

        let mut attempt = 0;
        loop {
            let response = self
                .client
                .post(&url)
                .header("x-goog-api-key", &self.api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        AiError::Timeout
                    } else {
                        AiError::RequestFailed(e.to_string())
                    }
                })?;

            let status = response.status();
            if status.is_success() {
                let json: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| AiError::ParseError(e.to_string()))?;

                // Extract text from Gemini response format
                return json["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| AiError::ParseError("No text in Gemini response".to_string()));
            }

            let status_code = status.as_u16();
            if should_retry(status_code, attempt) {
                let backoff = calculate_backoff(attempt);
                tokio::time::sleep(backoff).await;
                attempt += 1;
                continue;
            }

            let text = response.text().await.unwrap_or_default();
            return Err(AiError::RequestFailed(format!("HTTP {status}: {text}")));
        }
    }
}

/// Claude API provider.
#[derive(Debug, Clone)]
pub struct ClaudeProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl ClaudeProvider {
    /// Create a new Claude provider.
    #[must_use]
    pub fn new(base_url: String, api_key: String, model: String, max_tokens: u32) -> Self {
        Self {
            client: build_http_client(),
            base_url,
            api_key,
            model,
            max_tokens,
        }
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    async fn generate(&self, system: &str, user: &str) -> Result<String, AiError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [{
                "role": "user",
                "content": user
            }]
        });

        let mut attempt = 0;
        loop {
            let response = self
                .client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        AiError::Timeout
                    } else {
                        AiError::RequestFailed(e.to_string())
                    }
                })?;

            let status = response.status();
            if status.is_success() {
                let json: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| AiError::ParseError(e.to_string()))?;

                // Extract text from Claude response format
                return json["content"][0]["text"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| AiError::ParseError("No text in Claude response".to_string()));
            }

            let status_code = status.as_u16();
            if should_retry(status_code, attempt) {
                let backoff = calculate_backoff(attempt);
                tokio::time::sleep(backoff).await;
                attempt += 1;
                continue;
            }

            let text = response.text().await.unwrap_or_default();
            return Err(AiError::RequestFailed(format!("HTTP {status}: {text}")));
        }
    }
}

/// Provider enum for dispatch.
#[derive(Debug, Clone)]
pub enum Provider {
    Gemini(GeminiProvider),
    Claude(ClaudeProvider),
}

#[async_trait]
impl AiProvider for Provider {
    async fn generate(&self, system: &str, user: &str) -> Result<String, AiError> {
        match self {
            Self::Gemini(p) => p.generate(system, user).await,
            Self::Claude(p) => p.generate(system, user).await,
        }
    }
}

/// Client for making AI supervisor decisions.
#[derive(Debug, Clone)]
pub struct AiClient {
    provider: Provider,
    config: AiConfig,
}

impl AiClient {
    /// Create a new client with the given provider and config.
    #[must_use]
    pub fn new(provider: Provider, config: AiConfig) -> Self {
        Self { provider, config }
    }

    /// Create client from configuration.
    ///
    /// # Errors
    ///
    /// Returns `AiError::MissingApiKey` if the configured API key environment
    /// variable is not set.
    pub fn from_config(config: AiConfig) -> Result<Self, AiError> {
        let api_key = std::env::var(&config.api_key_env)
            .map_err(|_| AiError::MissingApiKey(config.api_key_env.clone()))?;

        let provider = match config.provider {
            ProviderKind::Gemini => Provider::Gemini(GeminiProvider::new(
                config.base_url.clone(),
                api_key,
                config.model.clone(),
                config.max_tokens,
            )),
            ProviderKind::Claude => Provider::Claude(ClaudeProvider::new(
                config.base_url.clone(),
                api_key,
                config.model.clone(),
                config.max_tokens,
            )),
        };

        Ok(Self { provider, config })
    }

    /// Create client from environment variables with default config.
    ///
    /// # Errors
    ///
    /// Returns `AiError::MissingApiKey` if the API key environment variable is not set.
    pub fn from_env() -> Result<Self, AiError> {
        Self::from_config(AiConfig::default())
    }

    /// Create client from environment variables with custom config.
    ///
    /// # Errors
    ///
    /// Returns `AiError::MissingApiKey` if the API key environment variable is not set.
    pub fn from_env_with_config(config: AiConfig) -> Result<Self, AiError> {
        Self::from_config(config)
    }

    /// Check if the client is configured.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        true // If we got here, we have an API key
    }

    /// Get the configured model.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Get the provider kind.
    #[must_use]
    pub fn provider_kind(&self) -> &ProviderKind {
        &self.config.provider
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
        let user_message = format!(
            "Context: {context}\n\nTool: {tool_name}\nInput: {}",
            serde_json::to_string_pretty(tool_input).unwrap_or_else(|_| tool_input.to_string())
        );

        let text = self
            .provider
            .generate(SUPERVISOR_SYSTEM_PROMPT, &user_message)
            .await?;

        extract_decision(&text)
    }
}

/// Extract a JSON object from AI response text.
///
/// Looks for JSON in the response and parses it into the specified type.
///
/// # Errors
///
/// Returns `AiError::ParseError` if no JSON object is found or parsing fails.
pub fn extract_json<T: DeserializeOwned>(text: &str) -> Result<T, AiError> {
    let json_start = text
        .find('{')
        .ok_or_else(|| AiError::ParseError(format!("No JSON object found in response: {text}")))?;

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
        .map_err(|e| AiError::ParseError(format!("Failed to parse JSON: {e}")))
}

/// Extract a `SupervisorDecision` from the AI response text.
///
/// Looks for JSON in the response and parses it.
fn extract_decision(text: &str) -> Result<SupervisorDecision, AiError> {
    extract_json(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_has_timeouts() {
        // The build_http_client function should create a client with timeouts configured
        let client = build_http_client();
        // If we get here without panic, the client was built successfully
        // We can't directly inspect timeout values, but we verify the builder works
        assert!(format!("{client:?}").contains("Client"));
    }

    #[test]
    fn test_gemini_provider_uses_configured_client() {
        let provider = GeminiProvider::new(
            "https://api.example.com".to_string(),
            "test-key".to_string(),
            "gemini-test".to_string(),
            1024,
        );
        // Verify the provider is created with the configured client
        assert_eq!(provider.model, "gemini-test");
        assert_eq!(provider.max_tokens, 1024);
    }

    #[test]
    fn test_claude_provider_uses_configured_client() {
        let provider = ClaudeProvider::new(
            "https://api.example.com".to_string(),
            "test-key".to_string(),
            "claude-test".to_string(),
            2048,
        );
        // Verify the provider is created with the configured client
        assert_eq!(provider.model, "claude-test");
        assert_eq!(provider.max_tokens, 2048);
    }

    #[tokio::test]
    async fn test_retry_on_server_error() {
        // Test that should_retry correctly identifies retryable errors
        assert!(should_retry(500, 0));
        assert!(should_retry(502, 1));
        assert!(should_retry(503, 2));
        assert!(!should_retry(500, MAX_RETRIES)); // Max retries reached
        assert!(!should_retry(400, 0)); // Client error, not retryable
        assert!(!should_retry(404, 0)); // Not found, not retryable
    }

    #[test]
    fn test_should_retry_logic() {
        // 5xx errors should be retried
        assert!(should_retry(500, 0));
        assert!(should_retry(502, 0));
        assert!(should_retry(503, 0));
        assert!(should_retry(504, 0));

        // 4xx errors should NOT be retried
        assert!(!should_retry(400, 0));
        assert!(!should_retry(401, 0));
        assert!(!should_retry(403, 0));
        assert!(!should_retry(404, 0));
        assert!(!should_retry(429, 0)); // Rate limit - could be retried but keeping simple

        // Success codes should NOT be retried
        assert!(!should_retry(200, 0));
        assert!(!should_retry(201, 0));

        // Max retries should stop retry
        assert!(!should_retry(500, MAX_RETRIES));
        assert!(!should_retry(503, MAX_RETRIES + 1));
    }

    #[test]
    fn test_calculate_backoff() {
        let backoff_0 = calculate_backoff(0);
        let backoff_1 = calculate_backoff(1);
        let backoff_2 = calculate_backoff(2);

        // Exponential backoff: 1s, 2s, 4s
        assert_eq!(backoff_0.as_secs(), 1);
        assert_eq!(backoff_1.as_secs(), 2);
        assert_eq!(backoff_2.as_secs(), 4);
    }

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
        let original = std::env::var("GEMINI_API_KEY").ok();
        std::env::remove_var("GEMINI_API_KEY");

        let result = AiClient::from_env();
        assert!(matches!(result, Err(AiError::MissingApiKey(_))));

        // Restore if it was set
        if let Some(key) = original {
            std::env::set_var("GEMINI_API_KEY", key);
        }
    }

    #[test]
    fn test_from_config_gemini() {
        std::env::set_var("TEST_GEMINI_KEY", "test-key");
        let config = AiConfig {
            provider: ProviderKind::Gemini,
            model: "gemini-3-flash".to_string(),
            max_tokens: 1024,
            base_url: "http://localhost:8045/v1beta".to_string(),
            api_key_env: "TEST_GEMINI_KEY".to_string(),
        };
        let client = AiClient::from_config(config).unwrap();
        assert!(matches!(client.provider, Provider::Gemini(_)));
        assert_eq!(client.model(), "gemini-3-flash");
        std::env::remove_var("TEST_GEMINI_KEY");
    }

    #[test]
    fn test_from_config_claude() {
        std::env::set_var("TEST_CLAUDE_KEY", "test-key");
        let config = AiConfig {
            provider: ProviderKind::Claude,
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 2048,
            base_url: "https://api.anthropic.com".to_string(),
            api_key_env: "TEST_CLAUDE_KEY".to_string(),
        };
        let client = AiClient::from_config(config).unwrap();
        assert!(matches!(client.provider, Provider::Claude(_)));
        assert_eq!(client.model(), "claude-sonnet-4-20250514");
        std::env::remove_var("TEST_CLAUDE_KEY");
    }
}
