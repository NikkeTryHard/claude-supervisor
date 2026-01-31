# Implement Run Command - Issue #53

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Wire the existing `Supervisor` orchestration to the `run` command stub in `main.rs`.

**Architecture:** The `run` command spawns a `ClaudeProcess`, wraps it with a `Supervisor` that parses stream-json events, evaluates tool calls against `PolicyEngine`, and optionally escalates to an AI supervisor. All components exist — this plan wires them together.

**Tech Stack:** Rust, tokio, clap

---

## Batch 1: Add Imports and Error Handling

**Goal:** Prepare main.rs with necessary imports and a helper function for run command errors.

### Task 1.1: Add Missing Imports

**Files:**
- Modify: `src/main.rs:1-30`

**Step 1: Write failing test**
```rust
// No test needed — this is import scaffolding
// Verification: cargo check
```

**Step 2: Verify failure**
Run: `cargo check 2>&1 | head -20`
Expected: Current code compiles (baseline check)

**Step 3: Implement**
Add these imports near the top of `src/main.rs` (after existing imports):

```rust
use claude_supervisor::cli::{ClaudeProcess, ClaudeProcessBuilder};
use claude_supervisor::supervisor::{Supervisor, SupervisorError, SupervisorResult};
use claude_supervisor::ai::AiClient;
```

**Step 4: Verify pass**
Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**
```bash
git add src/main.rs
git commit -m "feat(run): add imports for run command implementation"
```

---

### Task 1.2: Create handle_run Function Signature

**Files:**
- Modify: `src/main.rs`

**Step 1: Write failing test**
```rust
// No test — function stub
```

**Step 2: Verify failure**
Run: `cargo check`
Expected: Compiles (baseline)

**Step 3: Implement**
Add this function before the `main()` function:

```rust
/// Handle the run command - spawn and supervise Claude Code.
async fn handle_run(
    task: Option<String>,
    resume: Option<String>,
    config: SupervisorConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get prompt (task or "continue" for resume)
    let prompt = task.unwrap_or_else(|| "continue".to_string());

    // Build process
    let mut builder = ClaudeProcessBuilder::new(&prompt);

    // Add resume if provided
    if let Some(session_id) = &resume {
        builder = builder.resume(session_id);
    }

    // Add allowed tools if configured
    if !config.allowed_tools.is_empty() {
        let tools: Vec<&str> = config.allowed_tools.iter().map(String::as_str).collect();
        builder = builder.allowed_tools(&tools);
    }

    tracing::info!("Spawning Claude Code process");
    let process = ClaudeProcess::spawn(&builder)?;

    // Build policy engine
    let mut policy = PolicyEngine::new(config.policy.clone());
    for tool in &config.allowed_tools {
        policy.allow_tool(tool);
    }

    // Create supervisor (with or without AI)
    let mut supervisor = if config.ai_supervisor {
        tracing::info!("AI supervision enabled");
        let ai_client = AiClient::from_env()?;
        Supervisor::from_process_with_ai(process, policy, ai_client)?
    } else {
        Supervisor::from_process(process, policy)?
    };

    // Set task context
    supervisor.set_task(&prompt);

    // Initialize knowledge from current directory
    let cwd = std::env::current_dir()?;
    supervisor.init_knowledge(&cwd).await;

    // Run supervision loop
    tracing::info!("Starting supervision loop");
    let result = supervisor.run().await?;

    // Report result
    match result {
        SupervisorResult::Completed { session_id, cost_usd } => {
            tracing::info!(
                session_id = ?session_id,
                cost_usd = ?cost_usd,
                "Session completed successfully"
            );
        }
        SupervisorResult::Killed { reason } => {
            tracing::warn!(reason = %reason, "Session killed by supervisor");
        }
        SupervisorResult::ProcessExited => {
            tracing::info!("Claude process exited");
        }
        SupervisorResult::Cancelled => {
            tracing::info!("Session cancelled");
        }
    }

    Ok(())
}
```

**Step 4: Verify pass**
Run: `cargo check`
Expected: Compiles (function defined but not yet called)

**Step 5: Commit**
```bash
git add src/main.rs
git commit -m "feat(run): add handle_run function skeleton"
```

---

## Batch 2: Wire Run Command and Handle Errors

**Goal:** Replace the stub with a call to `handle_run` and add proper error handling.

### Task 2.1: Replace Stub with Function Call

**Files:**
- Modify: `src/main.rs:597` (the stub location)

**Step 1: Write failing test**
```rust
// Integration test - manual verification
```

**Step 2: Verify failure**
Run: `cargo run -- run "echo test" 2>&1 | grep -i "not yet implemented"`
Expected: Shows "Supervisor not yet implemented" warning

**Step 3: Implement**
Replace line 597 (`tracing::warn!("Supervisor not yet implemented");`) with:

```rust
            if let Err(e) = handle_run(task, resume, config).await {
                tracing::error!(error = %e, "Supervisor failed");
                std::process::exit(1);
            }
```

**Step 4: Verify pass**
Run: `cargo check`
Expected: Compiles successfully

Run: `cargo run -- run "echo test" 2>&1 | grep -i "not yet implemented"`
Expected: No output (warning removed)

**Step 5: Commit**
```bash
git add src/main.rs
git commit -m "feat(run): wire handle_run to run command"
```

---

### Task 2.2: Add Integration Test for Run Command

**Files:**
- Create: `tests/run_command.rs`

**Step 1: Write failing test**
```rust
//! Integration tests for the run command.

use std::process::Command;

#[test]
fn test_run_command_requires_task_or_resume() {
    let output = Command::new("cargo")
        .args(["run", "--", "run"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("either <TASK> or --resume") || !output.status.success(),
        "Expected error when neither task nor resume provided"
    );
}

#[test]
fn test_run_command_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "run", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--policy"), "Expected --policy in help");
    assert!(stdout.contains("--auto-continue"), "Expected --auto-continue in help");
}
```

**Step 2: Verify failure**
Run: `cargo nextest run test_run_command`
Expected: FAIL (test file doesn't exist)

**Step 3: Implement**
Create the file with the test code above.

**Step 4: Verify pass**
Run: `cargo nextest run test_run_command`
Expected: PASS (both tests pass)

**Step 5: Commit**
```bash
git add tests/run_command.rs
git commit -m "test(run): add integration tests for run command"
```

---

## Batch 3: Add Worktree Support (Optional Feature)

**Goal:** Wire the worktree configuration to run Claude in an isolated git worktree.

### Task 3.1: Add Worktree Integration to handle_run

**Files:**
- Modify: `src/main.rs` (handle_run function)

**Step 1: Write failing test**
```rust
// Manual verification with worktree flag
```

**Step 2: Verify failure**
Run: `cargo run -- run "echo test" --worktree 2>&1`
Expected: Currently ignores worktree flag (no worktree created)

**Step 3: Implement**
Update `handle_run` to accept and use worktree config. Add this at the start of the function:

```rust
async fn handle_run(
    task: Option<String>,
    resume: Option<String>,
    config: SupervisorConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use claude_supervisor::worktree::WorktreeManager;

    // Handle worktree isolation if enabled
    let (working_dir, _worktree_guard) = if config.worktree.enabled {
        tracing::info!("Creating isolated worktree for task");
        let manager = WorktreeManager::new(std::env::current_dir()?)?;
        let worktree = manager.create_for_task(
            task.as_deref().unwrap_or("supervised-task")
        ).await?;
        let path = worktree.path().to_path_buf();
        tracing::info!(path = %path.display(), "Running in worktree");
        (Some(path), Some(worktree))
    } else {
        (None, None)
    };

    // Get prompt (task or "continue" for resume)
    let prompt = task.unwrap_or_else(|| "continue".to_string());

    // Build process
    let mut builder = ClaudeProcessBuilder::new(&prompt);

    // Set working directory if using worktree
    if let Some(ref dir) = working_dir {
        builder = builder.working_dir(dir);
    }

    // ... rest of existing code
```

**Step 4: Verify pass**
Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**
```bash
git add src/main.rs
git commit -m "feat(run): add worktree isolation support"
```

---

### Task 3.2: Add Worktree Cleanup on Completion

**Files:**
- Modify: `src/main.rs` (handle_run function)

**Step 1: Write failing test**
```rust
// Manual verification
```

**Step 2: Verify failure**
Run: `cargo check`
Expected: Compiles (baseline)

**Step 3: Implement**
The worktree guard (`_worktree_guard`) already handles cleanup on drop if `auto_cleanup` is enabled. Add explicit cleanup logging at the end of `handle_run`:

```rust
    // Cleanup worktree if configured
    if config.worktree.enabled && config.worktree.auto_cleanup {
        if let Some(guard) = _worktree_guard {
            tracing::info!("Cleaning up worktree");
            drop(guard); // Explicit cleanup
        }
    }

    Ok(())
}
```

**Step 4: Verify pass**
Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**
```bash
git add src/main.rs
git commit -m "feat(run): add explicit worktree cleanup logging"
```

---

## Batch 4: Polish and Documentation

**Goal:** Add proper error messages and update documentation.

### Task 4.1: Improve Error Messages

**Files:**
- Modify: `src/main.rs`

**Step 1: Write failing test**
```rust
// Manual verification of error output
```

**Step 2: Verify failure**
Run: `cargo check`
Expected: Compiles (baseline)

**Step 3: Implement**
Update the error handling in the run command match arm:

```rust
            if let Err(e) = handle_run(task, resume, config).await {
                // Provide user-friendly error messages
                let msg = match e.downcast_ref::<claude_supervisor::cli::SpawnError>() {
                    Some(claude_supervisor::cli::SpawnError::NotFound) => {
                        "Claude CLI not found. Is 'claude' installed and in PATH?".to_string()
                    }
                    Some(claude_supervisor::cli::SpawnError::PermissionDenied) => {
                        "Permission denied when spawning Claude CLI".to_string()
                    }
                    _ => format!("{e}"),
                };
                eprintln!("error: {msg}");
                tracing::error!(error = %e, "Supervisor failed");
                std::process::exit(1);
            }
```

**Step 4: Verify pass**
Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**
```bash
git add src/main.rs
git commit -m "feat(run): improve error messages for common failures"
```

---

### Task 4.2: Close Issue #53

**Files:**
- None (GitHub action)

**Step 1: Verify all tests pass**
Run: `cargo nextest run`
Expected: All tests pass

**Step 2: Verify clippy**
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: No warnings

**Step 3: Create final commit**
```bash
git add -A
git commit -m "feat(run): implement run command to spawn and supervise Claude Code

Closes #53

- Wire ClaudeProcessBuilder to spawn Claude with stream-json output
- Connect Supervisor to parse events and enforce policies
- Add optional AI supervision via AiClient
- Support worktree isolation for task execution
- Add integration tests for run command"
```

**Step 4: Push and verify issue closes**
```bash
git push origin main
```

---

## Summary

| Batch | Tasks | Focus |
|-------|-------|-------|
| 1 | 1.1, 1.2 | Imports and function skeleton |
| 2 | 2.1, 2.2 | Wire command and add tests |
| 3 | 3.1, 3.2 | Worktree support |
| 4 | 4.1, 4.2 | Polish and close issue |

**Total: 4 batches, 8 tasks**

**Estimated complexity:** ~100-120 lines of new code in `handle_run` function.
