//! Stop hook handler.

use serde::{Deserialize, Serialize};

/// Decision for a Stop hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopDecision {
    Allow,
    Block,
}

/// Response from a Stop hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopResponse {
    pub decision: StopDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl StopResponse {
    #[must_use]
    pub fn allow() -> Self {
        Self {
            decision: StopDecision::Allow,
            reason: None,
        }
    }

    #[must_use]
    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            decision: StopDecision::Block,
            reason: Some(reason.into()),
        }
    }
}
