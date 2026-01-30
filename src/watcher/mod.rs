//! Watcher module for Claude Code conversation files.
//!
//! Provides JSONL parsing for session history files.

mod discovery;
mod error;
mod jsonl;
mod pattern;
mod reconstructor;
mod session_watcher;
mod subagent;
mod tailer;

pub use discovery::{
    discover_session, discover_subagent_files, extract_agent_id, find_latest_session,
    find_project_sessions_dir, find_session_by_id, find_subagents_dir, project_path_hash,
};
pub use error::WatcherError;
pub use jsonl::*;
pub use pattern::{PatternDetector, PatternThresholds, StuckPattern};
pub use reconstructor::{SessionReconstructor, ToolCallRecord};
pub use session_watcher::{SessionWatcher, WatcherEvent};
pub use subagent::{SubagentRecord, SubagentStatus, SubagentTracker, DEFAULT_MAX_SUBAGENTS};
pub use tailer::JsonlTailer;
