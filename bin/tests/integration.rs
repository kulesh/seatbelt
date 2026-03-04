use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;

fn seatbelt() -> assert_cmd::Command {
    cargo_bin_cmd!("seatbelt")
}

fn write_temp_profile(content: &str) -> tempfile::NamedTempFile {
    let f = tempfile::NamedTempFile::new().unwrap();
    fs::write(f.path(), content).unwrap();
    f
}

// --- compile ---

#[test]
fn compile_minimal_yaml() {
    let profile = write_temp_profile("version: 1\n");
    seatbelt()
        .args(["compile", &profile.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("(version 1)"))
        .stdout(predicate::str::contains("(deny default)"));
}

#[test]
fn compile_with_output_flag() {
    let profile = write_temp_profile("version: 1\n");
    let output = tempfile::NamedTempFile::new().unwrap();

    seatbelt()
        .args([
            "compile",
            "--output",
            &output.path().to_string_lossy(),
            &profile.path().to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let content = fs::read_to_string(output.path()).unwrap();
    assert!(content.contains("(version 1)"));
    assert!(content.contains("(deny default)"));
}

#[test]
fn compile_with_extends() {
    let yaml = "version: 1\nextends: ai-agent-strict\nnetwork:\n  outbound:\n    allow: true\n";
    let profile = write_temp_profile(yaml);
    seatbelt()
        .args(["compile", &profile.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("(allow network-outbound)"))
        .stdout(predicate::str::contains("(allow file-read*"));
}

#[test]
fn compile_unknown_field_error() {
    let profile = write_temp_profile("version: 1\nfilesytem:\n  read: []\n");
    seatbelt()
        .args(["compile", &profile.path().to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown field"));
}

#[test]
fn compile_file_not_found() {
    seatbelt()
        .args(["compile", "/nonexistent/path.yaml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// --- run ---

#[test]
fn run_dry_run_with_preset() {
    seatbelt()
        .args([
            "run",
            "--dry-run",
            "--preset",
            "ai-agent-strict",
            "--",
            "echo",
            "hello",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("(version 1)"))
        .stdout(predicate::str::contains("(deny default)"))
        .stdout(predicate::str::contains("(allow process-exec)"));
}

#[test]
fn run_dry_run_with_profile() {
    let profile = write_temp_profile("version: 1\nprocess:\n  allow_exec_any: true\n");
    seatbelt()
        .args([
            "run",
            "--dry-run",
            "--profile",
            &profile.path().to_string_lossy(),
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("(version 1)"));
}

#[test]
fn run_unknown_preset() {
    seatbelt()
        .args(["run", "--preset", "nonexistent", "--", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown preset"));
}

// --- help ---

#[test]
fn help_shows_all_commands() {
    seatbelt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("compile"))
        .stdout(predicate::str::contains("generate"))
        .stdout(predicate::str::contains("explain"))
        .stdout(predicate::str::contains("check"));
}

// --- check ---

#[test]
fn check_valid_profile() {
    let profile = write_temp_profile(
        "version: 1\nname: valid\nfilesystem:\n  write:\n    - (cwd)\nprocess:\n  allow_exec_any: true\n",
    );
    seatbelt()
        .args(["check", &profile.path().to_string_lossy()])
        .assert()
        .success();
}

#[test]
fn check_invalid_version() {
    let profile = write_temp_profile("version: 99\nname: bad\n");
    seatbelt()
        .args(["check", &profile.path().to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported profile version"));
}

#[test]
fn check_strict_warnings() {
    // Profile with no exec permissions triggers a warning
    let profile = write_temp_profile("version: 1\nname: strict-test\n");
    seatbelt()
        .args(["check", "--strict", &profile.path().to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("warning"));
}

#[test]
fn run_aborts_on_lint_error() {
    let profile = write_temp_profile("version: 99\nname: bad\nprocess:\n  allow_exec_any: true\n");
    seatbelt()
        .args([
            "run",
            "--dry-run",
            "--profile",
            &profile.path().to_string_lossy(),
            "--",
            "echo",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lint error"));
}

// --- stub commands ---

#[test]
fn generate_not_yet_implemented() {
    seatbelt()
        .args(["generate", "--", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn explain_not_yet_implemented() {
    seatbelt()
        .arg("explain")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yet implemented"));
}
