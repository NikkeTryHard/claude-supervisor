//! Tests for Claude process spawning and control.

use claude_supervisor::cli::{ClaudeProcess, ClaudeProcessBuilder, SpawnError};

#[test]
fn builder_new_creates_with_prompt() {
    let builder = ClaudeProcessBuilder::new("Fix the bug");
    let args = builder.build_args();

    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"Fix the bug".to_string()));
    assert!(args.contains(&"--output-format".to_string()));
    assert!(args.contains(&"stream-json".to_string()));
}

#[test]
fn builder_allowed_tools() {
    let builder = ClaudeProcessBuilder::new("task").allowed_tools(&["Read", "Write", "Bash"]);
    let args = builder.build_args();

    assert!(args.contains(&"--allowedTools".to_string()));
    assert!(args.contains(&"Read,Write,Bash".to_string()));
}

#[test]
fn builder_resume_session() {
    let builder = ClaudeProcessBuilder::new("continue").resume("session_abc123");
    let args = builder.build_args();

    assert!(args.contains(&"--resume".to_string()));
    assert!(args.contains(&"session_abc123".to_string()));
}

#[test]
fn builder_max_turns() {
    let builder = ClaudeProcessBuilder::new("task").max_turns(5);
    let args = builder.build_args();

    assert!(args.contains(&"--max-turns".to_string()));
    assert!(args.contains(&"5".to_string()));
}

#[test]
fn builder_append_system_prompt() {
    let builder = ClaudeProcessBuilder::new("task").append_system_prompt("Extra context here");
    let args = builder.build_args();

    assert!(args.contains(&"--append-system-prompt".to_string()));
    assert!(args.contains(&"Extra context here".to_string()));
}

#[test]
fn builder_system_prompt() {
    let builder = ClaudeProcessBuilder::new("task").system_prompt("Custom system prompt");
    let args = builder.build_args();

    assert!(args.contains(&"--system-prompt".to_string()));
    assert!(args.contains(&"Custom system prompt".to_string()));
}

#[test]
fn builder_chaining() {
    let builder = ClaudeProcessBuilder::new("task")
        .allowed_tools(&["Read"])
        .max_turns(10)
        .append_system_prompt("context");

    let args = builder.build_args();

    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"--allowedTools".to_string()));
    assert!(args.contains(&"--max-turns".to_string()));
    assert!(args.contains(&"--append-system-prompt".to_string()));
}

#[test]
fn builder_is_clone() {
    let builder = ClaudeProcessBuilder::new("task").max_turns(5);
    let cloned = builder.clone();

    assert_eq!(builder.build_args(), cloned.build_args());
}

#[test]
fn spawn_nonexistent_binary_returns_error() {
    let builder = ClaudeProcessBuilder::new("test");
    let result = ClaudeProcess::spawn_with_binary("nonexistent_binary_xyz", &builder);

    assert!(result.is_err());
    match result.unwrap_err() {
        SpawnError::NotFound => {}
        other => panic!("Expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn spawn_echo_and_wait() {
    let builder = ClaudeProcessBuilder::new("ignored");
    // Use echo as a test binary - it will just output and exit
    let result = ClaudeProcess::spawn_with_binary("echo", &builder);

    assert!(result.is_ok());
    let mut process = result.unwrap();

    // Process should have an ID
    assert!(process.id().is_some());

    // Wait for it to complete
    let status = process.wait().await;
    assert!(status.is_ok());
    assert!(status.unwrap().success());
}

#[tokio::test]
async fn take_stdout_once() {
    let builder = ClaudeProcessBuilder::new("hello");
    let mut process = ClaudeProcess::spawn_with_binary("echo", &builder).unwrap();

    // First take should succeed
    let stdout = process.take_stdout();
    assert!(stdout.is_some());

    // Second take should return None
    let stdout2 = process.take_stdout();
    assert!(stdout2.is_none());

    process.wait().await.unwrap();
}

#[tokio::test]
async fn take_stderr_once() {
    let builder = ClaudeProcessBuilder::new("test");
    let mut process = ClaudeProcess::spawn_with_binary("echo", &builder).unwrap();

    // First take should succeed
    let stderr = process.take_stderr();
    assert!(stderr.is_some());

    // Second take should return None
    let stderr2 = process.take_stderr();
    assert!(stderr2.is_none());

    process.wait().await.unwrap();
}

#[tokio::test]
async fn try_wait_on_running_process() {
    let builder = ClaudeProcessBuilder::new("1");
    // sleep 1 second to ensure process is still running when we check
    let mut process = ClaudeProcess::spawn_with_binary("sleep", &builder).unwrap();

    // Process should still be running
    let result = process.try_wait();
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());

    // Kill and cleanup
    process.kill().await.unwrap();
}

#[tokio::test]
async fn kill_running_process() {
    let builder = ClaudeProcessBuilder::new("10");
    let mut process = ClaudeProcess::spawn_with_binary("sleep", &builder).unwrap();

    // Kill should succeed
    let result = process.kill().await;
    assert!(result.is_ok());

    // Wait should show non-success (killed)
    let status = process.wait().await.unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn graceful_terminate_with_timeout() {
    let builder = ClaudeProcessBuilder::new("10");
    let mut process = ClaudeProcess::spawn_with_binary("sleep", &builder).unwrap();

    // Graceful terminate with short timeout
    let result = process
        .graceful_terminate(std::time::Duration::from_millis(100))
        .await;
    assert!(result.is_ok());
}
