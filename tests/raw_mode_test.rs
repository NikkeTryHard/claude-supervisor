//! Integration test for --raw flag

use std::process::Command;

#[test]
fn test_raw_flag_accepted() {
    let output = Command::new("cargo")
        .args(["run", "--", "run", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("--raw"),
        "Expected --raw flag in help output"
    );
}
