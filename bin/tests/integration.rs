use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

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

// --- explain ---

#[test]
fn explain_no_last_run() {
    // With no prior run, explain should give a helpful error (not a crash)
    seatbelt()
        .arg("explain")
        .env("XDG_CACHE_HOME", "/tmp/seatbelt-test-nonexistent-cache")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no previous run"));
}

#[test]
fn explain_from_log_file() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sandbox-violations.log");
    seatbelt()
        .args(["explain", "--log", &fixture.to_string_lossy(), "--all"])
        .assert()
        .success();
}

#[test]
fn run_verbose_flag_accepted() {
    let profile = write_temp_profile("version: 1\nname: v\nprocess:\n  allow_exec_any: true\n");
    // --verbose with --dry-run should still work (dry-run exits before spawning)
    seatbelt()
        .args([
            "run",
            "--dry-run",
            "--verbose",
            "--profile",
            &profile.path().to_string_lossy(),
            "--",
            "echo",
        ])
        .assert()
        .success();
}

#[test]
fn run_explain_flag_accepted() {
    let profile = write_temp_profile("version: 1\nname: e\nprocess:\n  allow_exec_any: true\n");
    seatbelt()
        .args([
            "run",
            "--dry-run",
            "--explain",
            "--profile",
            &profile.path().to_string_lossy(),
            "--",
            "echo",
        ])
        .assert()
        .success();
}

#[test]
fn run_without_profile_bootstraps_global_default() {
    let temp = tempfile::tempdir().unwrap();
    let xdg = temp.path().join("xdg");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&xdg).unwrap();
    fs::create_dir_all(&cwd).unwrap();

    seatbelt()
        .current_dir(&cwd)
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["run", "--dry-run", "--", "echo", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(version 1)"));

    let generated = xdg.join("seatbelt/profile.yaml");
    assert!(
        generated.exists(),
        "expected default profile at {:?}",
        generated
    );
    let content = fs::read_to_string(generated).unwrap();
    assert!(content.contains("extends: ai-agent-networked"));
}

// --- generate ---

#[test]
fn generate_help() {
    seatbelt()
        .args(["generate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--output"))
        .stdout(predicate::str::contains("--base-preset"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--runs"));
}

fn write_executable_script(path: &std::path::Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[test]
fn explain_uses_absolute_log_and_date_binaries() {
    let temp = tempfile::tempdir().unwrap();
    let fake_bin = temp.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();

    let marker = temp.path().join("path-hijack-marker");
    let fake_log = format!(
        "#!/bin/sh\necho hijacked-log > \"{}\"\nexit 0\n",
        marker.display()
    );
    let fake_date = format!(
        "#!/bin/sh\necho hijacked-date > \"{}\"\nprintf '2026-03-06 00:00:00\\n'\n",
        marker.display()
    );

    write_executable_script(&fake_bin.join("log"), &fake_log);
    write_executable_script(&fake_bin.join("date"), &fake_date);

    let current_path = std::env::var("PATH").unwrap_or_default();
    let poisoned_path = format!("{}:{}", fake_bin.display(), current_path);

    // If PATH lookup is used, one of the fake binaries will create the marker file.
    let _ = seatbelt()
        .args(["explain", "--pid", "123"])
        .env("PATH", poisoned_path)
        .assert();

    assert!(
        !marker.exists(),
        "expected absolute helper binary paths; PATH-hijack marker was created"
    );
}

#[test]
fn check_rejects_allow_domains() {
    let profile = write_temp_profile(
        "version: 1\nname: bad-network\nnetwork:\n  outbound:\n    allow: true\n    allow_domains:\n      - example.com\nprocess:\n  allow_exec_any: true\n",
    );
    seatbelt()
        .args(["check", &profile.path().to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("allow_domains is not supported"));
}

#[test]
fn run_aborts_on_allow_domains_error() {
    let profile = write_temp_profile(
        "version: 1\nname: bad-network\nnetwork:\n  outbound:\n    allow: true\n    allow_domains:\n      - example.com\nprocess:\n  allow_exec_any: true\n",
    );
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
        .failure()
        .stderr(predicate::str::contains("allow_domains is not supported"));
}
