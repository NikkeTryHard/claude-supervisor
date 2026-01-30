//! Integration tests for claude-supervisor.

mod cli;
mod supervisor;

#[test]
fn test_multi_command_help() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--", "multi", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should show help without error
    assert!(
        combined.contains("--task"),
        "Help should mention --task flag"
    );
    assert!(
        combined.contains("--max-parallel"),
        "Help should mention --max-parallel flag"
    );
}
