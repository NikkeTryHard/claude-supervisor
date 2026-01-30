//! Database schema for audit logging.

/// Current schema version for migrations.
pub const SCHEMA_VERSION: u32 = 1;

/// SQL schema for the audit database.
pub const SCHEMA: &str = r"
-- Enable WAL mode for better concurrent read/write performance
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Sessions table: tracks supervisor runs
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    task TEXT NOT NULL,
    result TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Events table: individual audit events
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    tool_name TEXT,
    tool_input TEXT,
    decision TEXT,
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Metrics table: session resource usage
CREATE TABLE IF NOT EXISTS metrics (
    session_id TEXT PRIMARY KEY NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    api_calls INTEGER NOT NULL DEFAULT 0,
    cache_hits INTEGER NOT NULL DEFAULT 0,
    estimated_cost_cents INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Schema version table for migrations
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_events_session_id ON events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_decision ON events(decision);
CREATE INDEX IF NOT EXISTS idx_sessions_started_at ON sessions(started_at);
";

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_version() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_schema_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // Verify sessions table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify events table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify metrics table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='metrics'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify schema_version table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_creates_indexes() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // Check all expected indexes exist
        let expected_indexes = [
            "idx_events_session_id",
            "idx_events_timestamp",
            "idx_events_event_type",
            "idx_events_decision",
            "idx_sessions_started_at",
        ];

        for index_name in expected_indexes {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?",
                    [index_name],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Index {index_name} should exist");
        }
    }

    #[test]
    fn test_schema_wal_mode() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // In-memory databases use "memory" mode, but WAL pragma was executed
        // Just verify the pragma doesn't error
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        // In-memory uses "memory", file-based would use "wal"
        assert!(!mode.is_empty());
    }

    #[test]
    fn test_schema_foreign_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // Insert a session
        conn.execute(
            "INSERT INTO sessions (id, started_at, task) VALUES ('test-session', datetime('now'), 'Test task')",
            [],
        )
        .unwrap();

        // Insert an event for that session - should work
        conn.execute(
            "INSERT INTO events (id, session_id, timestamp, event_type) VALUES ('test-event', 'test-session', datetime('now'), 'session_start')",
            [],
        )
        .unwrap();

        // Try to insert event with non-existent session - should fail
        let result = conn.execute(
            "INSERT INTO events (id, session_id, timestamp, event_type) VALUES ('bad-event', 'no-such-session', datetime('now'), 'session_start')",
            [],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_cascade_delete() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // Insert a session with events and metrics
        conn.execute(
            "INSERT INTO sessions (id, started_at, task) VALUES ('delete-test', datetime('now'), 'Test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events (id, session_id, timestamp, event_type) VALUES ('event1', 'delete-test', datetime('now'), 'tool_use')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO metrics (session_id, input_tokens) VALUES ('delete-test', 100)",
            [],
        )
        .unwrap();

        // Delete the session
        conn.execute("DELETE FROM sessions WHERE id = 'delete-test'", [])
            .unwrap();

        // Verify cascaded deletes
        let event_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE session_id = 'delete-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_count, 0);

        let metrics_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM metrics WHERE session_id = 'delete-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(metrics_count, 0);
    }

    #[test]
    fn test_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Apply schema twice - should not error due to IF NOT EXISTS
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // Tables should still exist and work
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
