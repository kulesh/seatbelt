use crate::error::{Result, SeatbeltError};

const LOG_BINARY: &str = "/usr/bin/log";

/// A parsed sandbox violation from macOS system log output.
#[derive(Debug, Clone)]
pub struct Violation {
    pub process_name: String,
    pub pid: u32,
    pub operation: String,
    pub path: String,
    pub raw: String,
}

/// Parse a single log line into a `Violation`, if it contains a sandbox deny.
///
/// macOS `log` compact format example:
/// ```text
/// 2024-01-15 10:30:00.123 Sandbox  pid:1234 process:(bash) deny(1) file-read-data /etc/passwd
/// ```
///
/// We look for lines containing "Sandbox" and "deny(" then extract the operation and path.
pub fn parse_violation_line(line: &str) -> Option<Violation> {
    // Must contain a sandbox deny
    if !line.contains("Sandbox") || !line.contains("deny(") {
        return None;
    }

    let raw = line.to_string();

    // Extract PID from various formats
    let pid = extract_pid(line).unwrap_or(0);

    // Extract process name
    let process_name = extract_process_name(line).unwrap_or_default();

    // Extract operation and path from the deny() portion
    // Pattern: deny(...) <operation> <path>
    let deny_pos = line.find("deny(")?;
    let after_deny = &line[deny_pos..];

    // Skip past "deny(N) "
    let after_paren = after_deny.find(") ")?;
    let remainder = &after_deny[after_paren + 2..];

    // First token is operation, rest is path
    let (operation, path) = match remainder.find(' ') {
        Some(pos) => (
            remainder[..pos].to_string(),
            remainder[pos + 1..].trim().to_string(),
        ),
        None => (remainder.trim().to_string(), String::new()),
    };

    Some(Violation {
        process_name,
        pid,
        operation,
        path,
        raw,
    })
}

/// Query the macOS system log for past violations by PID.
///
/// Runs: `log show --predicate 'eventMessage CONTAINS "Sandbox"' --start {start_time} --style compact`
/// and then filters parsed violations by the target PID.
pub fn query_violations(pid: u32, start_time: &str) -> Result<Vec<Violation>> {
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
        .map_err(|e| SeatbeltError::LogStreamError(format!("failed to run `log show`: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let violations = stdout
        .lines()
        .filter_map(parse_violation_line)
        .filter(|v| v.pid == pid)
        .collect();

    Ok(violations)
}

/// Spawn an async `log stream` process that yields violations as they occur.
pub fn stream_violations(
    pid: u32,
) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Violation> + Send>> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio_stream::wrappers::LinesStream;
    use tokio_stream::StreamExt;

    let predicate = "eventMessage CONTAINS \"Sandbox\"";

    let child = tokio::process::Command::new(LOG_BINARY)
        .args(["stream", "--predicate", predicate, "--style", "compact"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn();

    match child {
        Ok(mut child) => {
            let stdout = child.stdout.take().expect("stdout was piped");
            let reader = BufReader::new(stdout);
            let lines = LinesStream::new(reader.lines());
            Box::pin(lines.filter_map(
                move |line_result: std::result::Result<String, std::io::Error>| {
                    line_result
                        .ok()
                        .and_then(|line| parse_violation_line(&line))
                        .filter(|v| v.pid == pid)
                },
            ))
        }
        Err(_) => Box::pin(tokio_stream::empty()),
    }
}

fn extract_pid(line: &str) -> Option<u32> {
    // Try "pid:NNN" format
    if let Some(pos) = line.find("pid:") {
        let after = &line[pos + 4..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(pid) = num_str.parse() {
            return Some(pid);
        }
    }
    // Try "Sandbox: procname(PID)" format used in modern compact output
    if let Some(pos) = line.find("Sandbox: ") {
        let after = &line[pos + 9..];
        if let Some(open) = after.find('(') {
            let tail = &after[open + 1..];
            if let Some(close) = tail.find(')') {
                if let Ok(pid) = tail[..close].parse() {
                    return Some(pid);
                }
            }
        }
    }
    // Try "processID == NNN" or "[NNN]" format
    if let Some(pos) = line.find("processID == ") {
        let after = &line[pos + 13..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(pid) = num_str.parse() {
            return Some(pid);
        }
    }
    // Try [PID] format common in compact output
    if let Some(start) = line.find('[') {
        let after = &line[start + 1..];
        if let Some(end) = after.find(']') {
            if let Ok(pid) = after[..end].trim().parse() {
                return Some(pid);
            }
        }
    }
    None
}

fn extract_process_name(line: &str) -> Option<String> {
    // Try "process:(name)" format
    if let Some(pos) = line.find("process:(") {
        let after = &line[pos + 9..];
        if let Some(end) = after.find(')') {
            return Some(after[..end].to_string());
        }
    }
    // Try "process:name" format (no parens)
    if let Some(pos) = line.find("process:") {
        let after = &line[pos + 8..];
        let name: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    // Try "Sandbox: name(PID)" format used in modern compact output.
    if let Some(pos) = line.find("Sandbox: ") {
        let after = &line[pos + 9..];
        if let Some(open) = after.find('(') {
            let name = after[..open].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_deny_line() {
        let line = "2024-01-15 10:30:00.123 Sandbox  pid:1234 process:(bash) deny(1) file-read-data /etc/passwd";
        let v = parse_violation_line(line).unwrap();
        assert_eq!(v.pid, 1234);
        assert_eq!(v.process_name, "bash");
        assert_eq!(v.operation, "file-read-data");
        assert_eq!(v.path, "/etc/passwd");
    }

    #[test]
    fn parse_network_deny() {
        let line = "2024-01-15 10:30:00.123 Sandbox  pid:5678 process:(curl) deny(1) network-outbound *:443";
        let v = parse_violation_line(line).unwrap();
        assert_eq!(v.operation, "network-outbound");
        assert_eq!(v.path, "*:443");
    }

    #[test]
    fn parse_modern_compact_deny_line() {
        let line = "2026-03-05 21:26:41.625 E  kernel[0:1fa47b2] (Sandbox) Sandbox: cat(63674) deny(1) file-read-data /private/etc/hosts";
        let v = parse_violation_line(line).unwrap();
        assert_eq!(v.pid, 63674);
        assert_eq!(v.process_name, "cat");
        assert_eq!(v.operation, "file-read-data");
        assert_eq!(v.path, "/private/etc/hosts");
    }

    #[test]
    fn parse_no_path() {
        let line = "2024-01-15 10:30:00.123 Sandbox  pid:99 process:(test) deny(1) sysctl-write";
        let v = parse_violation_line(line).unwrap();
        assert_eq!(v.operation, "sysctl-write");
        assert!(v.path.is_empty());
    }

    #[test]
    fn non_sandbox_line_returns_none() {
        let line = "2024-01-15 10:30:00.123 kernel  some other message";
        assert!(parse_violation_line(line).is_none());
    }

    #[test]
    fn sandbox_without_deny_returns_none() {
        let line = "2024-01-15 10:30:00.123 Sandbox  pid:1234 process:(bash) allow(1) file-read-data /etc/passwd";
        assert!(parse_violation_line(line).is_none());
    }

    #[test]
    fn extract_pid_formats() {
        assert_eq!(extract_pid("pid:1234 foo"), Some(1234));
        assert_eq!(
            extract_pid("Sandbox: cat(63674) allow file-read-data"),
            Some(63674)
        );
        assert_eq!(extract_pid("processID == 5678 bar"), Some(5678));
        assert_eq!(extract_pid("foo [42] bar"), Some(42));
        assert_eq!(extract_pid("no pid here"), None);
    }

    #[test]
    fn extract_process_name_formats() {
        assert_eq!(
            extract_process_name("process:(bash) foo"),
            Some("bash".into())
        );
        assert_eq!(
            extract_process_name("Sandbox: cat(63674) deny(1) file-read-data /etc/hosts"),
            Some("cat".into())
        );
        assert_eq!(
            extract_process_name("process:curl foo"),
            Some("curl".into())
        );
        assert_eq!(extract_process_name("no process here"), None);
    }
}
