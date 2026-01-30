//! Stop hook handler.

use serde::{Deserialize, Serialize};

/// Decision for a Stop hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopDecision {
    Allow,
    Block,
}

/// Inner content of a Stop hook response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopOutput {
    pub hook_event_name: String,
    pub decision: StopDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response from a Stop hook wrapped in hookSpecificOutput.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopResponse {
    pub hook_specific_output: StopOutput,
}

impl StopResponse {
    #[must_use]
    pub fn allow() -> Self {
        Self {
            hook_specific_output: StopOutput {
                hook_event_name: "Stop".to_string(),
                decision: StopDecision::Allow,
                reason: None,
            },
        }
    }

    #[must_use]
    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            hook_specific_output: StopOutput {
                hook_event_name: "Stop".to_string(),
                decision: StopDecision::Block,
                reason: Some(reason.into()),
            },
        }
    }

    /// Get the stop decision.
    #[must_use]
    pub fn decision(&self) -> StopDecision {
        self.hook_specific_output.decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_response_format() {
        let response = StopResponse::allow();
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("hookSpecificOutput"));
        assert!(json.contains("\"decision\":\"allow\""));
        assert!(json.contains("\"hookEventName\":\"Stop\""));
    }

    #[test]
    fn test_block_response_format() {
        let response = StopResponse::block("Continue working");
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"decision\":\"block\""));
        assert!(json.contains("\"reason\":\"Continue working\""));
    }
}
