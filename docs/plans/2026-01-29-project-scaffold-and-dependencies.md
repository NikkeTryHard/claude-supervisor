# Project Scaffold and Dependencies Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Initialize Rust project structure with all module stubs and core dependencies (Issues #1, #2).

**Architecture:** Binary crate (`main.rs`) + library crate (`lib.rs`) with five modules: `cli` (process spawning/stream parsing), `hooks` (hook handlers), `supervisor` (policy engine), `ai` (Claude API client), `config` (configuration loading).

**Tech Stack:** Rust 2024 edition, tokio async runtime, clap CLI, serde JSON, tracing logging, thiserror errors.

---

### Batch 1: Cargo.toml and Core Structure

**Goal:** Set up Cargo.toml with metadata and dependencies, create lib.rs with module declarations.

#### Task 1.1: Update Cargo.toml with Metadata and Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Write failing test**

```bash
# No test file needed - we verify via cargo check
```

**Step 2: Verify current state fails**

Run: `cargo check 2>&1 | head -20`

Expected: Succeeds but has no dependencies (baseline).

**Step 3: Implement**

Replace `Cargo.toml` with:

```toml
[package]
name = "claude-supervisor"
version = "0.1.0"
edition = "2024"
authors = ["NikkeTryHard"]
description = "Automated Claude Code with AI oversight"
license = "MIT"
repository = "https://github.com/NikkeTryHard/claude-supervisor"
readme = "README.md"
keywords = ["claude", "supervisor", "ai", "automation"]
categories = ["command-line-utilities", "development-tools"]

[dependencies]
tokio = { version = "1", features = ["full", "process"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
tokio-test = "0.4"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = "warn"
pedantic = "warn"
```

**Step 4: Verify pass**

Run: `cargo check`

Expected: Compiles successfully (downloads dependencies).

**Step 5: Commit**

```bash
git add Cargo.toml
git commit -m "build: add core dependencies for phase 1"
```

---

#### Task 1.2: Create lib.rs with Module Declarations

**Files:**
- Create: `src/lib.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1`

Expected: FAIL - no library target exists yet.

**Step 2: Verify failure**

Run: `cargo check --lib 2>&1 | grep -i error`

Expected: Error about missing lib.rs or no library target.

**Step 3: Implement**

Create `src/lib.rs`:

```rust
//! Claude Supervisor - Automated Claude Code with AI oversight.
//!
//! This crate provides a supervisor layer that monitors Claude Code execution,
//! evaluates actions against configurable policies, and can intervene in real-time.

pub mod ai;
pub mod cli;
pub mod config;
pub mod hooks;
pub mod supervisor;
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1`

Expected: FAIL - modules don't exist yet (expected, will fix in next tasks).

**Step 5: Commit**

```bash
git add src/lib.rs
git commit -m "feat: add lib.rs with module declarations"
```

---

### Batch 2: CLI Module Stubs

**Goal:** Create cli module with stream parser and event type stubs.

#### Task 2.1: Create cli/mod.rs

**Files:**
- Create: `src/cli/mod.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "cli"`

Expected: Error about missing cli module.

**Step 2: Verify failure**

Confirmed by previous task - cli module missing.

**Step 3: Implement**

Create `src/cli/mod.rs`:

```rust
//! CLI module for Claude Code process spawning and stream parsing.

mod events;
mod stream;

pub use events::*;
pub use stream::*;
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "cli"`

Expected: No cli errors (but still missing submodules).

**Step 5: Commit**

```bash
git add src/cli/mod.rs
git commit -m "feat(cli): add cli module stub"
```

---

#### Task 2.2: Create cli/events.rs

**Files:**
- Create: `src/cli/events.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "events"`

Expected: Error about missing events module.

**Step 2: Verify failure**

Confirmed - events.rs missing.

**Step 3: Implement**

Create `src/cli/events.rs`:

```rust
//! Event types from Claude Code stream-json output.

use serde::{Deserialize, Serialize};

/// Events emitted by Claude Code in stream-json format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    /// System message or status update.
    System { message: String },

    /// Assistant text output.
    Assistant { text: String },

    /// Tool use request.
    ToolUse {
        tool_name: String,
        tool_input: serde_json::Value,
    },

    /// Tool result.
    ToolResult {
        tool_name: String,
        output: serde_json::Value,
    },

    /// Catch-all for unknown event types.
    #[serde(other)]
    Unknown,
}
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "events"`

Expected: No events errors.

**Step 5: Commit**

```bash
git add src/cli/events.rs
git commit -m "feat(cli): add event types for stream-json parsing"
```

---

#### Task 2.3: Create cli/stream.rs

**Files:**
- Create: `src/cli/stream.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "stream"`

Expected: Error about missing stream module.

**Step 2: Verify failure**

Confirmed - stream.rs missing.

**Step 3: Implement**

Create `src/cli/stream.rs`:

```rust
//! Stream parser for Claude Code stdout.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::cli::ClaudeEvent;

/// Error type for stream operations.
#[derive(thiserror::Error, Debug)]
pub enum StreamError {
    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),

    #[error("Failed to parse JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Process stdout not available")]
    NoStdout,
}

/// Spawn Claude Code in non-interactive mode.
pub fn spawn_claude(task: &str) -> Result<Child, StreamError> {
    let child = Command::new("claude")
        .args(["-p", task, "--output-format", "stream-json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    Ok(child)
}

/// Parse a single line of stream-json output.
pub fn parse_event(line: &str) -> Result<ClaudeEvent, StreamError> {
    let event: ClaudeEvent = serde_json::from_str(line)?;
    Ok(event)
}

/// Read events from Claude process stdout.
pub async fn read_events(
    child: &mut Child,
) -> Result<impl futures_core::Stream<Item = Result<ClaudeEvent, StreamError>> + '_, StreamError> {
    let stdout = child.stdout.take().ok_or(StreamError::NoStdout)?;
    let reader = BufReader::new(stdout).lines();

    Ok(futures_util::stream::unfold(reader, |mut reader| async {
        match reader.next_line().await {
            Ok(Some(line)) => {
                let event = parse_event(&line);
                Some((event, reader))
            }
            Ok(None) => None,
            Err(e) => Some((Err(StreamError::SpawnError(e)), reader)),
        }
    }))
}
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1`

Expected: Error about missing futures crates (we'll add them).

**Step 5: Add futures dependencies**

Add to `Cargo.toml` under `[dependencies]`:

```toml
futures-core = "0.3"
futures-util = "0.3"
```

Run: `cargo check --lib`

Expected: No stream errors.

**Step 6: Commit**

```bash
git add Cargo.toml src/cli/stream.rs
git commit -m "feat(cli): add stream parser for Claude process output"
```

---

### Batch 3: Hooks Module Stubs

**Goal:** Create hooks module with PreToolUse and Stop handlers.

#### Task 3.1: Create hooks/mod.rs

**Files:**
- Create: `src/hooks/mod.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "hooks"`

Expected: Error about missing hooks module.

**Step 2: Verify failure**

Confirmed - hooks module missing.

**Step 3: Implement**

Create `src/hooks/mod.rs`:

```rust
//! Hook handlers for Claude Code events.

mod pre_tool_use;
mod stop;

pub use pre_tool_use::*;
pub use stop::*;
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "hooks"`

Expected: No hooks errors (submodules still missing).

**Step 5: Commit**

```bash
git add src/hooks/mod.rs
git commit -m "feat(hooks): add hooks module stub"
```

---

#### Task 3.2: Create hooks/pre_tool_use.rs

**Files:**
- Create: `src/hooks/pre_tool_use.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "pre_tool_use"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/hooks/pre_tool_use.rs`:

```rust
//! PreToolUse hook handler.

use serde::{Deserialize, Serialize};

/// Decision for a PreToolUse hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    /// Allow the tool call to proceed.
    Allow,
    /// Deny the tool call.
    Deny,
    /// Ask the user for a decision.
    Ask,
}

/// Response from a PreToolUse hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseResponse {
    /// The permission decision.
    pub permission_decision: PermissionDecision,

    /// Reason for the decision (shown to Claude if denied).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Modified input parameters (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
}

impl PreToolUseResponse {
    /// Create an allow response.
    pub fn allow() -> Self {
        Self {
            permission_decision: PermissionDecision::Allow,
            reason: None,
            updated_input: None,
        }
    }

    /// Create a deny response with a reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            permission_decision: PermissionDecision::Deny,
            reason: Some(reason.into()),
            updated_input: None,
        }
    }

    /// Create an ask response.
    pub fn ask() -> Self {
        Self {
            permission_decision: PermissionDecision::Ask,
            reason: None,
            updated_input: None,
        }
    }
}
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "pre_tool_use"`

Expected: No errors.

**Step 5: Commit**

```bash
git add src/hooks/pre_tool_use.rs
git commit -m "feat(hooks): add PreToolUse response types"
```

---

#### Task 3.3: Create hooks/stop.rs

**Files:**
- Create: `src/hooks/stop.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "stop"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/hooks/stop.rs`:

```rust
//! Stop hook handler.

use serde::{Deserialize, Serialize};

/// Decision for a Stop hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopDecision {
    /// Allow Claude to stop.
    Allow,
    /// Block Claude from stopping (force continue).
    Block,
}

/// Response from a Stop hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopResponse {
    /// The stop decision.
    pub decision: StopDecision,

    /// Reason (required when blocking).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl StopResponse {
    /// Allow Claude to stop.
    pub fn allow() -> Self {
        Self {
            decision: StopDecision::Allow,
            reason: None,
        }
    }

    /// Block Claude from stopping with a reason.
    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            decision: StopDecision::Block,
            reason: Some(reason.into()),
        }
    }
}
```

**Step 4: Verify pass**

Run: `cargo check --lib`

Expected: No hooks errors.

**Step 5: Commit**

```bash
git add src/hooks/stop.rs
git commit -m "feat(hooks): add Stop response types"
```

---

### Batch 4: Supervisor Module Stubs

**Goal:** Create supervisor module with policy engine and state machine stubs.

#### Task 4.1: Create supervisor/mod.rs

**Files:**
- Create: `src/supervisor/mod.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "supervisor"`

Expected: Error about missing supervisor module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/supervisor/mod.rs`:

```rust
//! Supervisor module for policy enforcement and state management.

mod policy;
mod state;

pub use policy::*;
pub use state::*;
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "supervisor"`

Expected: No supervisor errors (submodules still missing).

**Step 5: Commit**

```bash
git add src/supervisor/mod.rs
git commit -m "feat(supervisor): add supervisor module stub"
```

---

#### Task 4.2: Create supervisor/policy.rs

**Files:**
- Create: `src/supervisor/policy.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "policy"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/supervisor/policy.rs`:

```rust
//! Policy engine for evaluating tool calls.

use serde::{Deserialize, Serialize};

/// Policy strictness level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyLevel {
    /// Allow most operations with minimal intervention.
    #[default]
    Permissive,

    /// Require approval for sensitive operations.
    Moderate,

    /// Require approval for all operations.
    Strict,
}

/// Policy decision for a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Allow the tool call.
    Allow,

    /// Deny the tool call with a reason.
    Deny(String),

    /// Escalate to AI supervisor for decision.
    Escalate(String),
}

/// Policy engine for evaluating actions.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    level: PolicyLevel,
    allowed_tools: Vec<String>,
    denied_tools: Vec<String>,
}

impl PolicyEngine {
    /// Create a new policy engine with the given level.
    pub fn new(level: PolicyLevel) -> Self {
        Self {
            level,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
        }
    }

    /// Evaluate a tool call against the policy.
    pub fn evaluate(&self, tool_name: &str, _tool_input: &serde_json::Value) -> PolicyDecision {
        // Check explicit deny list
        if self.denied_tools.iter().any(|t| t == tool_name) {
            return PolicyDecision::Deny(format!("Tool '{tool_name}' is explicitly denied"));
        }

        // Check explicit allow list
        if self.allowed_tools.iter().any(|t| t == tool_name) {
            return PolicyDecision::Allow;
        }

        // Apply policy level
        match self.level {
            PolicyLevel::Permissive => PolicyDecision::Allow,
            PolicyLevel::Moderate => PolicyDecision::Escalate(format!(
                "Tool '{tool_name}' requires supervisor approval"
            )),
            PolicyLevel::Strict => PolicyDecision::Escalate(format!(
                "Strict mode: Tool '{tool_name}' requires supervisor approval"
            )),
        }
    }

    /// Add a tool to the allow list.
    pub fn allow_tool(&mut self, tool: impl Into<String>) {
        self.allowed_tools.push(tool.into());
    }

    /// Add a tool to the deny list.
    pub fn deny_tool(&mut self, tool: impl Into<String>) {
        self.denied_tools.push(tool.into());
    }
}
```

**Step 4: Verify pass**

Run: `cargo check --lib`

Expected: No policy errors.

**Step 5: Commit**

```bash
git add src/supervisor/policy.rs
git commit -m "feat(supervisor): add policy engine"
```

---

#### Task 4.3: Create supervisor/state.rs

**Files:**
- Create: `src/supervisor/state.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "state"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/supervisor/state.rs`:

```rust
//! Session state machine.

use serde::{Deserialize, Serialize};

/// Session state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session not started.
    #[default]
    Idle,

    /// Claude is running.
    Running,

    /// Waiting for tool approval.
    WaitingForApproval,

    /// Waiting for AI supervisor decision.
    WaitingForSupervisor,

    /// Session paused by user.
    Paused,

    /// Session completed successfully.
    Completed,

    /// Session failed with error.
    Failed,
}

/// Session state machine.
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
    /// Create a new state machine in Idle state.
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            tool_calls: 0,
            approvals: 0,
            denials: 0,
        }
    }

    /// Get the current state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Transition to a new state.
    pub fn transition(&mut self, new_state: SessionState) {
        tracing::debug!(from = ?self.state, to = ?new_state, "State transition");
        self.state = new_state;
    }

    /// Record a tool call.
    pub fn record_tool_call(&mut self) {
        self.tool_calls += 1;
    }

    /// Record an approval.
    pub fn record_approval(&mut self) {
        self.approvals += 1;
    }

    /// Record a denial.
    pub fn record_denial(&mut self) {
        self.denials += 1;
    }

    /// Get session statistics.
    pub fn stats(&self) -> (usize, usize, usize) {
        (self.tool_calls, self.approvals, self.denials)
    }
}
```

**Step 4: Verify pass**

Run: `cargo check --lib`

Expected: No state errors.

**Step 5: Commit**

```bash
git add src/supervisor/state.rs
git commit -m "feat(supervisor): add session state machine"
```

---

### Batch 5: AI and Config Module Stubs

**Goal:** Create ai and config modules with client and configuration stubs.

#### Task 5.1: Create ai/mod.rs

**Files:**
- Create: `src/ai/mod.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "ai"`

Expected: Error about missing ai module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/ai/mod.rs`:

```rust
//! AI client module for supervisor decisions.

mod client;
mod prompts;

pub use client::*;
pub use prompts::*;
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "ai"`

Expected: No ai errors (submodules still missing).

**Step 5: Commit**

```bash
git add src/ai/mod.rs
git commit -m "feat(ai): add ai module stub"
```

---

#### Task 5.2: Create ai/client.rs

**Files:**
- Create: `src/ai/client.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "client"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/ai/client.rs`:

```rust
//! Claude API client wrapper for supervisor decisions.

use thiserror::Error;

/// Error type for AI client operations.
#[derive(Error, Debug)]
pub enum AiError {
    #[error("API key not configured")]
    MissingApiKey,

    #[error("API request failed: {0}")]
    RequestFailed(String),

    #[error("Failed to parse response: {0}")]
    ParseError(String),
}

/// AI client for supervisor decisions.
///
/// This is a stub that will be implemented with clust in Phase 3.
#[derive(Debug, Clone)]
pub struct AiClient {
    api_key: Option<String>,
}

impl Default for AiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AiClient {
    /// Create a new AI client.
    pub fn new() -> Self {
        Self { api_key: None }
    }

    /// Create a client from environment variable.
    pub fn from_env() -> Result<Self, AiError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        Ok(Self { api_key })
    }

    /// Check if the client is configured.
    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Ask the AI supervisor for a decision.
    ///
    /// Stub implementation - returns allow for now.
    pub async fn ask_supervisor(
        &self,
        _tool_name: &str,
        _tool_input: &serde_json::Value,
        _context: &str,
    ) -> Result<bool, AiError> {
        if !self.is_configured() {
            return Err(AiError::MissingApiKey);
        }

        // TODO: Implement with clust in Phase 3
        tracing::warn!("AI supervisor not implemented, defaulting to allow");
        Ok(true)
    }
}
```

**Step 4: Verify pass**

Run: `cargo check --lib`

Expected: No client errors.

**Step 5: Commit**

```bash
git add src/ai/client.rs
git commit -m "feat(ai): add AI client stub"
```

---

#### Task 5.3: Create ai/prompts.rs

**Files:**
- Create: `src/ai/prompts.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "prompts"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/ai/prompts.rs`:

```rust
//! System prompts for the AI supervisor.

/// System prompt for the AI supervisor.
pub const SUPERVISOR_SYSTEM_PROMPT: &str = r#"You are a security supervisor monitoring Claude Code execution.

Your role is to evaluate tool calls and decide whether they should be allowed.

When evaluating a tool call, consider:
1. Does this action align with the stated task?
2. Could this action cause unintended side effects?
3. Is this action within the expected scope?

Respond with:
- ALLOW: The action is safe and aligned with the task
- DENY: The action is risky or misaligned with the task
- REASON: Brief explanation of your decision
"#;

/// Format a tool call for supervisor review.
pub fn format_tool_review(tool_name: &str, tool_input: &serde_json::Value, task: &str) -> String {
    format!(
        r#"Task: {task}

Tool Call:
- Name: {tool_name}
- Input: {input}

Should this tool call be allowed?"#,
        input = serde_json::to_string_pretty(tool_input).unwrap_or_else(|_| tool_input.to_string())
    )
}
```

**Step 4: Verify pass**

Run: `cargo check --lib`

Expected: No prompts errors.

**Step 5: Commit**

```bash
git add src/ai/prompts.rs
git commit -m "feat(ai): add supervisor prompts"
```

---

#### Task 5.4: Create config/mod.rs

**Files:**
- Create: `src/config/mod.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "config"`

Expected: Error about missing config module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/config/mod.rs`:

```rust
//! Configuration module.

mod types;

pub use types::*;
```

**Step 4: Verify pass**

Run: `cargo check --lib 2>&1 | grep "config"`

Expected: No config errors (submodule still missing).

**Step 5: Commit**

```bash
git add src/config/mod.rs
git commit -m "feat(config): add config module stub"
```

---

#### Task 5.5: Create config/types.rs

**Files:**
- Create: `src/config/types.rs`

**Step 1: Write failing test**

Run: `cargo check --lib 2>&1 | grep "types"`

Expected: Error about missing module.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Create `src/config/types.rs`:

```rust
//! Configuration types.

use serde::{Deserialize, Serialize};

use crate::supervisor::PolicyLevel;

/// Supervisor configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    /// Policy level.
    #[serde(default)]
    pub policy: PolicyLevel,

    /// Auto-continue without user prompts.
    #[serde(default)]
    pub auto_continue: bool,

    /// Tools to always allow.
    #[serde(default)]
    pub allowed_tools: Vec<String>,

    /// Tools to always deny.
    #[serde(default)]
    pub denied_tools: Vec<String>,

    /// Enable AI supervisor for uncertain decisions.
    #[serde(default)]
    pub ai_supervisor: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            policy: PolicyLevel::Permissive,
            auto_continue: false,
            allowed_tools: vec![
                "Read".to_string(),
                "Glob".to_string(),
                "Grep".to_string(),
            ],
            denied_tools: Vec::new(),
            ai_supervisor: false,
        }
    }
}
```

**Step 4: Verify pass**

Run: `cargo check --lib`

Expected: Library compiles successfully.

**Step 5: Commit**

```bash
git add src/config/types.rs
git commit -m "feat(config): add configuration types"
```

---

### Batch 6: Main Entry Point and CLI

**Goal:** Update main.rs with clap CLI and tracing initialization.

#### Task 6.1: Update main.rs with CLI

**Files:**
- Modify: `src/main.rs`

**Step 1: Write failing test**

Run: `cargo run -- --help 2>&1`

Expected: Just prints "Hello, world!" - no CLI yet.

**Step 2: Verify failure**

Confirmed.

**Step 3: Implement**

Replace `src/main.rs` with:

```rust
//! Claude Supervisor - Automated Claude Code with AI oversight.

use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use claude_supervisor::config::SupervisorConfig;
use claude_supervisor::supervisor::PolicyLevel;

#[derive(Parser)]
#[command(
    name = "claude-supervisor",
    about = "Automated Claude Code with AI oversight",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run Claude Code with supervision.
    Run {
        /// The task to execute.
        task: String,

        /// Policy level (permissive, moderate, strict).
        #[arg(short, long, default_value = "permissive")]
        policy: String,

        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
    },
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

fn parse_policy(s: &str) -> PolicyLevel {
    match s.to_lowercase().as_str() {
        "strict" => PolicyLevel::Strict,
        "moderate" => PolicyLevel::Moderate,
        _ => PolicyLevel::Permissive,
    }
}

#[tokio::main]
async fn main() {
    init_tracing();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            task,
            policy,
            auto_continue,
        } => {
            let config = SupervisorConfig {
                policy: parse_policy(&policy),
                auto_continue,
                ..Default::default()
            };

            tracing::info!(
                task = %task,
                policy = ?config.policy,
                auto_continue = config.auto_continue,
                "Starting Claude supervisor"
            );

            // TODO: Implement supervisor loop in Phase 2
            tracing::warn!("Supervisor not yet implemented");
        }
    }
}
```

**Step 4: Verify pass**

Run: `cargo run -- --help`

Expected: Shows CLI help with "run" subcommand.

Run: `cargo run -- run "test task" --policy strict`

Expected: Logs "Starting Claude supervisor" with task and policy.

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: add CLI entry point with clap"
```

---

### Batch 7: Final Verification

**Goal:** Verify everything compiles and passes clippy.

#### Task 7.1: Run Full Build and Clippy

**Files:**
- None (verification only)

**Step 1: Build**

Run: `cargo build`

Expected: Compiles successfully.

**Step 2: Clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: No warnings or errors.

**Step 3: Test run**

Run: `cargo run -- run "Hello world" --policy permissive`

Expected: Logs supervisor start message.

**Step 4: Commit all remaining**

```bash
git add -A
git commit -m "chore: complete phase 1 project scaffold"
```

---

## Summary

| Batch | Tasks | Description |
|-------|-------|-------------|
| 1 | 2 | Cargo.toml + lib.rs |
| 2 | 3 | CLI module (events, stream) |
| 3 | 3 | Hooks module (pre_tool_use, stop) |
| 4 | 3 | Supervisor module (policy, state) |
| 5 | 5 | AI + Config modules |
| 6 | 1 | Main entry point |
| 7 | 1 | Final verification |

**Total:** 18 tasks across 7 batches.
