//! Claude API client wrapper for supervisor decisions.

use thiserror::Error;

/// Errors from AI client operations.
#[derive(Error, Debug)]
pub enum AiError {
    #[error("API key not configured")]
    MissingApiKey,
    #[error("API request failed: {0}")]
    RequestFailed(String),
    #[error("Failed to parse response: {0}")]
    ParseError(String),
}

/// Client for making AI supervisor decisions.
#[derive(Debug, Clone)]
pub struct AiClient {
    api_key: Option<String>,
}

impl Default for AiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AiClient {
    #[must_use]
    pub fn new() -> Self {
        Self { api_key: None }
    }

    /// Create client from environment variables.
    ///
    /// # Errors
    ///
    /// Currently infallible but returns Result for future compatibility.
    pub fn from_env() -> Result<Self, AiError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        Ok(Self { api_key })
    }

    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Ask the AI supervisor whether to allow a tool call.
    ///
    /// # Errors
    ///
    /// Returns `AiError::MissingApiKey` if the API key is not configured.
    #[allow(clippy::unused_async)]
    pub async fn ask_supervisor(
        &self,
        _tool_name: &str,
        _tool_input: &serde_json::Value,
        _context: &str,
    ) -> Result<bool, AiError> {
        if !self.is_configured() {
            return Err(AiError::MissingApiKey);
        }
        tracing::warn!("AI supervisor not implemented, defaulting to allow");
        Ok(true)
    }
}
