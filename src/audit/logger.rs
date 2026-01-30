//! Audit log implementation with async `SQLite` operations.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::error::AuditError;
use super::schema::SCHEMA;
use super::types::{AuditEvent, AuditSession, Decision, SessionMetrics};

/// Returns the default path for the audit database.
///
/// This is `~/.local/share/claude-supervisor/audit.db` on Unix systems.
#[must_use]
pub fn default_audit_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("claude-supervisor")
        .join("audit.db")
}

/// Audit log for recording supervisor decisions and events.
///
/// Uses `SQLite` for persistent storage with async operations via `spawn_blocking`.
#[derive(Debug, Clone)]
pub struct AuditLog {
    conn: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl AuditLog {
    /// Open an audit log at the specified path.
    ///
    /// Creates parent directories if they don't exist and initializes the schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or the schema cannot be applied.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, AuditError> {
        let path = path.as_ref().to_path_buf();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await.map_err(|source| {
                    AuditError::CreateDir {
                        path: parent.to_path_buf(),
                        source,
                    }
                })?;
            }
        }

        let path_clone = path.clone();
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection, AuditError> {
            let conn =
                Connection::open(&path_clone).map_err(|source| AuditError::DatabaseOpen {
                    path: path_clone,
                    source,
                })?;
            conn.execute_batch(SCHEMA)?;
            Ok(conn)
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(path),
        })
    }

    /// Open an in-memory audit log for testing.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be created or the schema cannot be applied.
    pub async fn open_in_memory() -> Result<Self, AuditError> {
        let conn = tokio::task::spawn_blocking(|| -> Result<Connection, AuditError> {
            let conn = Connection::open_in_memory()?;
            conn.execute_batch(SCHEMA)?;
            Ok(conn)
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        })
    }

    /// Returns the path to the database, if opened from a file.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Log a session start.
    ///
    /// # Errors
    ///
    /// Returns an error if the session cannot be inserted.
    pub async fn log_session_start(&self, session: &AuditSession) -> Result<(), AuditError> {
        let id = session.id.to_string();
        let started_at = session.started_at.to_rfc3339();
        let task = session.task.clone();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<(), AuditError> {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO sessions (id, started_at, task) VALUES (?1, ?2, ?3)",
                params![id, started_at, task],
            )?;
            Ok(())
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Log a session end with result.
    ///
    /// # Errors
    ///
    /// Returns an error if the session cannot be updated.
    pub async fn log_session_end(
        &self,
        session_id: Uuid,
        result: impl Into<String>,
    ) -> Result<(), AuditError> {
        let id = session_id.to_string();
        let ended_at = chrono::Utc::now().to_rfc3339();
        let result = result.into();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<(), AuditError> {
            let conn = conn.blocking_lock();
            conn.execute(
                "UPDATE sessions SET ended_at = ?1, result = ?2 WHERE id = ?3",
                params![ended_at, result, id],
            )?;
            Ok(())
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Log an audit event.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be inserted.
    pub async fn log_event(&self, event: &AuditEvent) -> Result<(), AuditError> {
        let id = event.id.to_string();
        let session_id = event.session_id.to_string();
        let timestamp = event.timestamp.to_rfc3339();
        let event_type = event.event_type.as_str().to_string();
        let tool_name = event.tool_name.clone();
        let tool_input = event
            .tool_input
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let decision = event.decision.map(|d| d.as_str().to_string());
        let reason = event.reason.clone();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<(), AuditError> {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO events (id, session_id, timestamp, event_type, tool_name, tool_input, decision, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, session_id, timestamp, event_type, tool_name, tool_input, decision, reason],
            )?;
            Ok(())
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Log or update session metrics.
    ///
    /// Uses INSERT OR REPLACE to upsert the metrics record.
    ///
    /// # Errors
    ///
    /// Returns an error if the metrics cannot be inserted.
    pub async fn log_metrics(&self, metrics: &SessionMetrics) -> Result<(), AuditError> {
        let session_id = metrics.session_id.to_string();
        let input_tokens = metrics.input_tokens;
        let output_tokens = metrics.output_tokens;
        let api_calls = metrics.api_calls;
        let cache_hits = metrics.cache_hits;
        let estimated_cost_cents = metrics.estimated_cost_cents;
        let updated_at = chrono::Utc::now().to_rfc3339();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<(), AuditError> {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO metrics (session_id, input_tokens, output_tokens, api_calls, cache_hits, estimated_cost_cents, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![session_id, input_tokens, output_tokens, api_calls, cache_hits, estimated_cost_cents, updated_at],
            )?;
            Ok(())
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Get events for a session, ordered by timestamp descending.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get_events(
        &self,
        session_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, AuditError> {
        let session_id_str = session_id.to_string();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<AuditEvent>, AuditError> {
            let conn = conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, session_id, timestamp, event_type, tool_name, tool_input, decision, reason
                 FROM events WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT ?2",
            )?;

            let events = stmt
                .query_map(params![session_id_str, i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
                    let id: String = row.get(0)?;
                    let session_id: String = row.get(1)?;
                    let timestamp: String = row.get(2)?;
                    let event_type: String = row.get(3)?;
                    let tool_name: Option<String> = row.get(4)?;
                    let tool_input: Option<String> = row.get(5)?;
                    let decision: Option<String> = row.get(6)?;
                    let reason: Option<String> = row.get(7)?;

                    Ok((
                        id,
                        session_id,
                        timestamp,
                        event_type,
                        tool_name,
                        tool_input,
                        decision,
                        reason,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut result = Vec::with_capacity(events.len());
            for (id, session_id, timestamp, event_type, tool_name, tool_input, decision, reason) in
                events
            {
                let id = Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil());
                let session_id = Uuid::parse_str(&session_id).unwrap_or_else(|_| Uuid::nil());
                let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp)
                    .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc));
                let event_type = match event_type.as_str() {
                    "session_start" => super::types::EventType::SessionStart,
                    "session_end" => super::types::EventType::SessionEnd,
                    "tool_use" => super::types::EventType::ToolUse,
                    "policy_decision" => super::types::EventType::PolicyDecision,
                    "ai_escalation" => super::types::EventType::AiEscalation,
                    _ => super::types::EventType::Error,
                };
                let tool_input = tool_input
                    .and_then(|s| serde_json::from_str(&s).ok());
                let decision = decision.and_then(|d| match d.as_str() {
                    "allow" => Some(Decision::Allow),
                    "deny" => Some(Decision::Deny),
                    "escalate" => Some(Decision::Escalate),
                    _ => None,
                });

                result.push(AuditEvent {
                    id,
                    session_id,
                    timestamp,
                    event_type,
                    tool_name,
                    tool_input,
                    decision,
                    reason,
                });
            }

            Ok(result)
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Get metrics for a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get_metrics(
        &self,
        session_id: Uuid,
    ) -> Result<Option<SessionMetrics>, AuditError> {
        let session_id_str = session_id.to_string();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<SessionMetrics>, AuditError> {
            let conn = conn.blocking_lock();
            let result = conn
                .query_row(
                    "SELECT session_id, input_tokens, output_tokens, api_calls, cache_hits, estimated_cost_cents
                     FROM metrics WHERE session_id = ?1",
                    params![session_id_str],
                    |row| {
                        let session_id: String = row.get(0)?;
                        let input_tokens: u64 = row.get(1)?;
                        let output_tokens: u64 = row.get(2)?;
                        let api_calls: u64 = row.get(3)?;
                        let cache_hits: u64 = row.get(4)?;
                        let estimated_cost_cents: u64 = row.get(5)?;
                        Ok((session_id, input_tokens, output_tokens, api_calls, cache_hits, estimated_cost_cents))
                    },
                )
                .optional()?;

            Ok(result.map(
                |(session_id, input_tokens, output_tokens, api_calls, cache_hits, estimated_cost_cents): (String, u64, u64, u64, u64, u64)| {
                    let session_id = Uuid::parse_str(&session_id).unwrap_or_else(|_| Uuid::nil());
                    SessionMetrics {
                        session_id,
                        input_tokens,
                        output_tokens,
                        api_calls,
                        cache_hits,
                        estimated_cost_cents,
                    }
                },
            ))
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Count total events in the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn count_events(&self) -> Result<u64, AuditError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<u64, AuditError> {
            let conn = conn.blocking_lock();
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
            Ok(count.unsigned_abs())
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }

    /// Count events by decision type.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn count_by_decision(&self, decision: Decision) -> Result<u64, AuditError> {
        let decision_str = decision.as_str().to_string();

        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<u64, AuditError> {
            let conn = conn.blocking_lock();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM events WHERE decision = ?1",
                params![decision_str],
                |row| row.get(0),
            )?;
            Ok(count.unsigned_abs())
        })
        .await
        .map_err(|_| AuditError::TaskCancelled)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::types::EventType;

    #[tokio::test]
    async fn test_open_in_memory() {
        let log = AuditLog::open_in_memory().await.unwrap();
        assert!(log.path().is_none());
    }

    #[tokio::test]
    async fn test_log_session_lifecycle() {
        let log = AuditLog::open_in_memory().await.unwrap();

        let session = AuditSession::new("Test task");
        let session_id = session.id;

        // Log session start
        log.log_session_start(&session).await.unwrap();

        // Log session end
        log.log_session_end(session_id, "Success").await.unwrap();

        // Verify by checking we can log events for this session
        let event = AuditEvent::builder(session_id, EventType::SessionEnd)
            .reason("Session completed")
            .build();
        log.log_event(&event).await.unwrap();

        let events = log.get_events(session_id, 10).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_log_event() {
        let log = AuditLog::open_in_memory().await.unwrap();

        let session = AuditSession::new("Test task");
        log.log_session_start(&session).await.unwrap();

        let event = AuditEvent::builder(session.id, EventType::ToolUse)
            .tool_name("Bash")
            .tool_input(serde_json::json!({"command": "ls -la"}))
            .decision(Decision::Allow)
            .reason("Command is safe")
            .build();

        log.log_event(&event).await.unwrap();

        let count = log.count_events().await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_get_events() {
        let log = AuditLog::open_in_memory().await.unwrap();

        let session = AuditSession::new("Test task");
        log.log_session_start(&session).await.unwrap();

        // Log multiple events
        for i in 0..5 {
            let event = AuditEvent::builder(session.id, EventType::ToolUse)
                .tool_name(format!("Tool{i}"))
                .decision(Decision::Allow)
                .build();
            log.log_event(&event).await.unwrap();
        }

        // Get with limit
        let events = log.get_events(session.id, 3).await.unwrap();
        assert_eq!(events.len(), 3);

        // Get all
        let events = log.get_events(session.id, 100).await.unwrap();
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn test_log_and_get_metrics() {
        let log = AuditLog::open_in_memory().await.unwrap();

        let session = AuditSession::new("Test task");
        log.log_session_start(&session).await.unwrap();

        let mut metrics = SessionMetrics::new(session.id);
        metrics.add_tokens(1000, 500);
        metrics.record_api_call();
        metrics.record_cache_hit();
        metrics.calculate_cost();

        log.log_metrics(&metrics).await.unwrap();

        let retrieved = log.get_metrics(session.id).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.input_tokens, 1000);
        assert_eq!(retrieved.output_tokens, 500);
        assert_eq!(retrieved.api_calls, 1);
        assert_eq!(retrieved.cache_hits, 1);

        // Update metrics
        metrics.add_tokens(500, 250);
        log.log_metrics(&metrics).await.unwrap();

        let updated = log.get_metrics(session.id).await.unwrap().unwrap();
        assert_eq!(updated.input_tokens, 1500);
        assert_eq!(updated.output_tokens, 750);
    }

    #[tokio::test]
    async fn test_count_by_decision() {
        let log = AuditLog::open_in_memory().await.unwrap();

        let session = AuditSession::new("Test task");
        log.log_session_start(&session).await.unwrap();

        // Log events with different decisions
        for _ in 0..3 {
            let event = AuditEvent::builder(session.id, EventType::PolicyDecision)
                .decision(Decision::Allow)
                .build();
            log.log_event(&event).await.unwrap();
        }

        for _ in 0..2 {
            let event = AuditEvent::builder(session.id, EventType::PolicyDecision)
                .decision(Decision::Deny)
                .build();
            log.log_event(&event).await.unwrap();
        }

        let event = AuditEvent::builder(session.id, EventType::AiEscalation)
            .decision(Decision::Escalate)
            .build();
        log.log_event(&event).await.unwrap();

        assert_eq!(log.count_by_decision(Decision::Allow).await.unwrap(), 3);
        assert_eq!(log.count_by_decision(Decision::Deny).await.unwrap(), 2);
        assert_eq!(log.count_by_decision(Decision::Escalate).await.unwrap(), 1);
    }

    #[test]
    fn test_default_audit_path() {
        let path = default_audit_path();
        assert!(path.ends_with("claude-supervisor/audit.db"));
    }

    #[tokio::test]
    async fn test_get_metrics_nonexistent() {
        let log = AuditLog::open_in_memory().await.unwrap();

        let result = log.get_metrics(Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_open_creates_parent_dirs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("nested").join("deep").join("audit.db");

        let log = AuditLog::open(&db_path).await.unwrap();
        assert_eq!(log.path(), Some(db_path.as_path()));

        // Verify file exists
        assert!(db_path.exists());
    }
}
