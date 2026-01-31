//! Tests for --activity CLI flag.

use std::process::Command;

#[test]
fn test_run_command_accepts_activity_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_claude-supervisor"))
        .args(["run", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--activity"),
        "Help should mention --activity flag"
    );
}
