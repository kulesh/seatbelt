use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use seatbelt_lib::explainer;
use seatbelt_lib::log_stream;
use seatbelt_lib::presets;
use seatbelt_lib::profile::linter::{self, Severity};
use seatbelt_lib::profile::{compiler, default, loader, resolver};
use seatbelt_lib::sbpl::ops::{classify_operation, OperationKind};
use tokio_stream::StreamExt;

use crate::cli::RunArgs;

/// Persisted metadata from the last `seatbelt run` invocation.
/// Used by `seatbelt explain` when no --pid or --log is given.
#[derive(serde::Serialize, serde::Deserialize)]
struct LastRun {
    pid: u32,
    start_time: String,
    command: Vec<String>,
}

/// Execute `seatbelt run` — the primary command.
pub async fn run(args: &RunArgs) -> Result<()> {
    let profile = load_profile_or_preset(&args.profile, &args.preset)?;

    // Lint before resolving — catch YAML-level mistakes early
    let diags = linter::lint(&profile);
    let mut lint_errors = 0usize;
    for d in &diags {
        let prefix = match d.severity {
            Severity::Error => {
                lint_errors += 1;
                "error".red().bold()
            }
            Severity::Warning => "warning".yellow().bold(),
            Severity::Info => "info".blue().bold(),
        };
        eprintln!("{prefix}: {}", d.message);
        if let Some(ref suggestion) = d.suggestion {
            eprintln!("  {} {suggestion}", "hint:".dimmed());
        }
    }
    if lint_errors > 0 {
        bail!(
            "{}",
            seatbelt_lib::error::SeatbeltError::LintErrors(lint_errors)
        );
    }

    let cwd = std::env::current_dir().context("cannot determine current directory")?;
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let resolved = resolver::resolve(&profile, &cwd, &home)?;

    let command_binary = resolve_binary(&args.command[0]);
    let sbpl = compiler::compile(&resolved, command_binary.as_deref())?;

    if args.dry_run {
        println!("{sbpl}");
        return Ok(());
    }

    let sandbox_exec = Path::new("/usr/bin/sandbox-exec");
    if !sandbox_exec.exists() {
        bail!(
            "{}",
            seatbelt_lib::error::SeatbeltError::SandboxExecNotFound
        );
    }

    let tmp = tempfile::NamedTempFile::new().context("failed to create temp file for SBPL")?;
    std::fs::write(tmp.path(), &sbpl).context("failed to write SBPL to temp file")?;

    // Record start time for post-mortem log queries
    let start_time = format_iso8601(SystemTime::now());

    let mut child = tokio::process::Command::new(sandbox_exec)
        .args(["-f", &tmp.path().to_string_lossy()])
        .arg("--")
        .args(&args.command)
        .spawn()
        .context("failed to spawn sandbox-exec")?;

    let pid = child.id().unwrap_or(0);

    // Persist LastRun for `seatbelt explain`
    persist_last_run(&LastRun {
        pid,
        start_time: start_time.clone(),
        command: args.command.clone(),
    });

    // If --verbose, stream violations in real time
    if args.verbose {
        let mut stream = log_stream::stream_violations(pid);
        let mut child_handle = tokio::spawn(async move { child.wait().await });

        // Read violations until the child exits
        loop {
            tokio::select! {
                Some(v) = stream.next() => {
                    let kind = classify_operation(&v.operation);
                    let prefix = match kind {
                        OperationKind::FileRead | OperationKind::FileWrite => "fs".yellow(),
                        OperationKind::Network => "net".red(),
                        OperationKind::ProcessExec => "exec".blue(),
                        _ => "sys".dimmed(),
                    };
                    eprintln!("[{prefix}] {} {}", v.operation, v.path);
                }
                result = &mut child_handle => {
                    let status = result.context("child task panicked")?
                        .context("failed to wait on sandbox-exec")?;

                    // Drain remaining violations
                    while let Some(v) = stream.next().await {
                        let kind = classify_operation(&v.operation);
                        let prefix = match kind {
                            OperationKind::FileRead | OperationKind::FileWrite => "fs".yellow(),
                            OperationKind::Network => "net".red(),
                            OperationKind::ProcessExec => "exec".blue(),
                            _ => "sys".dimmed(),
                        };
                        eprintln!("[{prefix}] {} {}", v.operation, v.path);
                    }

                    if args.explain {
                        print_post_mortem_explanations(pid, &start_time, false)?;
                    }

                    std::process::exit(status.code().unwrap_or(1));
                }
            }
        }
    } else {
        let status = child
            .wait()
            .await
            .context("failed to wait on sandbox-exec")?;

        if args.explain {
            print_post_mortem_explanations(pid, &start_time, false)?;
        }

        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Execute `seatbelt <external>` — find default profile, then run.
pub async fn run_external(args: &[String]) -> Result<()> {
    let profile_path = default::find_default_profile().ok_or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        default::no_profile_error(&cwd)
    })?;

    let run_args = RunArgs {
        profile: Some(profile_path),
        preset: None,
        dry_run: false,
        explain: false,
        verbose: false,
        command: args.to_vec(),
    };
    run(&run_args).await
}

/// Print explanations for violations from a completed run.
fn print_post_mortem_explanations(pid: u32, start_time: &str, show_all: bool) -> Result<()> {
    let violations = log_stream::query_violations(pid, start_time)?;

    if violations.is_empty() {
        eprintln!("{}", "No sandbox violations detected.".dimmed());
        return Ok(());
    }

    let filtered: Vec<_> = if show_all {
        violations
    } else {
        violations
            .into_iter()
            .filter(|v| {
                let kind = classify_operation(&v.operation);
                matches!(
                    kind,
                    OperationKind::FileRead | OperationKind::FileWrite | OperationKind::ProcessExec
                )
            })
            .collect()
    };

    eprintln!("\n{} {} violation(s):\n", "===".bold(), filtered.len());

    for v in &filtered {
        let explanation = explainer::explain_violation(v);
        eprintln!("  {} {}", "●".red(), explanation.headline.bold());
        eprintln!("    {}", explanation.context.dimmed());
        if let Some(ref fix) = explanation.yaml_fix {
            eprintln!("    {} {fix}", "fix:".green());
        }
        eprintln!();
    }

    Ok(())
}

/// Load the LastRun from the cache directory.
fn load_last_run() -> Result<LastRun> {
    let path = last_run_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("no previous run found at {}", path.display()))?;
    let last_run: LastRun =
        serde_json::from_str(&content).context("failed to parse last-run.json")?;
    Ok(last_run)
}

fn persist_last_run(last_run: &LastRun) {
    let path = last_run_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(last_run).unwrap_or_default();
    let _ = std::fs::write(&path, json);
}

fn last_run_path() -> PathBuf {
    let cache_dir = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".cache")
        });
    cache_dir.join("seatbelt").join("last-run.json")
}

fn load_profile_or_preset(
    profile_path: &Option<PathBuf>,
    preset_name: &Option<String>,
) -> Result<seatbelt_lib::profile::schema::Profile> {
    if let Some(path) = profile_path {
        Ok(loader::load_profile(path)?)
    } else if let Some(name) = preset_name {
        let yaml = presets::get_preset(name)
            .ok_or_else(|| seatbelt_lib::error::SeatbeltError::UnknownPreset(name.clone()))?;
        Ok(loader::load_profile_from_str(yaml)?)
    } else {
        let path = default::find_default_profile().ok_or_else(|| {
            let cwd = std::env::current_dir().unwrap_or_default();
            default::no_profile_error(&cwd)
        })?;
        Ok(loader::load_profile(&path)?)
    }
}

/// Resolve a command name to an absolute path.
/// If already absolute, return as-is. Otherwise search $PATH.
fn resolve_binary(name: &str) -> Option<String> {
    let path = Path::new(name);
    if path.is_absolute() {
        return Some(name.to_string());
    }

    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Path::new(dir).join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Format a `SystemTime` as ISO 8601 (YYYY-MM-DD HH:MM:SS) for `log show --start`.
fn format_iso8601(time: SystemTime) -> String {
    let dur = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    // Manual UTC breakdown — no chrono dependency
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to Y-M-D (civil calendar)
    let (year, month, day) = days_to_civil(days as i64);

    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02}")
}

/// Convert days since Unix epoch to (year, month, day).
/// Algorithm from Howard Hinnant's chrono-compatible date library.
fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Public: explain violations from a log file.
pub fn explain_from_log(path: &Path, show_all: bool) -> Result<()> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let violations: Vec<_> = content
        .lines()
        .filter_map(log_stream::parse_violation_line)
        .collect();

    if violations.is_empty() {
        eprintln!("No sandbox violations found in {}", path.display());
        return Ok(());
    }

    let filtered: Vec<_> = if show_all {
        violations
    } else {
        violations
            .into_iter()
            .filter(|v| {
                let kind = classify_operation(&v.operation);
                matches!(
                    kind,
                    OperationKind::FileRead | OperationKind::FileWrite | OperationKind::ProcessExec
                )
            })
            .collect()
    };

    for v in &filtered {
        let explanation = explainer::explain_violation(v);
        eprintln!("  {} {}", "●".red(), explanation.headline.bold());
        eprintln!("    {}", explanation.context.dimmed());
        if let Some(ref fix) = explanation.yaml_fix {
            eprintln!("    {} {fix}", "fix:".green());
        }
        eprintln!();
    }

    Ok(())
}

/// Public: explain violations from a PID by querying system log.
pub fn explain_from_pid(pid: u32, show_all: bool) -> Result<()> {
    // Use a generous lookback window — 1 hour ago
    let start_time = format_iso8601(
        SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(3600))
            .unwrap_or(SystemTime::UNIX_EPOCH),
    );
    print_post_mortem_explanations(pid, &start_time, show_all)
}

/// Public: explain violations from the last run.
pub fn explain_last_run(show_all: bool) -> Result<()> {
    let last_run = load_last_run()?;
    eprintln!(
        "Explaining violations for: {}",
        last_run.command.join(" ").bold()
    );
    print_post_mortem_explanations(last_run.pid, &last_run.start_time, show_all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_absolute_path() {
        assert_eq!(
            resolve_binary("/usr/bin/echo"),
            Some("/usr/bin/echo".into())
        );
    }

    #[test]
    fn resolve_from_path() {
        let result = resolve_binary("echo");
        assert!(result.is_some());
        assert!(result.unwrap().starts_with('/'));
    }

    #[test]
    fn resolve_nonexistent() {
        let result = resolve_binary("definitely_not_a_real_binary_xyz");
        assert!(result.is_none());
    }

    #[test]
    fn format_iso8601_epoch() {
        let s = format_iso8601(SystemTime::UNIX_EPOCH);
        assert_eq!(s, "1970-01-01 00:00:00");
    }

    #[test]
    fn format_iso8601_known_date() {
        // 2024-01-15 12:00:00 UTC = 1705320000 seconds since epoch
        let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1705320000);
        let s = format_iso8601(time);
        assert_eq!(s, "2024-01-15 12:00:00");
    }

    #[test]
    fn last_run_path_is_deterministic() {
        let path = last_run_path();
        assert!(path.ends_with("seatbelt/last-run.json"));
    }
}
