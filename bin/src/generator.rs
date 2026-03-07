use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use seatbelt_lib::presets;
use seatbelt_lib::profile::loader;
use seatbelt_lib::profile::schema::*;
use seatbelt_lib::sbpl::ops::{classify_operation, OperationKind};

use crate::cli::GenerateArgs;

const LOG_BINARY: &str = "/usr/bin/log";
const DATE_BINARY: &str = "/bin/date";

/// Observed access patterns from running a process under a report-all sandbox.
#[derive(Debug, Default)]
struct Observations {
    file_reads: BTreeSet<String>,
    file_writes: BTreeSet<String>,
    network_outbound: bool,
    process_execs: BTreeSet<String>,
    mach_lookups: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct ObservedEvent {
    operation: String,
    path: String,
}

impl Observations {
    fn merge(&mut self, other: &Observations) {
        self.file_reads.extend(other.file_reads.iter().cloned());
        self.file_writes.extend(other.file_writes.iter().cloned());
        self.network_outbound |= other.network_outbound;
        self.process_execs
            .extend(other.process_execs.iter().cloned());
        self.mach_lookups.extend(other.mach_lookups.iter().cloned());
    }

    fn from_events(events: &[ObservedEvent]) -> Self {
        let mut obs = Observations::default();
        for event in events {
            match classify_operation(&event.operation) {
                OperationKind::FileRead => {
                    if !event.path.is_empty() {
                        obs.file_reads.insert(event.path.clone());
                    }
                }
                OperationKind::FileWrite => {
                    if !event.path.is_empty() {
                        obs.file_writes.insert(event.path.clone());
                    }
                }
                OperationKind::Network => {
                    obs.network_outbound = true;
                }
                OperationKind::ProcessExec => {
                    if !event.path.is_empty() {
                        obs.process_execs.insert(event.path.clone());
                    }
                }
                OperationKind::MachLookup => {
                    if !event.path.is_empty() {
                        obs.mach_lookups.insert(event.path.clone());
                    }
                }
                _ => {}
            }
        }
        obs
    }
}

/// Report-all SBPL profile that allows execution while reporting key operations.
const REPORT_ALL_SBPL: &str = "\
(version 1)
(allow default)
(allow file-read* (with report))
(allow file-write* (with report))
(allow network-outbound (with report))
(allow process-exec (with report))
(allow mach-lookup (with report))
";

/// Entry point for `seatbelt generate`.
pub async fn generate(args: &GenerateArgs) -> Result<()> {
    let observations = observe_process(&args.command, args.runs).await?;

    let profile = build_profile(&observations, args.base_preset.as_deref())?;

    let output_str = match args.format.as_str() {
        "yaml" => serde_yaml::to_string(&profile).context("failed to serialize profile as YAML")?,
        "sbpl" => {
            let cwd = std::env::current_dir().context("cannot determine current directory")?;
            let home = dirs::home_dir().context("cannot determine home directory")?;
            let resolved = seatbelt_lib::profile::resolver::resolve(&profile, &cwd, &home)?;
            seatbelt_lib::profile::compiler::compile(&resolved, None)?
        }
        _ => unreachable!("clap validates format"),
    };

    if let Some(ref output_path) = args.output {
        std::fs::write(output_path, &output_str)
            .with_context(|| format!("failed to write to {}", output_path.display()))?;
        eprintln!("{} wrote profile to {}", "✓".green(), output_path.display());
    } else {
        println!("{output_str}");
    }

    Ok(())
}

/// Run the command under a report-all sandbox, collecting reported operations.
/// Multiple runs are unioned to capture non-deterministic access patterns.
async fn observe_process(command: &[String], runs: u32) -> Result<Observations> {
    let sandbox_exec = Path::new("/usr/bin/sandbox-exec");
    if !sandbox_exec.exists() {
        anyhow::bail!(
            "{}",
            seatbelt_lib::error::SeatbeltError::SandboxExecNotFound
        );
    }

    let tmp =
        tempfile::NamedTempFile::new().context("failed to create temp file for report-all SBPL")?;
    std::fs::write(tmp.path(), REPORT_ALL_SBPL)
        .context("failed to write report-all SBPL to temp file")?;

    let mut combined = Observations::default();

    for run_idx in 0..runs {
        if runs > 1 {
            eprintln!("{} observation run {}/{}", "→".blue(), run_idx + 1, runs);
        }

        let start_time = format_iso8601_now();

        let mut child = tokio::process::Command::new(sandbox_exec)
            .args(["-f", &tmp.path().to_string_lossy()])
            .arg("--")
            .args(command)
            .spawn()
            .context("failed to spawn sandbox-exec")?;

        let pid = child.id().unwrap_or(0);
        if pid == 0 {
            bail!("failed to determine observed process PID");
        }
        let status = child
            .wait()
            .await
            .context("failed to wait on sandbox-exec")?;
        let run_events = query_reported_events(pid, &start_time)?;

        // sandbox-exec exits 65 on profile parse/compile errors.
        if run_events.is_empty() && status.code() == Some(65) {
            bail!("report-all observation profile failed to compile");
        }

        let obs = Observations::from_events(&run_events);
        eprintln!(
            "  observed: {} reads, {} writes, {} execs, {} mach lookups{}",
            obs.file_reads.len(),
            obs.file_writes.len(),
            obs.process_execs.len(),
            obs.mach_lookups.len(),
            if obs.network_outbound {
                ", network"
            } else {
                ""
            }
        );
        combined.merge(&obs);
    }

    Ok(combined)
}

fn query_reported_events(pid: u32, start_time: &str) -> Result<Vec<ObservedEvent>> {
    let predicate = "eventMessage CONTAINS \"Sandbox\"";
    let output = std::process::Command::new(LOG_BINARY)
        .args([
            "show",
            "--predicate",
            predicate,
            "--start",
            start_time,
            "--style",
            "compact",
        ])
        .output()
        .context("failed to run `log show` for observation mode")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`log show` failed for observation mode (status {}): {}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter_map(parse_reported_event_line)
        .filter_map(|(event_pid, event)| (event_pid == pid).then_some(event))
        .collect())
}

fn parse_reported_event_line(line: &str) -> Option<(u32, ObservedEvent)> {
    if !line.contains("Sandbox") || line.contains("duplicate report for Sandbox") {
        return None;
    }

    let event_pid = extract_event_pid(line)?;

    let details = if let Some(pos) = line.find(" allow ") {
        &line[pos + " allow ".len()..]
    } else if let Some(pos) = line.find("deny(") {
        let deny_tail = &line[pos..];
        let after_paren = deny_tail.find(") ")?;
        &deny_tail[after_paren + 2..]
    } else {
        return None;
    };

    let (operation, path) = match details.find(' ') {
        Some(pos) => (details[..pos].trim(), details[pos + 1..].trim()),
        None => (details.trim(), ""),
    };
    if operation.is_empty() {
        return None;
    }

    Some((
        event_pid,
        ObservedEvent {
            operation: operation.to_string(),
            path: path.to_string(),
        },
    ))
}

fn extract_event_pid(line: &str) -> Option<u32> {
    if let Some(pos) = line.find("pid:") {
        let after = &line[pos + 4..];
        let pid: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        return pid.parse().ok();
    }

    if let Some(pos) = line.find("Sandbox: ") {
        let after = &line[pos + "Sandbox: ".len()..];
        if let Some(open) = after.find('(') {
            let tail = &after[open + 1..];
            if let Some(close) = tail.find(')') {
                return tail[..close].parse().ok();
            }
        }
    }

    None
}

/// Build a Profile from observations, optionally subtracting a base preset's coverage.
fn build_profile(observations: &Observations, base_preset: Option<&str>) -> Result<Profile> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let read_paths = minimize_paths(&observations.file_reads, &cwd, &home);
    let write_paths = minimize_paths(&observations.file_writes, &cwd, &home);

    let mut profile = Profile {
        version: 1,
        name: Some("generated".into()),
        description: Some("auto-generated from observed process behavior".into()),
        extends: None,
        filesystem: FilesystemRules {
            read: read_paths,
            write: write_paths,
            deny: Vec::new(),
        },
        network: NetworkRules {
            outbound: OutboundNetworkRules {
                allow: observations.network_outbound,
                allow_domains: Vec::new(),
            },
            inbound: InboundNetworkRules { allow: false },
        },
        process: ProcessRules {
            allow_exec: observations.process_execs.iter().cloned().collect(),
            allow_exec_any: false,
            allow_fork: true,
        },
        system: SystemRules {
            allow_sysctl_read: true,
            allow_sysctl_write: false,
            allow_mach_lookup: observations.mach_lookups.iter().cloned().collect(),
        },
    };

    // If a base preset is given, subtract its rules and use `extends`
    if let Some(preset_name) = base_preset {
        let preset_yaml = presets::get_preset(preset_name)
            .ok_or_else(|| seatbelt_lib::error::SeatbeltError::UnknownPreset(preset_name.into()))?;
        let base: Profile = loader::load_profile_from_str(preset_yaml)?;

        subtract_preset(&mut profile, &base);
        profile.extends = Some(preset_name.to_string());
    }

    Ok(profile)
}

/// Remove rules from `profile` that are already covered by `base`.
fn subtract_preset(profile: &mut Profile, base: &Profile) {
    let base_reads: BTreeSet<_> = base.filesystem.read.iter().cloned().collect();
    let base_writes: BTreeSet<_> = base.filesystem.write.iter().cloned().collect();
    let base_execs: BTreeSet<_> = base.process.allow_exec.iter().cloned().collect();
    let base_mach: BTreeSet<_> = base.system.allow_mach_lookup.iter().cloned().collect();

    profile.filesystem.read.retain(|p| !base_reads.contains(p));
    profile
        .filesystem
        .write
        .retain(|p| !base_writes.contains(p));
    profile
        .process
        .allow_exec
        .retain(|p| !base_execs.contains(p));
    profile
        .system
        .allow_mach_lookup
        .retain(|s| !base_mach.contains(s));

    if base.network.outbound.allow && profile.network.outbound.allow {
        profile.network.outbound.allow = false;
    }
    if base.process.allow_exec_any {
        profile.process.allow_exec.clear();
        profile.process.allow_exec_any = false;
    }
}

/// Reduce a set of observed paths into minimal YAML-friendly path list.
///
/// Strategy:
/// 1. Reverse magic variables: home → (home), cwd → (cwd), tmpdir → (tmpdir)
/// 2. Group by prefix — if >2 siblings share a parent, use the parent
/// 3. Cap depth at 4 components
/// 4. Blocklist roots that are too broad
fn minimize_paths(paths: &BTreeSet<String>, cwd: &Path, home: &Path) -> Vec<String> {
    if paths.is_empty() {
        return Vec::new();
    }

    let cwd_str = cwd.to_string_lossy();
    let home_str = home.to_string_lossy();
    let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
    let tmpdir = tmpdir.trim_end_matches('/');

    // Step 1: reverse magic variables and cap depth
    let normalized: BTreeSet<String> = paths
        .iter()
        .map(|p| {
            let mut s = p.clone();
            // Order matters: cwd before home (cwd is often under home)
            if let Some(suffix) = s.strip_prefix(cwd_str.as_ref()) {
                s = format!("(cwd){suffix}");
            } else if s.starts_with(tmpdir) && tmpdir != "/tmp" {
                s = format!("(tmpdir){}", &s[tmpdir.len()..]);
            } else if s.starts_with("/tmp") || s.starts_with("/private/tmp") {
                s = format!(
                    "(tmpdir){}",
                    s.strip_prefix("/private/tmp")
                        .unwrap_or(s.strip_prefix("/tmp").unwrap_or(&s))
                );
            } else if let Some(suffix) = s.strip_prefix(home_str.as_ref()) {
                s = format!("(home){suffix}");
            }
            cap_depth(&s, 4)
        })
        .collect();

    // Step 2: group siblings — if >2 paths share a parent, collapse to parent
    let mut parent_counts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in &normalized {
        let parent = parent_path(p);
        parent_counts.entry(parent).or_default().push(p.clone());
    }

    let mut result: BTreeSet<String> = BTreeSet::new();
    for (parent, children) in &parent_counts {
        if children.len() > 2 && !is_blocklisted(parent) {
            result.insert(parent.clone());
        } else {
            for child in children {
                if !is_blocklisted(child) {
                    result.insert(child.clone());
                }
            }
        }
    }

    // Deduplicate: remove paths that are subpaths of other results
    let sorted: Vec<String> = result.into_iter().collect();
    let mut final_paths: Vec<String> = Vec::new();
    for path in &sorted {
        let already_covered = final_paths
            .iter()
            .any(|existing| path.starts_with(existing) && path.len() > existing.len());
        if !already_covered {
            final_paths.push(path.clone());
        }
    }

    final_paths
}

/// Truncate a path to at most `max` components.
fn cap_depth(path: &str, max: usize) -> String {
    // Handle magic variable prefixes
    let (prefix, rest) = if let Some(rest) = path.strip_prefix("(cwd)") {
        ("(cwd)", rest)
    } else if let Some(rest) = path.strip_prefix("(home)") {
        ("(home)", rest)
    } else if let Some(rest) = path.strip_prefix("(tmpdir)") {
        ("(tmpdir)", rest)
    } else {
        ("", path)
    };

    let components: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if components.len() <= max {
        return path.to_string();
    }

    let truncated = components[..max].join("/");
    if prefix.is_empty() {
        format!("/{truncated}")
    } else {
        format!("{prefix}/{truncated}")
    }
}

/// Get the parent of a path string.
fn parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => path.to_string(),
    }
}

/// Paths that are too broad to include as-is.
fn is_blocklisted(path: &str) -> bool {
    matches!(
        path,
        "/" | "/usr" | "/System" | "/Library" | "/private" | "/var"
    )
}

/// Format current time as ISO 8601 for `log show --start`.
fn format_iso8601_now() -> String {
    // `log show --start` expects local wall-clock time in this format.
    if let Ok(output) = std::process::Command::new(DATE_BINARY)
        .args(["+%Y-%m-%d %H:%M:%S"])
        .output()
    {
        if output.status.success() {
            let local = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !local.is_empty() {
                return local;
            }
        }
    }

    // Fallback: UTC conversion without external command.
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (year, month, day) = days_to_civil(days as i64);
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02}")
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_cwd() -> PathBuf {
        PathBuf::from("/Users/test/project")
    }

    fn test_home() -> PathBuf {
        PathBuf::from("/Users/test")
    }

    #[test]
    fn parse_reported_allow_line() {
        let line = "2026-03-05 21:22:39.028 Df kernel[0:1fa1fd2] (Sandbox) Sandbox: cat(62534) allow file-read-data /private/etc/hosts";
        let (pid, event) = parse_reported_event_line(line).expect("expected allow event");
        assert_eq!(pid, 62534);
        assert_eq!(event.operation, "file-read-data");
        assert_eq!(event.path, "/private/etc/hosts");
    }

    #[test]
    fn parse_reported_deny_line() {
        let line = "2024-01-15 10:30:00.123 Sandbox  pid:1234 process:(bash) deny(1) file-read-data /etc/passwd";
        let (pid, event) = parse_reported_event_line(line).expect("expected deny event");
        assert_eq!(pid, 1234);
        assert_eq!(event.operation, "file-read-data");
        assert_eq!(event.path, "/etc/passwd");
    }

    #[test]
    fn ignore_duplicate_report_line() {
        let line = "2026-03-05 21:22:39.028 Df kernel[0:1fa1fd2] (Sandbox) 1 duplicate report for Sandbox: cat(62534) allow file-read-data /bin/cat";
        assert!(parse_reported_event_line(line).is_none());
    }

    #[test]
    fn extract_event_pid_from_modern_line() {
        let line = "2026-03-05 21:22:39.028 Df kernel[0:1fa1fd2] (Sandbox) Sandbox: cat(62534) allow file-read-data /private/etc/hosts";
        assert_eq!(extract_event_pid(line), Some(62534));
    }

    #[test]
    fn extract_event_pid_from_legacy_line() {
        let line = "2024-01-15 10:30:00.123 Sandbox  pid:1234 process:(bash) deny(1) file-read-data /etc/passwd";
        assert_eq!(extract_event_pid(line), Some(1234));
    }

    #[test]
    fn extract_event_pid_missing_returns_none() {
        assert_eq!(
            extract_event_pid("Sandbox: cat(x) allow file-read-data"),
            None
        );
    }

    #[test]
    fn observations_from_events_filters_empty_paths() {
        let events = vec![
            ObservedEvent {
                operation: "file-read-data".into(),
                path: "/etc/hosts".into(),
            },
            ObservedEvent {
                operation: "process-exec".into(),
                path: String::new(),
            },
            ObservedEvent {
                operation: "network-outbound".into(),
                path: "*:443".into(),
            },
        ];
        let obs = Observations::from_events(&events);
        assert!(obs.file_reads.contains("/etc/hosts"));
        assert!(obs.process_execs.is_empty());
        assert!(obs.network_outbound);
    }

    #[test]
    fn minimize_paths_groups_siblings() {
        let mut paths = BTreeSet::new();
        paths.insert("/usr/lib/libA.dylib".into());
        paths.insert("/usr/lib/libB.dylib".into());
        paths.insert("/usr/lib/libC.dylib".into());

        let result = minimize_paths(&paths, &test_cwd(), &test_home());
        // 3 siblings → collapsed to parent
        assert!(result.contains(&"/usr/lib".to_string()));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn minimize_paths_respects_depth_cap() {
        assert_eq!(cap_depth("/a/b/c/d/e/f/g", 4), "/a/b/c/d");
        assert_eq!(cap_depth("(home)/a/b/c/d/e", 4), "(home)/a/b/c/d");
    }

    #[test]
    fn minimize_paths_blocklist() {
        let mut paths = BTreeSet::new();
        paths.insert("/usr".into());
        paths.insert("/System".into());
        paths.insert("/".into());

        let result = minimize_paths(&paths, &test_cwd(), &test_home());
        assert!(result.is_empty());
    }

    #[test]
    fn minimize_paths_magic_vars() {
        let cwd = PathBuf::from("/Users/test/project");
        let home = PathBuf::from("/Users/test");

        let mut paths = BTreeSet::new();
        paths.insert("/Users/test/project/src/main.rs".into());
        paths.insert("/Users/test/.config/foo".into());

        let result = minimize_paths(&paths, &cwd, &home);
        assert!(result.iter().any(|p| p.starts_with("(cwd)")));
        assert!(result.iter().any(|p| p.starts_with("(home)")));
    }

    #[test]
    fn build_profile_from_observations() {
        let obs = Observations {
            file_reads: ["/usr/lib/libfoo.dylib".into()].into(),
            file_writes: ["/tmp/output".into()].into(),
            network_outbound: true,
            process_execs: ["/usr/bin/git".into()].into(),
            mach_lookups: ["com.apple.system.logger".into()].into(),
        };

        let profile = build_profile(&obs, None).unwrap();
        assert_eq!(profile.version, 1);
        assert!(profile.network.outbound.allow);
        assert!(profile
            .process
            .allow_exec
            .contains(&"/usr/bin/git".to_string()));
        assert!(profile
            .system
            .allow_mach_lookup
            .contains(&"com.apple.system.logger".to_string()));
    }

    #[test]
    fn base_preset_subtraction() {
        let obs = Observations {
            file_reads: BTreeSet::new(),
            file_writes: BTreeSet::new(),
            network_outbound: true,
            process_execs: BTreeSet::new(),
            mach_lookups: ["com.apple.system.logger".into()].into(),
        };

        let profile = build_profile(&obs, Some("ai-agent-networked")).unwrap();
        // ai-agent-networked already allows outbound — should be subtracted
        assert!(!profile.network.outbound.allow);
        // ai-agent-networked already includes this mach lookup — should be subtracted
        assert!(!profile
            .system
            .allow_mach_lookup
            .contains(&"com.apple.system.logger".to_string()));
        assert_eq!(profile.extends.as_deref(), Some("ai-agent-networked"));
    }

    #[test]
    fn cap_depth_short_path() {
        assert_eq!(cap_depth("/usr/lib", 4), "/usr/lib");
    }

    #[test]
    fn cap_depth_with_magic_prefix() {
        assert_eq!(cap_depth("(tmpdir)/a/b", 4), "(tmpdir)/a/b");
    }

    #[test]
    fn parent_path_root() {
        assert_eq!(parent_path("/usr"), "/");
    }

    #[test]
    fn parent_path_nested() {
        assert_eq!(parent_path("/usr/lib/foo"), "/usr/lib");
    }

    #[test]
    fn blocklist_rejects_broad_paths() {
        assert!(is_blocklisted("/"));
        assert!(is_blocklisted("/usr"));
        assert!(is_blocklisted("/System"));
        assert!(!is_blocklisted("/usr/lib"));
        assert!(!is_blocklisted("/opt/homebrew"));
    }
}
