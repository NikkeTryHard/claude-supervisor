//! Session state machine.

use serde::{Deserialize, Serialize};

/// Current state of a supervisor session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    #[default]
    Idle,
    Running,
    WaitingForApproval,
    WaitingForSupervisor,
    Paused,
    Completed,
    Failed,
}

/// State machine for tracking session progress.
#[derive(Debug, Clone)]
pub struct SessionStateMachine {
    state: SessionState,
    tool_calls: usize,
    approvals: usize,
    denials: usize,
}

impl Default for SessionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStateMachine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            tool_calls: 0,
            approvals: 0,
            denials: 0,
        }
    }

    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn transition(&mut self, new_state: SessionState) {
        tracing::debug!(from = ?self.state, to = ?new_state, "State transition");
        self.state = new_state;
    }

    pub fn record_tool_call(&mut self) {
        self.tool_calls = self.tool_calls.saturating_add(1);
    }

    pub fn record_approval(&mut self) {
        self.approvals = self.approvals.saturating_add(1);
    }

    pub fn record_denial(&mut self) {
        self.denials = self.denials.saturating_add(1);
    }

    #[must_use]
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            tool_calls: self.tool_calls,
            approvals: self.approvals,
            denials: self.denials,
        }
    }
}

/// Session statistics.
#[derive(Debug, Clone, Copy)]
pub struct SessionStats {
    pub tool_calls: usize,
    pub approvals: usize,
    pub denials: usize,
}
