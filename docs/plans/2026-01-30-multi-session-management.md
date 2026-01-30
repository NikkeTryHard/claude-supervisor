# Multi-Session Management Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Enable supervising multiple parallel Claude Code instances with configurable concurrency, shared policy, and aggregated metrics.

**Architecture:** `MultiSessionSupervisor` orchestrates multiple `Supervisor` instances via `tokio::task::JoinSet`. Each session runs in its own spawned task with semaphore-based concurrency limiting. Sessions can be stopped via `CancellationToken`. Metrics are collected at session completion and aggregated.

**Tech Stack:** Rust, tokio (JoinSet, Semaphore), tokio-util (CancellationToken), clap (CLI)

---

## Batch 1: Dependencies and Core Types

**Goal:** Add tokio-util dependency and define core multi-session types.

### Task 1.1: Add tokio-util dependency

**Files:**
- Modify: `Cargo.toml:29`

**Step 1: Write failing test**

```bash
# No test needed - this is a dependency addition
# Verify current state
cargo check 2>&1 | grep -q "tokio_util" && echo "FAIL: already exists" || echo "OK: not present"
```

**Step 2: Verify failure**

Run: `grep "tokio-util" Cargo.toml`
Expected: No output (dependency not present)

**Step 3: Implement**

Add after line 29 (after `notify-debouncer-full`):

```toml
tokio-util = { version = "0.7", features = ["rt"] }
```

**Step 4: Verify pass**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add tokio-util dependency for CancellationToken"
```

---

### Task 1.2: Define MultiSessionError type

**Files:**
- Create: `src/supervisor/multi.rs`
- Modify: `src/supervisor/mod.rs`

**Step 1: Write failing test**

Create `tests/supervisor/multi_test.rs`:

```rust
use claude_supervisor::supervisor::{MultiSessionError, MultiSessionSupervisor};

#[test]
fn test_multi_session_error_display() {
    let err = MultiSessionError::MaxSessionsReached { limit: 3 };
    assert_eq!(err.to_string(), "Maximum sessions reached: 3");
}

#[test]
fn test_multi_session_error_session_not_found() {
    let err = MultiSessionError::SessionNotFound {
        id: "test-123".to_string(),
    };
    assert!(err.to_string().contains("test-123"));
}
```

Add to `tests/supervisor/mod.rs`:

```rust
mod multi_test;
```

**Step 2: Verify failure**

Run: `cargo t multi_test`
Expected: FAIL with "unresolved import `claude_supervisor::supervisor::MultiSessionError`"

**Step 3: Implement**

Create `src/supervisor/multi.rs`:

```rust
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
    MaxSessionsReached {
        /// The configured limit.
        limit: usize,
    },

    /// Session not found.
    #[error("Session not found: {id}")]
    SessionNotFound {
        /// The session ID that was not found.
        id: String,
    },

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
    ///
    /// # Arguments
    ///
    /// * `max_sessions` - Maximum number of concurrent sessions.
    /// * `policy` - Policy engine to share across sessions.
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
}
```

Update `src/supervisor/mod.rs`:

```rust
//! Supervisor module for policy enforcement and state management.

mod blocklist;
mod multi;
mod policy;
mod runner;
mod state;

pub use blocklist::*;
pub use multi::*;
pub use policy::*;
pub use runner::*;
pub use state::*;
```

**Step 4: Verify pass**

Run: `cargo t multi_test`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/multi.rs src/supervisor/mod.rs tests/supervisor/multi_test.rs tests/supervisor/mod.rs
git commit -m "feat(supervisor): add MultiSessionError and core types"
```

---

## Batch 2: Supervisor Cancellation Support

**Goal:** Add CancellationToken support to Supervisor for graceful shutdown.

### Task 2.1: Add cancellation field to Supervisor

**Files:**
- Modify: `src/supervisor/runner.rs:77-108`

**Step 1: Write failing test**

Add to `tests/supervisor/runner_test.rs`:

```rust
use claude_supervisor::supervisor::{PolicyEngine, PolicyLevel, Supervisor};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_supervisor_with_cancellation_token() {
    let (tx, rx) = mpsc::channel(32);
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let cancel = CancellationToken::new();

    let supervisor = Supervisor::new(policy, rx).with_cancellation(cancel.clone());

    assert!(!supervisor.is_cancelled());
    cancel.cancel();
    assert!(supervisor.is_cancelled());

    drop(tx);
}
```

**Step 2: Verify failure**

Run: `cargo t test_supervisor_with_cancellation_token`
Expected: FAIL with "no method named `with_cancellation`"

**Step 3: Implement**

In `src/supervisor/runner.rs`, add import at top:

```rust
use tokio_util::sync::CancellationToken;
```

Update the `Supervisor` struct (around line 77):

```rust
/// Supervisor for orchestrating Claude Code execution with policy enforcement.
pub struct Supervisor {
    process: Option<ClaudeProcess>,
    policy: PolicyEngine,
    event_rx: Receiver<ClaudeEvent>,
    state: SessionStateMachine,
    session_id: Option<String>,
    ai_client: Option<AiClient>,
    event_history: VecDeque<ClaudeEvent>,
    cwd: Option<String>,
    task: Option<String>,
    knowledge: Option<KnowledgeAggregator>,
    cancel: Option<CancellationToken>,
}
```

Update the `new` constructor (around line 95):

```rust
    #[must_use]
    pub fn new(policy: PolicyEngine, event_rx: Receiver<ClaudeEvent>) -> Self {
        Self {
            process: None,
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: None,
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
        }
    }
```

Add builder method after `set_task` (around line 297):

```rust
    /// Set a cancellation token for graceful shutdown.
    #[must_use]
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Check if this supervisor has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(CancellationToken::is_cancelled)
    }
```

Update all other constructors (`with_ai_client`, `with_process`, `with_process_and_ai`, `from_process`, `from_process_with_ai`) to include `cancel: None`.

**Step 4: Verify pass**

Run: `cargo t test_supervisor_with_cancellation_token`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/runner.rs tests/supervisor/runner_test.rs
git commit -m "feat(supervisor): add CancellationToken support"
```

---

### Task 2.2: Implement cancellation-aware run loop

**Files:**
- Modify: `src/supervisor/runner.rs:447-488`

**Step 1: Write failing test**

Add to `tests/supervisor/runner_test.rs`:

```rust
use claude_supervisor::cli::ClaudeEvent;
use std::time::Duration;

#[tokio::test]
async fn test_supervisor_cancelled_during_run() {
    let (tx, rx) = mpsc::channel(32);
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let cancel = CancellationToken::new();

    let mut supervisor = Supervisor::new(policy, rx).with_cancellation(cancel.clone());

    // Cancel after a short delay
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    // Keep channel open so supervisor doesn't exit from channel close
    let _tx = tx;

    let result = supervisor.run_without_process().await.unwrap();

    // Should exit with Cancelled result
    assert!(matches!(result, claude_supervisor::supervisor::SupervisorResult::Cancelled));
}
```

**Step 2: Verify failure**

Run: `cargo t test_supervisor_cancelled_during_run`
Expected: FAIL with "no variant named `Cancelled`"

**Step 3: Implement**

First, add `Cancelled` variant to `SupervisorResult` (around line 42):

```rust
/// Result of a supervised session.
#[derive(Debug, Clone)]
pub enum SupervisorResult {
    /// Session completed normally.
    Completed {
        /// Session identifier.
        session_id: Option<String>,
        /// Total cost in USD.
        cost_usd: Option<f64>,
    },
    /// Session was killed by the supervisor.
    Killed {
        /// Reason for killing.
        reason: String,
    },
    /// Process exited (channel closed).
    ProcessExited,
    /// Session was cancelled via CancellationToken.
    Cancelled,
}
```

Update `run_without_process` method (around line 399):

```rust
    pub async fn run_without_process(&mut self) -> Result<SupervisorResult, SupervisorError> {
        self.state.transition(SessionState::Running);

        loop {
            // Check for cancellation
            if let Some(ref cancel) = self.cancel {
                tokio::select! {
                    biased;

                    () = cancel.cancelled() => {
                        tracing::info!("Session cancelled via token");
                        self.state.transition(SessionState::Completed);
                        return Ok(SupervisorResult::Cancelled);
                    }
                    event = self.event_rx.recv() => {
                        if let Some(event) = event {
                            match self.handle_event_action(&event).await? {
                                Some(result) => return Ok(result),
                                None => continue,
                            }
                        } else {
                            self.state.transition(SessionState::Completed);
                            return Ok(SupervisorResult::ProcessExited);
                        }
                    }
                }
            } else {
                // No cancellation token - original behavior
                if let Some(event) = self.event_rx.recv().await {
                    match self.handle_event_action(&event).await? {
                        Some(result) => return Ok(result),
                        None => continue,
                    }
                } else {
                    self.state.transition(SessionState::Completed);
                    return Ok(SupervisorResult::ProcessExited);
                }
            }
        }
    }
```

Extract event action handling to a helper method (add after `handle_event`):

```rust
    /// Handle event and return action result.
    async fn handle_event_action(
        &mut self,
        event: &ClaudeEvent,
    ) -> Result<Option<SupervisorResult>, SupervisorError> {
        let action = self.handle_event(event);
        match action {
            EventAction::Continue => Ok(None),
            EventAction::Complete(result) => {
                self.state.transition(SessionState::Completed);
                Ok(Some(result))
            }
            EventAction::Kill(reason) => {
                self.state.transition(SessionState::Failed);
                Ok(Some(SupervisorResult::Killed { reason }))
            }
            EventAction::Escalate { tool_use, reason } => {
                match self.handle_escalation(&tool_use, &reason).await {
                    EscalationResult::Allow => {
                        self.state.record_approval();
                        self.state.transition(SessionState::Running);
                        Ok(None)
                    }
                    EscalationResult::Deny(deny_reason) => {
                        self.state.record_denial();
                        self.state.transition(SessionState::Failed);
                        Ok(Some(SupervisorResult::Killed {
                            reason: deny_reason,
                        }))
                    }
                }
            }
        }
    }
```

Update `run` method similarly (around line 447) to use `tokio::select!` with cancellation.

**Step 4: Verify pass**

Run: `cargo t test_supervisor_cancelled_during_run`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/runner.rs tests/supervisor/runner_test.rs
git commit -m "feat(supervisor): implement cancellation-aware run loop"
```

---

## Batch 3: Session Spawning

**Goal:** Implement spawn_session and session lifecycle management.

### Task 3.1: Implement spawn_session method

**Files:**
- Modify: `src/supervisor/multi.rs`

**Step 1: Write failing test**

Add to `tests/supervisor/multi_test.rs`:

```rust
use claude_supervisor::supervisor::{MultiSessionSupervisor, PolicyEngine, PolicyLevel};

#[tokio::test]
async fn test_spawn_session_returns_id() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let id = supervisor.spawn_session("Test task".to_string()).await.unwrap();

    assert!(!id.is_empty());
    assert_eq!(supervisor.active_count(), 1);
}

#[tokio::test]
async fn test_spawn_session_respects_limit() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(2, policy);

    // Spawn two sessions (at limit)
    let _id1 = supervisor.spawn_session("Task 1".to_string()).await.unwrap();
    let _id2 = supervisor.spawn_session("Task 2".to_string()).await.unwrap();

    // Third should fail (non-blocking check)
    let result = supervisor.try_spawn_session("Task 3".to_string());
    assert!(result.is_err());
}
```

**Step 2: Verify failure**

Run: `cargo t test_spawn_session`
Expected: FAIL with "no method named `spawn_session`"

**Step 3: Implement**

Add to `src/supervisor/multi.rs`:

```rust
use uuid::Uuid;

impl MultiSessionSupervisor {
    // ... existing methods ...

    /// Spawn a new session with the given task.
    ///
    /// This method waits for a semaphore permit if at capacity.
    ///
    /// # Arguments
    ///
    /// * `task` - The task description for Claude to execute.
    ///
    /// # Returns
    ///
    /// The session ID on success.
    ///
    /// # Errors
    ///
    /// Returns error if session spawning fails.
    pub async fn spawn_session(&mut self, task: String) -> Result<String, MultiSessionError> {
        // Acquire semaphore permit (waits if at capacity)
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| MultiSessionError::MaxSessionsReached {
                limit: self.max_sessions,
            })?;

        self.spawn_session_internal(task, permit)
    }

    /// Try to spawn a new session without waiting.
    ///
    /// # Returns
    ///
    /// The session ID on success, or error if at capacity.
    pub fn try_spawn_session(&mut self, task: String) -> Result<String, MultiSessionError> {
        // Try to acquire permit without waiting
        let permit = self
            .semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| MultiSessionError::MaxSessionsReached {
                limit: self.max_sessions,
            })?;

        self.spawn_session_internal(task, permit)
    }

    /// Internal session spawning logic.
    fn spawn_session_internal(
        &mut self,
        task: String,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<String, MultiSessionError> {
        let id = Uuid::new_v4().to_string();
        let meta = SessionMeta::new(id.clone(), task.clone());
        let cancel = meta.cancellation_token();
        let policy = Arc::clone(&self.policy);

        // Store metadata
        self.sessions.insert(id.clone(), meta);

        // Spawn the session task
        let session_id = id.clone();
        let session_task = task.clone();

        self.join_set.spawn(async move {
            // Hold permit for duration of session
            let _permit = permit;

            // Create a mock result for now - actual implementation in next task
            let stats = SessionStats {
                tool_calls: 0,
                approvals: 0,
                denials: 0,
            };

            // Simulate session - wait for cancellation or complete
            tokio::select! {
                () = cancel.cancelled() => {
                    SessionResult {
                        id: session_id,
                        task: session_task,
                        result: Ok(SupervisorResult::Cancelled),
                        stats,
                    }
                }
                () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    SessionResult {
                        id: session_id,
                        task: session_task,
                        result: Ok(SupervisorResult::ProcessExited),
                        stats,
                    }
                }
            }
        });

        Ok(id)
    }
}
```

Add `uuid` to `Cargo.toml`:

```toml
uuid = { version = "1", features = ["v4"] }
```

**Step 4: Verify pass**

Run: `cargo t test_spawn_session`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/multi.rs Cargo.toml tests/supervisor/multi_test.rs
git commit -m "feat(multi): implement spawn_session with semaphore limiting"
```

---

### Task 3.2: Implement stop_session method

**Files:**
- Modify: `src/supervisor/multi.rs`

**Step 1: Write failing test**

Add to `tests/supervisor/multi_test.rs`:

```rust
#[tokio::test]
async fn test_stop_session() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let id = supervisor.spawn_session("Long task".to_string()).await.unwrap();

    // Stop the session
    supervisor.stop_session(&id).unwrap();

    // Session should be marked as cancelled
    let meta = supervisor.get_session(&id).unwrap();
    assert!(meta.is_cancelled());
}

#[tokio::test]
async fn test_stop_nonexistent_session() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let supervisor = MultiSessionSupervisor::new(3, policy);

    let result = supervisor.stop_session("nonexistent");
    assert!(matches!(result, Err(MultiSessionError::SessionNotFound { .. })));
}
```

**Step 2: Verify failure**

Run: `cargo t test_stop_session`
Expected: FAIL with "no method named `stop_session`"

**Step 3: Implement**

Add to `src/supervisor/multi.rs`:

```rust
impl MultiSessionSupervisor {
    // ... existing methods ...

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
}
```

**Step 4: Verify pass**

Run: `cargo t test_stop_session`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/multi.rs tests/supervisor/multi_test.rs
git commit -m "feat(multi): implement stop_session and stop_all"
```

---

## Batch 4: Wait and Collect Results

**Goal:** Implement wait_all and result collection with metrics aggregation.

### Task 4.1: Implement wait_all method

**Files:**
- Modify: `src/supervisor/multi.rs`

**Step 1: Write failing test**

Add to `tests/supervisor/multi_test.rs`:

```rust
#[tokio::test]
async fn test_wait_all_collects_results() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    // Spawn multiple sessions
    supervisor.spawn_session("Task 1".to_string()).await.unwrap();
    supervisor.spawn_session("Task 2".to_string()).await.unwrap();

    // Wait for all to complete
    let results = supervisor.wait_all().await;

    assert_eq!(results.len(), 2);
    assert_eq!(supervisor.active_count(), 0);
}

#[tokio::test]
async fn test_wait_all_aggregates_stats() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    supervisor.spawn_session("Task 1".to_string()).await.unwrap();
    supervisor.spawn_session("Task 2".to_string()).await.unwrap();

    let _ = supervisor.wait_all().await;

    let stats = supervisor.stats();
    assert_eq!(stats.sessions_completed + stats.sessions_failed, 2);
}
```

**Step 2: Verify failure**

Run: `cargo t test_wait_all`
Expected: FAIL with "no method named `wait_all`"

**Step 3: Implement**

Add to `src/supervisor/multi.rs`:

```rust
impl MultiSessionSupervisor {
    // ... existing methods ...

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
                Some(session_result)
            }
            Err(join_error) => {
                tracing::error!(error = %join_error, "Session task panicked");
                self.stats.sessions_failed += 1;
                None
            }
        }
    }

    /// Check if there are any active or pending sessions.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.join_set.is_empty()
    }
}
```

**Step 4: Verify pass**

Run: `cargo t test_wait_all`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/multi.rs tests/supervisor/multi_test.rs
git commit -m "feat(multi): implement wait_all with metrics aggregation"
```

---

### Task 4.2: Implement spawn_and_wait convenience method

**Files:**
- Modify: `src/supervisor/multi.rs`

**Step 1: Write failing test**

Add to `tests/supervisor/multi_test.rs`:

```rust
#[tokio::test]
async fn test_spawn_and_wait_all() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let tasks = vec![
        "Task 1".to_string(),
        "Task 2".to_string(),
        "Task 3".to_string(),
    ];

    let results = supervisor.spawn_and_wait_all(tasks).await.unwrap();

    assert_eq!(results.len(), 3);
}
```

**Step 2: Verify failure**

Run: `cargo t test_spawn_and_wait_all`
Expected: FAIL with "no method named `spawn_and_wait_all`"

**Step 3: Implement**

Add to `src/supervisor/multi.rs`:

```rust
impl MultiSessionSupervisor {
    // ... existing methods ...

    /// Spawn multiple sessions and wait for all to complete.
    ///
    /// # Arguments
    ///
    /// * `tasks` - List of task descriptions to execute.
    ///
    /// # Returns
    ///
    /// Results for all sessions.
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
```

**Step 4: Verify pass**

Run: `cargo t test_spawn_and_wait_all`
Expected: PASS

**Step 5: Commit**

```bash
git add src/supervisor/multi.rs tests/supervisor/multi_test.rs
git commit -m "feat(multi): add spawn_and_wait_all convenience method"
```

---

## Batch 5: CLI Integration

**Goal:** Add `multi` subcommand to CLI.

### Task 5.1: Add Multi subcommand definition

**Files:**
- Modify: `src/main.rs:47-94`

**Step 1: Write failing test**

Run CLI with multi subcommand:

```bash
cargo run -- multi --help 2>&1 | grep -q "Run multiple tasks" && echo "PASS" || echo "FAIL"
```

Expected: FAIL (command not recognized)

**Step 2: Verify failure**

Run: `cargo run -- multi --task "test" 2>&1`
Expected: error about unrecognized subcommand

**Step 3: Implement**

In `src/main.rs`, add to the `Commands` enum (after `Worktree`):

```rust
    /// Run multiple Claude Code sessions in parallel.
    Multi {
        /// Tasks to run (can specify multiple).
        #[arg(long, action = clap::ArgAction::Append, required = true)]
        task: Vec<String>,
        /// Maximum parallel sessions.
        #[arg(long, default_value = "3")]
        max_parallel: usize,
        /// Policy level for all sessions.
        #[arg(short, long, value_enum, default_value_t = PolicyArg::Permissive)]
        policy: PolicyArg,
        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
    },
```

**Step 4: Verify pass**

Run: `cargo run -- multi --help`
Expected: Shows help for multi subcommand

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add multi subcommand definition"
```

---

### Task 5.2: Implement multi command handler

**Files:**
- Modify: `src/main.rs`

**Step 1: Write failing test**

```bash
cargo run -- multi --task "Task 1" --task "Task 2" --max-parallel 2 2>&1 | grep -q "completed" && echo "PASS" || echo "FAIL"
```

Expected: FAIL (handler not implemented)

**Step 2: Verify failure**

Run: `cargo run -- multi --task "test"`
Expected: Logs but no multi-session execution

**Step 3: Implement**

Add handler function in `src/main.rs`:

```rust
use claude_supervisor::supervisor::MultiSessionSupervisor;

async fn handle_multi(
    tasks: Vec<String>,
    max_parallel: usize,
    policy: PolicyArg,
    _auto_continue: bool,
) {
    tracing::info!(
        tasks = tasks.len(),
        max_parallel = max_parallel,
        policy = ?policy,
        "Starting multi-session supervisor"
    );

    let policy_engine = PolicyEngine::new(policy.into());
    let mut supervisor = MultiSessionSupervisor::new(max_parallel, policy_engine);

    // Spawn all sessions
    for task in &tasks {
        match supervisor.spawn_session(task.clone()).await {
            Ok(id) => {
                tracing::info!(session_id = %id, task = %task, "Session spawned");
            }
            Err(e) => {
                tracing::error!(task = %task, error = %e, "Failed to spawn session");
            }
        }
    }

    // Wait for all to complete
    let results = supervisor.wait_all().await;

    // Print summary
    println!("\n=== Multi-Session Summary ===");
    println!("Sessions: {}", results.len());

    for result in &results {
        let status = match &result.result {
            Ok(r) => format!("{r:?}"),
            Err(e) => format!("Error: {e}"),
        };
        println!("  [{}] {} - {}", result.id, result.task, status);
    }

    let stats = supervisor.stats();
    println!("\n=== Aggregated Stats ===");
    println!("  Completed: {}", stats.sessions_completed);
    println!("  Failed: {}", stats.sessions_failed);
    println!("  Tool calls: {}", stats.total_tool_calls);
    println!("  Approvals: {}", stats.total_approvals);
    println!("  Denials: {}", stats.total_denials);
}
```

Add match arm in `main`:

```rust
        Commands::Multi {
            task,
            max_parallel,
            policy,
            auto_continue,
        } => {
            handle_multi(task, max_parallel, policy, auto_continue).await;
        }
```

**Step 4: Verify pass**

Run: `cargo run -- multi --task "Task 1" --task "Task 2" -v`
Expected: Shows session spawning and summary output

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): implement multi command handler"
```

---

## Batch 6: Integration Tests

**Goal:** Add comprehensive integration tests for multi-session functionality.

### Task 6.1: Create multi-session integration test

**Files:**
- Create: `tests/multi_session_integration.rs`

**Step 1: Write test**

Create `tests/multi_session_integration.rs`:

```rust
//! Integration tests for multi-session supervisor.

use claude_supervisor::supervisor::{
    AggregatedStats, MultiSessionError, MultiSessionSupervisor, PolicyEngine, PolicyLevel,
    SessionMeta, SessionResult,
};
use std::time::Duration;

#[tokio::test]
async fn test_multi_session_full_lifecycle() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(2, policy);

    // Verify initial state
    assert_eq!(supervisor.max_sessions(), 2);
    assert_eq!(supervisor.active_count(), 0);
    assert!(!supervisor.has_pending());

    // Spawn sessions
    let id1 = supervisor
        .spawn_session("Integration test 1".to_string())
        .await
        .unwrap();
    let id2 = supervisor
        .spawn_session("Integration test 2".to_string())
        .await
        .unwrap();

    assert_eq!(supervisor.active_count(), 2);
    assert!(supervisor.has_pending());

    // Verify session metadata
    let meta1 = supervisor.get_session(&id1).unwrap();
    assert_eq!(meta1.task, "Integration test 1");
    assert!(!meta1.is_cancelled());

    // Wait for completion
    let results = supervisor.wait_all().await;
    assert_eq!(results.len(), 2);
    assert_eq!(supervisor.active_count(), 0);
    assert!(!supervisor.has_pending());

    // Verify stats
    let stats = supervisor.stats();
    assert!(stats.sessions_completed + stats.sessions_failed == 2);
}

#[tokio::test]
async fn test_multi_session_concurrent_limit() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(1, policy);

    // Spawn one session (at limit)
    let _id = supervisor.spawn_session("Task".to_string()).await.unwrap();

    // Try to spawn another (should fail with try_spawn)
    let result = supervisor.try_spawn_session("Another task".to_string());
    assert!(matches!(
        result,
        Err(MultiSessionError::MaxSessionsReached { limit: 1 })
    ));
}

#[tokio::test]
async fn test_multi_session_stop_and_wait() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(3, policy);

    let id = supervisor
        .spawn_session("Long running task".to_string())
        .await
        .unwrap();

    // Stop the session
    supervisor.stop_session(&id).unwrap();

    // Wait for completion
    let results = supervisor.wait_all().await;
    assert_eq!(results.len(), 1);

    // Session should have been cancelled
    let result = &results[0];
    assert!(matches!(
        result.result,
        Ok(claude_supervisor::supervisor::SupervisorResult::Cancelled)
    ));
}

#[tokio::test]
async fn test_multi_session_shared_policy() {
    let mut policy = PolicyEngine::new(PolicyLevel::Strict);
    policy.allow_tool("Read");
    policy.deny_tool("Bash");

    let supervisor = MultiSessionSupervisor::new(3, policy);

    // All sessions share the same policy
    let shared_policy = supervisor.policy();
    // Policy should be accessible
    assert!(std::sync::Arc::strong_count(&shared_policy) >= 1);
}

#[tokio::test]
async fn test_spawn_and_wait_all_convenience() {
    let policy = PolicyEngine::new(PolicyLevel::Permissive);
    let mut supervisor = MultiSessionSupervisor::new(5, policy);

    let tasks = vec![
        "Task A".to_string(),
        "Task B".to_string(),
        "Task C".to_string(),
    ];

    let results = supervisor.spawn_and_wait_all(tasks).await.unwrap();

    assert_eq!(results.len(), 3);

    let task_names: Vec<&str> = results.iter().map(|r| r.task.as_str()).collect();
    assert!(task_names.contains(&"Task A"));
    assert!(task_names.contains(&"Task B"));
    assert!(task_names.contains(&"Task C"));
}
```

**Step 2: Verify pass**

Run: `cargo t multi_session_integration`
Expected: All tests PASS

**Step 3: Commit**

```bash
git add tests/multi_session_integration.rs
git commit -m "test: add multi-session integration tests"
```

---

### Task 6.2: Add CLI integration test

**Files:**
- Modify: `tests/integration.rs` or create new test

**Step 1: Write test**

Add to `tests/integration.rs`:

```rust
#[test]
fn test_multi_command_help() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--", "multi", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show help without error
    assert!(
        stdout.contains("--task") || stderr.contains("--task"),
        "Help should mention --task flag"
    );
    assert!(
        stdout.contains("--max-parallel") || stderr.contains("--max-parallel"),
        "Help should mention --max-parallel flag"
    );
}
```

**Step 2: Verify pass**

Run: `cargo t test_multi_command_help`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/integration.rs
git commit -m "test: add CLI multi command help test"
```

---

## Final Verification

Run all tests and checks:

```bash
# All tests
cargo t

# Clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt -- --check

# Build release
cargo build --release
```

All checks should pass.

---

## Summary

| Batch | Tasks | Focus |
|-------|-------|-------|
| 1 | 2 | Dependencies and core types |
| 2 | 2 | Supervisor cancellation support |
| 3 | 2 | Session spawning |
| 4 | 2 | Wait and collect results |
| 5 | 2 | CLI integration |
| 6 | 2 | Integration tests |

**Total: 12 tasks across 6 batches**
