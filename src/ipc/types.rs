//! IPC message types.
//!
//! This module defines the message types used for communication between
//! hook binaries and the supervisor process.

use serde::{Deserialize, Serialize};

/// Request from hook to supervisor for escalation.
///
/// When a hook binary encounters a tool call that requires supervisor
/// evaluation, it sends this request over the IPC socket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EscalationRequest {
    /// Session ID from Claude Code.
    pub session_id: String,
    /// Name of the tool being called.
    pub tool_name: String,
    /// Tool input parameters.
    pub tool_input: serde_json::Value,
    /// Reason for escalation.
    pub reason: String,
}

/// Response from supervisor to hook.
///
/// The supervisor evaluates the escalation request against its policies
/// and returns one of these decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum EscalationResponse {
    /// Allow the tool call to proceed unchanged.
    Allow,
    /// Deny the tool call with a reason.
    Deny {
        /// Explanation for why the tool call was denied.
        reason: String,
    },
    /// Allow the tool call with modified input parameters.
    Modify {
        /// The modified input parameters to use instead.
        updated_input: serde_json::Value,
    },
}

/// Request from Stop hook to supervisor for Q&A escalation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StopEscalationRequest {
    /// Session ID from Claude Code.
    pub session_id: String,
    /// The final message/summary from Claude before stopping.
    /// Note: This may be empty - the supervisor should read the transcript
    /// for the actual final message when `transcript_path` is provided.
    pub final_message: String,
    /// Path to the conversation transcript file.
    /// The supervisor uses this to read the full context and final message.
    pub transcript_path: Option<String>,
    /// The original task being worked on.
    pub task: Option<String>,
    /// Current iteration count for this session.
    pub iteration: u32,
}

/// Response from supervisor to Stop hook.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum StopEscalationResponse {
    /// Allow Claude to stop - task is complete.
    Allow,
    /// Block stop and provide continuation instructions.
    Continue {
        /// Reason/instructions for Claude to continue.
        reason: String,
    },
}

/// Errors that can occur during IPC.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// Failed to connect to the supervisor socket.
    #[error("Failed to connect to supervisor: {0}")]
    ConnectionFailed(#[from] std::io::Error),

    /// The supervisor socket does not exist.
    #[error("Supervisor not running (socket not found)")]
    SupervisorNotRunning,

    /// The operation timed out.
    #[error("IPC timeout after {0}ms")]
    Timeout(u64),

    /// Failed to serialize or deserialize a message.
    #[error("Failed to serialize message: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// The response from the supervisor was invalid.
    #[error("Invalid response from supervisor")]
    InvalidResponse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn escalation_request_serialization_roundtrip() {
        let request = EscalationRequest {
            session_id: "session-123".to_string(),
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "ls -la"}),
            reason: "Potentially dangerous command".to_string(),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: EscalationRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(request, deserialized);
    }

    #[test]
    fn escalation_response_allow_serialization() {
        let response = EscalationResponse::Allow;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, r#"{"decision":"allow"}"#);

        let deserialized: EscalationResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn escalation_response_deny_serialization() {
        let response = EscalationResponse::Deny {
            reason: "Command not allowed".to_string(),
        };
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains(r#""decision":"deny""#));
        assert!(serialized.contains(r#""reason":"Command not allowed""#));

        let deserialized: EscalationResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn escalation_response_modify_serialization() {
        let response = EscalationResponse::Modify {
            updated_input: json!({"command": "ls"}),
        };
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains(r#""decision":"modify""#));
        assert!(serialized.contains(r#""updated_input""#));

        let deserialized: EscalationResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn escalation_request_json_line_format() {
        let request = EscalationRequest {
            session_id: "abc".to_string(),
            tool_name: "Read".to_string(),
            tool_input: json!({"path": "/etc/passwd"}),
            reason: "Sensitive file access".to_string(),
        };

        // Verify it can be serialized to a single line (no embedded newlines)
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(!serialized.contains('\n'));
    }

    #[test]
    fn ipc_error_display() {
        let err = IpcError::SupervisorNotRunning;
        assert_eq!(err.to_string(), "Supervisor not running (socket not found)");

        let err = IpcError::Timeout(4000);
        assert_eq!(err.to_string(), "IPC timeout after 4000ms");
    }

    #[test]
    fn stop_escalation_request_serialization_roundtrip() {
        let request = StopEscalationRequest {
            session_id: "session-123".to_string(),
            final_message: "I've completed the task".to_string(),
            transcript_path: Some("/home/user/.claude/projects/abc/conversation.jsonl".to_string()),
            task: Some("Fix the auth bug".to_string()),
            iteration: 3,
        };
        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: StopEscalationRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(request, deserialized);
    }

    #[test]
    fn stop_escalation_response_allow_serialization() {
        let response = StopEscalationResponse::Allow;
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!(serialized, r#"{"decision":"allow"}"#);
        let deserialized: StopEscalationResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(response, deserialized);
    }

    #[test]
    fn stop_escalation_response_continue_serialization() {
        let response = StopEscalationResponse::Continue {
            reason: "Task incomplete, need to run tests".to_string(),
        };
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains(r#""decision":"continue""#));
        assert!(serialized.contains(r#""reason":"Task incomplete"#));
        let deserialized: StopEscalationResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(response, deserialized);
    }
}
