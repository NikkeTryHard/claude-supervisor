//! Multi-session supervisor for parallel Claude Code execution.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::supervisor::{PolicyEngine, SessionStats, SupervisorError, SupervisorResult};

/// Error type for multi-session operations.
#[derive(thiserror::Error, Debug)]
pub enum MultiSessionError {
    /// Maximum concurrent sessions reached.
    #[error("Maximum sessions reached: {limit}")]
    MaxSessionsReached { limit: usize },

    /// Session not found.
    #[error("Session not found: {id}")]
    SessionNotFound { id: String },

    /// Supervisor error during session execution.
    #[error("Supervisor error: {0}")]
    SupervisorError(#[from] SupervisorError),

    /// Task join error.
    #[error("Task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

/// Metadata for a running session.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    /// Unique session identifier.
    pub id: String,
    /// Task description.
    pub task: String,
    /// When the session started.
    pub started_at: Instant,
    /// Cancellation token for stopping the session.
    cancel: CancellationToken,
}

impl SessionMeta {
    /// Create new session metadata.
    #[must_use]
    pub fn new(id: String, task: String) -> Self {
        Self {
            id,
            task,
            started_at: Instant::now(),
            cancel: CancellationToken::new(),
        }
    }

    /// Get a clone of the cancellation token.
    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Cancel this session.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Check if this session is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// Result of a completed session.
#[derive(Debug)]
pub struct SessionResult {
    /// Session identifier.
    pub id: String,
    /// Task that was executed.
    pub task: String,
    /// Result of the session.
    pub result: Result<SupervisorResult, SupervisorError>,
    /// Session statistics.
    pub stats: SessionStats,
}

/// Aggregated statistics across all sessions.
#[derive(Debug, Clone, Default)]
pub struct AggregatedStats {
    /// Total sessions completed.
    pub sessions_completed: usize,
    /// Total sessions failed.
    pub sessions_failed: usize,
    /// Total tool calls across all sessions.
    pub total_tool_calls: usize,
    /// Total approvals across all sessions.
    pub total_approvals: usize,
    /// Total denials across all sessions.
    pub total_denials: usize,
}

impl AggregatedStats {
    /// Add stats from a completed session.
    pub fn add(&mut self, stats: &SessionStats, success: bool) {
        if success {
            self.sessions_completed += 1;
        } else {
            self.sessions_failed += 1;
        }
        self.total_tool_calls += stats.tool_calls;
        self.total_approvals += stats.approvals;
        self.total_denials += stats.denials;
    }
}

/// Supervisor for managing multiple parallel Claude Code sessions.
pub struct MultiSessionSupervisor {
    /// Active session metadata.
    sessions: HashMap<String, SessionMeta>,
    /// Join set for tracking spawned tasks.
    join_set: JoinSet<SessionResult>,
    /// Semaphore for limiting concurrent sessions.
    #[allow(dead_code)] // Used in future batches for spawn limiting
    semaphore: Arc<Semaphore>,
    /// Shared policy engine.
    policy: Arc<PolicyEngine>,
    /// Maximum concurrent sessions.
    max_sessions: usize,
    /// Aggregated statistics.
    stats: AggregatedStats,
}

impl MultiSessionSupervisor {
    /// Create a new multi-session supervisor.
    #[must_use]
    pub fn new(max_sessions: usize, policy: PolicyEngine) -> Self {
        Self {
            sessions: HashMap::new(),
            join_set: JoinSet::new(),
            semaphore: Arc::new(Semaphore::new(max_sessions)),
            policy: Arc::new(policy),
            max_sessions,
            stats: AggregatedStats::default(),
        }
    }

    /// Get the maximum number of concurrent sessions.
    #[must_use]
    pub fn max_sessions(&self) -> usize {
        self.max_sessions
    }

    /// Get the number of currently active sessions.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get metadata for all active sessions.
    #[must_use]
    pub fn active_sessions(&self) -> Vec<&SessionMeta> {
        self.sessions.values().collect()
    }

    /// Get metadata for a specific session.
    #[must_use]
    pub fn get_session(&self, id: &str) -> Option<&SessionMeta> {
        self.sessions.get(id)
    }

    /// Get the shared policy engine.
    #[must_use]
    pub fn policy(&self) -> Arc<PolicyEngine> {
        Arc::clone(&self.policy)
    }

    /// Get the aggregated statistics.
    #[must_use]
    pub fn stats(&self) -> &AggregatedStats {
        &self.stats
    }

    /// Check if there are any active or pending sessions.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.join_set.is_empty()
    }
}
