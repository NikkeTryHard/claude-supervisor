//! Multi-session supervisor for parallel Claude Code execution.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

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

    /// Spawn a new session with the given task.
    ///
    /// This method waits for a semaphore permit if at capacity.
    ///
    /// # Errors
    ///
    /// Returns error if session spawning fails.
    pub async fn spawn_session(&mut self, task: String) -> Result<String, MultiSessionError> {
        // Acquire semaphore permit (waits if at capacity)
        let permit = self.semaphore.clone().acquire_owned().await.map_err(|_| {
            MultiSessionError::MaxSessionsReached {
                limit: self.max_sessions,
            }
        })?;

        Ok(self.spawn_session_internal(&task, permit))
    }

    /// Try to spawn a new session without waiting.
    ///
    /// # Returns
    ///
    /// The session ID on success, or error if at capacity.
    ///
    /// # Errors
    ///
    /// Returns `MaxSessionsReached` if already at capacity.
    pub fn try_spawn_session(&mut self, task: &str) -> Result<String, MultiSessionError> {
        // Try to acquire permit without waiting
        let permit = self.semaphore.clone().try_acquire_owned().map_err(|_| {
            MultiSessionError::MaxSessionsReached {
                limit: self.max_sessions,
            }
        })?;

        Ok(self.spawn_session_internal(task, permit))
    }

    /// Internal session spawning logic.
    fn spawn_session_internal(
        &mut self,
        task: &str,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let meta = SessionMeta::new(id.clone(), task.to_string());
        let cancel = meta.cancellation_token();

        // Store metadata
        self.sessions.insert(id.clone(), meta);

        // Spawn the session task
        let session_id = id.clone();
        let session_task = task.to_string();

        self.join_set.spawn(async move {
            // Hold permit for duration of session
            let _permit = permit;

            let stats = SessionStats {
                tool_calls: 0,
                approvals: 0,
                denials: 0,
            };

            // Wait for cancellation or simulate completion
            tokio::select! {
                () = cancel.cancelled() => {
                    SessionResult {
                        id: session_id,
                        task: session_task,
                        result: Ok(SupervisorResult::Cancelled),
                        stats,
                    }
                }
                () = tokio::time::sleep(Duration::from_millis(100)) => {
                    SessionResult {
                        id: session_id,
                        task: session_task,
                        result: Ok(SupervisorResult::ProcessExited),
                        stats,
                    }
                }
            }
        });

        tracing::info!(session_id = %id, task = %task, "Session spawned");
        id
    }

    /// Stop a running session by ID.
    ///
    /// # Errors
    ///
    /// Returns `SessionNotFound` if no session with the given ID exists.
    pub fn stop_session(&self, id: &str) -> Result<(), MultiSessionError> {
        let meta = self
            .sessions
            .get(id)
            .ok_or_else(|| MultiSessionError::SessionNotFound { id: id.to_string() })?;

        meta.cancel();
        tracing::info!(session_id = %id, "Session stop requested");
        Ok(())
    }

    /// Stop all running sessions.
    pub fn stop_all(&self) {
        for (id, meta) in &self.sessions {
            meta.cancel();
            tracing::info!(session_id = %id, "Session stop requested");
        }
    }

    /// Wait for all sessions to complete and collect results.
    ///
    /// This consumes all pending session results and updates aggregated stats.
    pub async fn wait_all(&mut self) -> Vec<SessionResult> {
        let mut results = Vec::new();

        while let Some(join_result) = self.join_set.join_next().await {
            match join_result {
                Ok(session_result) => {
                    // Remove from active sessions
                    self.sessions.remove(&session_result.id);

                    // Update aggregated stats
                    let success = session_result.result.is_ok();
                    self.stats.add(&session_result.stats, success);

                    tracing::info!(
                        session_id = %session_result.id,
                        task = %session_result.task,
                        success = success,
                        "Session completed"
                    );

                    results.push(session_result);
                }
                Err(join_error) => {
                    tracing::error!(error = %join_error, "Session task panicked");
                    self.stats.sessions_failed += 1;
                }
            }
        }

        results
    }

    /// Wait for the next session to complete.
    ///
    /// Returns `None` if no sessions are running.
    pub async fn wait_next(&mut self) -> Option<SessionResult> {
        let join_result = self.join_set.join_next().await?;

        match join_result {
            Ok(session_result) => {
                self.sessions.remove(&session_result.id);
                let success = session_result.result.is_ok();
                self.stats.add(&session_result.stats, success);

                tracing::info!(
                    session_id = %session_result.id,
                    task = %session_result.task,
                    success = success,
                    "Session completed"
                );

                Some(session_result)
            }
            Err(join_error) => {
                tracing::error!(error = %join_error, "Session task panicked");
                self.stats.sessions_failed += 1;
                None
            }
        }
    }

    /// Spawn multiple sessions and wait for all to complete.
    ///
    /// # Arguments
    ///
    /// * `tasks` - List of task descriptions to execute.
    ///
    /// # Returns
    ///
    /// Results for all sessions.
    ///
    /// # Errors
    ///
    /// Returns error if session spawning fails.
    pub async fn spawn_and_wait_all(
        &mut self,
        tasks: Vec<String>,
    ) -> Result<Vec<SessionResult>, MultiSessionError> {
        // Spawn all tasks
        for task in tasks {
            self.spawn_session(task).await?;
        }

        // Wait for all to complete
        Ok(self.wait_all().await)
    }
}
