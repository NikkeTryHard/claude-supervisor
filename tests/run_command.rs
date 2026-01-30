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
        "Expected error when neither task nor resume provided, got: {stderr}"
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
    assert!(
        stdout.contains("--auto-continue"),
        "Expected --auto-continue in help"
    );
}
