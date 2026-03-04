use crate::log_stream::Violation;

/// A human-readable explanation for a sandbox violation.
#[derive(Debug, Clone)]
pub struct Explanation {
    pub headline: String,
    pub context: String,
    pub yaml_fix: Option<String>,
}

/// Classify a file path into a semantic category for tailored explanations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathClass {
    SshKey,
    AwsCredentials,
    GcloudCredentials,
    HomeConfig,
    SystemLib,
    TmpDir,
    HomeBrew,
    Network,
    MachService,
    Unknown,
}

/// Produce a human-readable explanation for a sandbox violation.
pub fn explain_violation(v: &Violation) -> Explanation {
    let class = classify_path(&v.operation, &v.path);

    match class {
        PathClass::SshKey => Explanation {
            headline: format!("Blocked read of SSH key: {}", v.path),
            context: "SSH keys are sensitive credentials. Sandboxed processes should not have access to your SSH private keys.".into(),
            yaml_fix: Some("If this process genuinely needs SSH access, remove the path from `filesystem.deny` — but consider the security implications.".into()),
        },

        PathClass::AwsCredentials => Explanation {
            headline: format!("Blocked read of AWS credentials: {}", v.path),
            context: "AWS credentials contain secret access keys. The sandbox is protecting these from exfiltration.".into(),
            yaml_fix: Some("Add to `filesystem.read` if the process needs AWS API access:\n  filesystem:\n    read:\n      - (home)/.aws".into()),
        },

        PathClass::GcloudCredentials => Explanation {
            headline: format!("Blocked read of Google Cloud credentials: {}", v.path),
            context: "GCloud credentials contain OAuth tokens and service account keys.".into(),
            yaml_fix: Some("Add to `filesystem.read` if the process needs GCloud access:\n  filesystem:\n    read:\n      - (home)/.config/gcloud".into()),
        },

        PathClass::HomeConfig => Explanation {
            headline: format!("Blocked access to config file: {}", v.path),
            context: "The process tried to read a configuration file in your home directory.".into(),
            yaml_fix: Some(format!("Add to `filesystem.read`:\n  filesystem:\n    read:\n      - {}", generalize_home_path(&v.path))),
        },

        PathClass::SystemLib => Explanation {
            headline: format!("Blocked read of system library: {}", v.path),
            context: "System libraries are normally allowed by the baseline rules. This may indicate a non-standard library path.".into(),
            yaml_fix: Some(format!("Add to `filesystem.read`:\n  filesystem:\n    read:\n      - {}", v.path)),
        },

        PathClass::TmpDir => Explanation {
            headline: format!("Blocked write to temp directory: {}", v.path),
            context: "Temp directories are commonly needed. This path should probably be allowed.".into(),
            yaml_fix: Some("Add `(tmpdir)` to `filesystem.write`:\n  filesystem:\n    write:\n      - (tmpdir)".into()),
        },

        PathClass::HomeBrew => Explanation {
            headline: format!("Blocked access to Homebrew path: {}", v.path),
            context: "Homebrew-installed tools and libraries live under /opt/homebrew or /usr/local.".into(),
            yaml_fix: Some("Add Homebrew path to `filesystem.read`:\n  filesystem:\n    read:\n      - /opt/homebrew".into()),
        },

        PathClass::Network => Explanation {
            headline: format!("Blocked network access: {} {}", v.operation, v.path),
            context: "The process tried to make a network connection.".into(),
            yaml_fix: Some("Enable outbound networking:\n  network:\n    outbound:\n      allow: true".into()),
        },

        PathClass::MachService => Explanation {
            headline: format!("Blocked Mach service lookup: {}", v.path),
            context: "Mach services are macOS IPC endpoints. Some are needed for basic functionality.".into(),
            yaml_fix: Some(format!("Add to `system.allow_mach_lookup`:\n  system:\n    allow_mach_lookup:\n      - {}", v.path)),
        },

        PathClass::Unknown => Explanation {
            headline: format!("Blocked {}: {}", v.operation, v.path),
            context: format!("The process '{}' (pid {}) was denied the '{}' operation.", v.process_name, v.pid, v.operation),
            yaml_fix: None,
        },
    }
}

fn classify_path(operation: &str, path: &str) -> PathClass {
    // Network operations
    if operation.starts_with("network") {
        return PathClass::Network;
    }

    // Mach service lookups
    if operation.starts_with("mach-lookup") {
        return PathClass::MachService;
    }

    // SSH keys
    if path.contains("/.ssh/") || path.ends_with("/.ssh") {
        return PathClass::SshKey;
    }

    // AWS credentials
    if path.contains("/.aws/") || path.ends_with("/.aws") {
        return PathClass::AwsCredentials;
    }

    // GCloud credentials
    if path.contains("/.config/gcloud") {
        return PathClass::GcloudCredentials;
    }

    // Temp directories
    if path.starts_with("/tmp")
        || path.starts_with("/private/tmp")
        || path.starts_with("/var/folders")
    {
        return PathClass::TmpDir;
    }

    // Homebrew
    if path.starts_with("/opt/homebrew") || path.starts_with("/usr/local/Cellar") {
        return PathClass::HomeBrew;
    }

    // System libraries (non-standard paths not covered by baseline)
    if path.starts_with("/usr/lib")
        || path.starts_with("/System/Library")
        || path.starts_with("/Library/Apple")
    {
        return PathClass::SystemLib;
    }

    // Home directory config files
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if path.starts_with(home_str.as_ref()) && path.contains("/.") {
            return PathClass::HomeConfig;
        }
    }

    PathClass::Unknown
}

/// Replace the literal home directory prefix with `(home)` for YAML suggestions.
fn generalize_home_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if let Some(suffix) = path.strip_prefix(home_str.as_ref()) {
            return format!("(home){suffix}");
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_violation(operation: &str, path: &str) -> Violation {
        Violation {
            process_name: "test".into(),
            pid: 1234,
            operation: operation.into(),
            path: path.into(),
            raw: String::new(),
        }
    }

    #[test]
    fn explain_ssh_key() {
        let v = make_violation("file-read-data", "/Users/test/.ssh/id_rsa");
        let e = explain_violation(&v);
        assert!(e.headline.contains("SSH key"));
    }

    #[test]
    fn explain_aws_credentials() {
        let v = make_violation("file-read-data", "/Users/test/.aws/credentials");
        let e = explain_violation(&v);
        assert!(e.headline.contains("AWS"));
    }

    #[test]
    fn explain_gcloud() {
        let v = make_violation(
            "file-read-data",
            "/Users/test/.config/gcloud/credentials.db",
        );
        let e = explain_violation(&v);
        assert!(e.headline.contains("Google Cloud"));
    }

    #[test]
    fn explain_tmpdir() {
        let v = make_violation("file-write-data", "/tmp/some-file");
        let e = explain_violation(&v);
        assert!(e.headline.contains("temp directory"));
        assert!(e.yaml_fix.unwrap().contains("(tmpdir)"));
    }

    #[test]
    fn explain_homebrew() {
        let v = make_violation("file-read-data", "/opt/homebrew/lib/libfoo.dylib");
        let e = explain_violation(&v);
        assert!(e.headline.contains("Homebrew"));
    }

    #[test]
    fn explain_network() {
        let v = make_violation("network-outbound", "*:443");
        let e = explain_violation(&v);
        assert!(e.headline.contains("network"));
        assert!(e.yaml_fix.unwrap().contains("outbound"));
    }

    #[test]
    fn explain_mach_service() {
        let v = make_violation("mach-lookup", "com.apple.system.logger");
        let e = explain_violation(&v);
        assert!(e.headline.contains("Mach service"));
        assert!(e.yaml_fix.unwrap().contains("com.apple.system.logger"));
    }

    #[test]
    fn explain_system_lib() {
        let v = make_violation(
            "file-read-data",
            "/System/Library/Frameworks/Security.framework",
        );
        let e = explain_violation(&v);
        assert!(e.headline.contains("system library"));
    }

    #[test]
    fn explain_unknown() {
        let v = make_violation("ipc-posix-shm", "/some/random/path");
        let e = explain_violation(&v);
        assert!(e.headline.contains("Blocked"));
        assert!(e.yaml_fix.is_none());
    }

    #[test]
    fn classify_private_tmp() {
        assert_eq!(
            classify_path("file-write-data", "/private/tmp/foo"),
            PathClass::TmpDir
        );
    }

    #[test]
    fn classify_var_folders() {
        assert_eq!(
            classify_path("file-write-data", "/var/folders/ab/cd1234/T/tmp.xxx"),
            PathClass::TmpDir
        );
    }
}
