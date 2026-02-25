use std::process::Command;

#[test]
fn test_binary_builds() {
    let status = Command::new("cargo")
        .args(["build"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("failed to run cargo build");
    assert!(status.success());
}

#[test]
fn test_help_flag() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("aimcp") || stdout.contains("Unified AI MCP Server"),
        "Help output should mention aimcp or its description, got: {}",
        stdout
    );
}

#[test]
fn test_version_flag() {
    let output = Command::new("cargo")
        .args(["run", "--", "--version"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("aimcp"),
        "Version output should contain 'aimcp', got: {}",
        stdout
    );
}
