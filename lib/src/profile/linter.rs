use crate::profile::schema::Profile;

/// Severity levels for lint diagnostics, ordered by increasing severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A single diagnostic produced by the linter.
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Lint a raw profile for common mistakes and misconfigurations.
///
/// Operates on the pre-resolution `Profile` because it checks YAML-level
/// concerns: missing names, suspicious path patterns, contradictory flags.
pub fn lint(profile: &Profile) -> Vec<LintDiagnostic> {
    let mut diags = Vec::new();

    // Rule 1: version must be 1
    if profile.version != 1 {
        diags.push(LintDiagnostic {
            severity: Severity::Error,
            message: format!(
                "unsupported profile version {} (only version 1 is supported)",
                profile.version
            ),
            suggestion: Some("set `version: 1`".into()),
        });
    }

    // Rule 2: allow_domains without outbound allow
    if !profile.network.outbound.allow_domains.is_empty() && !profile.network.outbound.allow {
        diags.push(LintDiagnostic {
            severity: Severity::Error,
            message: "allow_domains specified but outbound network is not allowed".into(),
            suggestion: Some("set `network.outbound.allow: true` or remove allow_domains".into()),
        });
    }

    // Rule 3: write path outside cwd/tmpdir
    for path in &profile.filesystem.write {
        if !is_safe_write_path(path) {
            diags.push(LintDiagnostic {
                severity: Severity::Warning,
                message: format!("write path outside cwd/tmpdir: {path}"),
                suggestion: Some(
                    "write paths should generally use (cwd) or (tmpdir) prefixes".into(),
                ),
            });
        }
    }

    // Rule 4: unrestricted outbound network
    if profile.network.outbound.allow && profile.network.outbound.allow_domains.is_empty() {
        diags.push(LintDiagnostic {
            severity: Severity::Warning,
            message: "unrestricted outbound network access".into(),
            suggestion: Some("consider limiting with allow_domains or disabling outbound".into()),
        });
    }

    // Rule 5: no exec permissions configured
    if !profile.process.allow_exec_any && profile.process.allow_exec.is_empty() {
        diags.push(LintDiagnostic {
            severity: Severity::Warning,
            message: "no exec permissions configured".into(),
            suggestion: Some(
                "set `process.allow_exec_any: true` or list specific executables in allow_exec"
                    .into(),
            ),
        });
    }

    // Rule 6: profile has no name
    if profile.name.is_none() {
        diags.push(LintDiagnostic {
            severity: Severity::Info,
            message: "profile has no name".into(),
            suggestion: Some("add a `name` field for easier identification".into()),
        });
    }

    diags
}

/// A write path is "safe" if it targets cwd, tmpdir, or /tmp.
fn is_safe_write_path(path: &str) -> bool {
    path.contains("(cwd)")
        || path.contains("(tmpdir)")
        || path.starts_with("/tmp")
        || path.starts_with("/private/tmp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::schema::Profile;

    fn clean_profile() -> Profile {
        serde_yaml::from_str(
            r#"
version: 1
name: clean
filesystem:
  write:
    - (cwd)
    - (tmpdir)
process:
  allow_exec_any: true
"#,
        )
        .unwrap()
    }

    #[test]
    fn clean_profile_no_diagnostics() {
        let diags = lint(&clean_profile());
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn rule_version_must_be_1() {
        let mut p = clean_profile();
        p.version = 2;
        let diags = lint(&p);
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Error
                && d.message.contains("unsupported profile version")));
    }

    #[test]
    fn rule_allow_domains_without_outbound() {
        let p: Profile = serde_yaml::from_str(
            r#"
version: 1
name: test
network:
  outbound:
    allow: false
    allow_domains:
      - example.com
process:
  allow_exec_any: true
"#,
        )
        .unwrap();
        let diags = lint(&p);
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("allow_domains")));
    }

    #[test]
    fn rule_write_outside_cwd_tmpdir() {
        let p: Profile = serde_yaml::from_str(
            r#"
version: 1
name: test
filesystem:
  write:
    - /var/log
process:
  allow_exec_any: true
"#,
        )
        .unwrap();
        let diags = lint(&p);
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("/var/log")));
    }

    #[test]
    fn rule_unrestricted_outbound() {
        let p: Profile = serde_yaml::from_str(
            r#"
version: 1
name: test
network:
  outbound:
    allow: true
process:
  allow_exec_any: true
"#,
        )
        .unwrap();
        let diags = lint(&p);
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("unrestricted")));
    }

    #[test]
    fn rule_no_exec_permissions() {
        let p: Profile = serde_yaml::from_str(
            r#"
version: 1
name: test
"#,
        )
        .unwrap();
        let diags = lint(&p);
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("no exec")));
    }

    #[test]
    fn rule_no_name() {
        let p: Profile = serde_yaml::from_str(
            r#"
version: 1
process:
  allow_exec_any: true
"#,
        )
        .unwrap();
        let diags = lint(&p);
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Info && d.message.contains("no name")));
    }

    #[test]
    fn safe_write_paths_not_flagged() {
        assert!(is_safe_write_path("(cwd)/output"));
        assert!(is_safe_write_path("(tmpdir)/cache"));
        assert!(is_safe_write_path("/tmp/work"));
        assert!(is_safe_write_path("/private/tmp/work"));
        assert!(!is_safe_write_path("/var/log"));
        assert!(!is_safe_write_path("/usr/local"));
    }
}
