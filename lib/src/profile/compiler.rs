use crate::error::{Result, SeatbeltError};
use crate::profile::resolver::{glob_to_regex, ResolvedPath, ResolvedProfile};

const SBPL_MAX_SIZE: usize = 65_535;
const SBPL_WARN_SIZE: usize = 50_000;

/// Compile a resolved profile into an SBPL string.
/// `command_binary` is the absolute path of the command to sandbox — an exec-allow
/// rule is automatically emitted for it so the user doesn't need to list it.
pub fn compile(profile: &ResolvedProfile, command_binary: Option<&str>) -> Result<String> {
    let mut rules: Vec<String> = Vec::with_capacity(64);

    // Header
    rules.push("(version 1)".into());
    rules.push("(deny default)".into());
    rules.push("(deny file-write-create (vnode-type SYMLINK))".into());

    // Baseline — required by virtually every process
    emit_baseline_rules(&mut rules);

    // Filesystem: write (emit both read + write)
    for rp in &profile.filesystem_write {
        emit_path_rule(&mut rules, "allow", "file-read*", rp);
        emit_path_rule(&mut rules, "allow", "file-write*", rp);
    }

    // Filesystem: read
    for rp in &profile.filesystem_read {
        emit_path_rule(&mut rules, "allow", "file-read*", rp);
    }

    // Filesystem: deny (last — wins over allows via last-rule-wins semantics)
    for rp in &profile.filesystem_deny {
        emit_path_rule(&mut rules, "deny", "file-read*", rp);
        emit_path_rule(&mut rules, "deny", "file-write*", rp);
    }

    // Network
    if profile.network.outbound.allow {
        rules.push("(allow network-outbound)".into());
    }
    if profile.network.inbound.allow {
        rules.push("(allow network-inbound)".into());
    }

    // Process: fork
    if profile.process.allow_fork {
        rules.push("(allow process-fork)".into());
    }

    // Process: auto exec-allow for the sandboxed command
    if let Some(binary) = command_binary {
        rules.push(format!(
            "(allow process-exec (literal {}))",
            sbpl_string(binary)
        ));
    }

    // Process: exec rules from profile
    if profile.process.allow_exec_any {
        rules.push("(allow process-exec)".into());
    } else {
        for path in &profile.process.allow_exec {
            emit_exec_rule(&mut rules, path);
        }
    }

    // System: sysctl
    if profile.system.allow_sysctl_read {
        rules.push("(allow sysctl-read)".into());
    }
    if profile.system.allow_sysctl_write {
        rules.push("(allow sysctl-write)".into());
    }

    // System: mach-lookup
    for service in &profile.system.allow_mach_lookup {
        rules.push(format!(
            "(allow mach-lookup (global-name {}))",
            sbpl_string(service)
        ));
    }

    let sbpl = rules.join("\n");

    if sbpl.len() > SBPL_MAX_SIZE {
        return Err(SeatbeltError::CompilationError(format!(
            "Generated SBPL is {} bytes, exceeding the {} byte kernel limit",
            sbpl.len(),
            SBPL_MAX_SIZE
        )));
    }
    if sbpl.len() > SBPL_WARN_SIZE {
        eprintln!(
            "seatbelt: warning: generated SBPL is {} bytes (limit: {})",
            sbpl.len(),
            SBPL_MAX_SIZE
        );
    }

    Ok(sbpl)
}

/// Escape a value for use inside an SBPL string literal.
fn sbpl_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    format!("\"{escaped}\"")
}

/// Dispatch: glob -> regex matcher, literal -> subpath matcher.
fn emit_path_rule(rules: &mut Vec<String>, action: &str, operation: &str, rp: &ResolvedPath) {
    if rp.is_glob {
        let regex = glob_to_regex(&rp.path);
        rules.push(format!(
            "({action} {operation} (regex {}))",
            sbpl_string(&regex)
        ));
    } else {
        rules.push(format!(
            "({action} {operation} (subpath {}))",
            sbpl_string(&rp.path)
        ));
    }
}

/// Emit exec rules: literal for exact paths, regex for globs.
fn emit_exec_rule(rules: &mut Vec<String>, path: &str) {
    if path.contains('*') || path.contains('?') {
        let regex = glob_to_regex(path);
        rules.push(format!(
            "(allow process-exec (regex {}))",
            sbpl_string(&regex)
        ));
    } else {
        rules.push(format!(
            "(allow process-exec (literal {}))",
            sbpl_string(path)
        ));
    }
}

fn emit_baseline_rules(rules: &mut Vec<String>) {
    rules.push("(allow file-read* (subpath \"/usr/lib\"))".into());
    rules.push("(allow file-read* (subpath \"/usr/share\"))".into());
    rules.push("(allow file-read* (subpath \"/System/Library\"))".into());
    rules.push("(allow file-read* (subpath \"/Library/Apple\"))".into());
    rules.push("(allow file-read* (subpath \"/private/var/db/dyld\"))".into());
    rules.push("(allow file-read-metadata (literal \"/\"))".into());
    rules.push("(allow file-read-metadata (literal \"/usr\"))".into());
    rules.push("(allow file-read-metadata (literal \"/var\"))".into());
    rules.push("(allow process-exec (subpath \"/Library/Apple/usr/libexec/oah\"))".into());
    rules.push("(allow signal (target self))".into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::schema::*;

    fn minimal_resolved() -> ResolvedProfile {
        ResolvedProfile {
            filesystem_read: vec![],
            filesystem_write: vec![],
            filesystem_deny: vec![],
            network: NetworkRules::default(),
            process: ProcessRules::default(),
            system: SystemRules::default(),
        }
    }

    #[test]
    fn header_and_baseline() {
        let sbpl = compile(&minimal_resolved(), None).unwrap();
        let lines: Vec<&str> = sbpl.lines().collect();
        assert_eq!(lines[0], "(version 1)");
        assert_eq!(lines[1], "(deny default)");
        assert_eq!(lines[2], "(deny file-write-create (vnode-type SYMLINK))");
        assert!(sbpl.contains("(allow file-read* (subpath \"/usr/lib\"))"));
        assert!(sbpl.contains("(allow file-read* (subpath \"/private/var/db/dyld\"))"));
        assert!(sbpl.contains("(allow signal (target self))"));
    }

    #[test]
    fn rosetta2_in_baseline() {
        let sbpl = compile(&minimal_resolved(), None).unwrap();
        assert!(sbpl.contains("(allow process-exec (subpath \"/Library/Apple/usr/libexec/oah\"))"));
    }

    #[test]
    fn no_broad_exec_in_baseline() {
        let sbpl = compile(&minimal_resolved(), None).unwrap();
        let has_bare_exec = sbpl.lines().any(|l| l.trim() == "(allow process-exec)");
        assert!(!has_bare_exec);
    }

    #[test]
    fn subpath_for_literal_read() {
        let mut p = minimal_resolved();
        p.filesystem_read.push(ResolvedPath {
            path: "/opt/homebrew".into(),
            is_glob: false,
        });
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow file-read* (subpath \"/opt/homebrew\"))"));
    }

    #[test]
    fn regex_for_glob_deny() {
        let mut p = minimal_resolved();
        p.filesystem_deny.push(ResolvedPath {
            path: "/Users/test/.ssh/id_*".into(),
            is_glob: true,
        });
        let sbpl = compile(&p, None).unwrap();
        let expected_regex = sbpl_string(r"^/Users/test/\.ssh/id_[^/]*$");
        let expected_read = format!("(deny file-read* (regex {expected_regex}))");
        let expected_write = format!("(deny file-write* (regex {expected_regex}))");
        assert!(
            sbpl.contains(&expected_read),
            "missing deny file-read regex"
        );
        assert!(
            sbpl.contains(&expected_write),
            "missing deny file-write regex"
        );
    }

    #[test]
    fn write_emits_both_read_and_write() {
        let mut p = minimal_resolved();
        p.filesystem_write.push(ResolvedPath {
            path: "/tmp/work".into(),
            is_glob: false,
        });
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow file-read* (subpath \"/tmp/work\"))"));
        assert!(sbpl.contains("(allow file-write* (subpath \"/tmp/work\"))"));
    }

    #[test]
    fn auto_exec_allow() {
        let sbpl = compile(&minimal_resolved(), Some("/usr/bin/python3")).unwrap();
        assert!(sbpl.contains("(allow process-exec (literal \"/usr/bin/python3\"))"));
    }

    #[test]
    fn allow_exec_any() {
        let mut p = minimal_resolved();
        p.process.allow_exec_any = true;
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.lines().any(|l| l.trim() == "(allow process-exec)"));
    }

    #[test]
    fn specific_exec_literal() {
        let mut p = minimal_resolved();
        p.process.allow_exec = vec!["/usr/bin/git".into()];
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow process-exec (literal \"/usr/bin/git\"))"));
    }

    #[test]
    fn specific_exec_glob() {
        let mut p = minimal_resolved();
        p.process.allow_exec = vec!["/opt/homebrew/bin/python3.*".into()];
        let sbpl = compile(&p, None).unwrap();
        let expected = format!(
            "(allow process-exec (regex {}))",
            sbpl_string(r"^/opt/homebrew/bin/python3\.[^/]*$")
        );
        assert!(sbpl.contains(&expected), "missing exec glob regex");
    }

    #[test]
    fn network_outbound() {
        let mut p = minimal_resolved();
        p.network.outbound.allow = true;
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow network-outbound)"));
    }

    #[test]
    fn network_inbound() {
        let mut p = minimal_resolved();
        p.network.inbound.allow = true;
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow network-inbound)"));
    }

    #[test]
    fn sysctl_read() {
        let sbpl = compile(&minimal_resolved(), None).unwrap();
        assert!(sbpl.contains("(allow sysctl-read)"));
    }

    #[test]
    fn sysctl_write() {
        let mut p = minimal_resolved();
        p.system.allow_sysctl_write = true;
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow sysctl-write)"));
    }

    #[test]
    fn mach_lookup() {
        let mut p = minimal_resolved();
        p.system.allow_mach_lookup = vec!["com.apple.system.logger".into()];
        let sbpl = compile(&p, None).unwrap();
        assert!(sbpl.contains("(allow mach-lookup (global-name \"com.apple.system.logger\"))"));
    }

    #[test]
    fn fork_allowed_by_default() {
        let sbpl = compile(&minimal_resolved(), None).unwrap();
        assert!(sbpl.contains("(allow process-fork)"));
    }

    #[test]
    fn fork_disabled() {
        let mut p = minimal_resolved();
        p.process.allow_fork = false;
        let sbpl = compile(&p, None).unwrap();
        assert!(!sbpl.contains("(allow process-fork)"));
    }

    #[test]
    fn deny_comes_after_allow() {
        let mut p = minimal_resolved();
        p.filesystem_read.push(ResolvedPath {
            path: "/Users/test".into(),
            is_glob: false,
        });
        p.filesystem_deny.push(ResolvedPath {
            path: "/Users/test/.ssh/id_*".into(),
            is_glob: true,
        });
        let sbpl = compile(&p, None).unwrap();
        let allow_pos = sbpl
            .find("(allow file-read* (subpath \"/Users/test\"))")
            .unwrap();
        let deny_pos = sbpl.find("(deny file-read*").unwrap();
        assert!(deny_pos > allow_pos, "deny rules must follow allow rules");
    }

    #[test]
    fn size_limit_exceeded() {
        let mut p = minimal_resolved();
        for i in 0..3000 {
            p.filesystem_read.push(ResolvedPath {
                path: format!("/very/long/path/that/fills/space/{i}"),
                is_glob: false,
            });
        }
        let result = compile(&p, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kernel limit"));
    }

    #[test]
    fn symlink_prevention() {
        let sbpl = compile(&minimal_resolved(), None).unwrap();
        assert!(sbpl.contains("(deny file-write-create (vnode-type SYMLINK))"));
    }

    #[test]
    fn escapes_literal_path_for_subpath_rule() {
        let mut p = minimal_resolved();
        p.filesystem_read.push(ResolvedPath {
            path: r#"/tmp/quote"and\slash"#.into(),
            is_glob: false,
        });
        let sbpl = compile(&p, None).unwrap();
        let expected = format!(
            "(allow file-read* (subpath {}))",
            sbpl_string(r#"/tmp/quote"and\slash"#)
        );
        assert!(sbpl.contains(&expected), "missing escaped subpath literal");
    }

    #[test]
    fn escapes_command_binary_literal() {
        let binary = r#"/tmp/my "bin"\exec"#;
        let sbpl = compile(&minimal_resolved(), Some(binary)).unwrap();
        let expected = format!("(allow process-exec (literal {}))", sbpl_string(binary));
        assert!(sbpl.contains(&expected), "missing escaped command binary");
    }

    #[test]
    fn escapes_mach_lookup_name() {
        let mut p = minimal_resolved();
        p.system.allow_mach_lookup = vec![r#"com.example."svc"\name"#.into()];
        let sbpl = compile(&p, None).unwrap();
        let expected = format!(
            "(allow mach-lookup (global-name {}))",
            sbpl_string(r#"com.example."svc"\name"#)
        );
        assert!(sbpl.contains(&expected), "missing escaped mach lookup");
    }
}
