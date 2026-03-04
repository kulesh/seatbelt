use std::path::Path;

use crate::error::Result;
use crate::profile::schema::Profile;

/// A path with its magic variables expanded and glob status determined.
#[derive(Debug, Clone)]
pub struct ResolvedPath {
    pub path: String,
    pub is_glob: bool,
}

/// A fully resolved profile — all magic variables expanded, paths classified.
/// The compiler accepts only this type, enforcing the parse → resolve → compile pipeline.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub filesystem_read: Vec<ResolvedPath>,
    pub filesystem_write: Vec<ResolvedPath>,
    pub filesystem_deny: Vec<ResolvedPath>,
    pub network: crate::profile::schema::NetworkRules,
    pub process: crate::profile::schema::ProcessRules,
    pub system: crate::profile::schema::SystemRules,
}

/// Resolve all magic variables in a profile's paths.
pub fn resolve(profile: &Profile, cwd: &Path, home: &Path) -> Result<ResolvedProfile> {
    let resolve_paths = |raw_paths: &[String]| -> Vec<ResolvedPath> {
        raw_paths
            .iter()
            .flat_map(|raw| resolve_path(raw, cwd, home))
            .collect()
    };

    Ok(ResolvedProfile {
        filesystem_read: resolve_paths(&profile.filesystem.read),
        filesystem_write: resolve_paths(&profile.filesystem.write),
        filesystem_deny: resolve_paths(&profile.filesystem.deny),
        network: profile.network.clone(),
        process: profile.process.clone(),
        system: profile.system.clone(),
    })
}

/// Expand a single raw path string into one or more resolved paths.
/// `(tmpdir)` expands to three paths; `(bundle ...)` is not yet implemented.
pub fn resolve_path(raw: &str, cwd: &Path, home: &Path) -> Vec<ResolvedPath> {
    let expanded = expand_variables(raw, cwd, home);
    expanded
        .into_iter()
        .map(|path| {
            let is_glob = path.contains('*') || path.contains('?');
            ResolvedPath { path, is_glob }
        })
        .collect()
}

fn expand_variables(raw: &str, cwd: &Path, home: &Path) -> Vec<String> {
    if raw == "(cwd)" {
        return vec![cwd.to_string_lossy().into_owned()];
    }
    if raw == "(home)" {
        return vec![home.to_string_lossy().into_owned()];
    }
    if raw == "(tmpdir)" {
        return tmpdir_paths();
    }
    if let Some(suffix) = raw.strip_prefix("(cwd)") {
        return vec![format!("{}{}", cwd.display(), suffix)];
    }
    if let Some(suffix) = raw.strip_prefix("(home)") {
        return vec![format!("{}{}", home.display(), suffix)];
    }
    if let Some(suffix) = raw.strip_prefix("(tmpdir)") {
        return tmpdir_paths()
            .into_iter()
            .map(|base| format!("{}{}", base, suffix))
            .collect();
    }
    if raw.starts_with("(bundle ") {
        eprintln!(
            "seatbelt: warning: (bundle ...) is not yet supported, skipping: {}",
            raw
        );
        return vec![];
    }
    if let Some(suffix) = raw.strip_prefix('~') {
        return vec![format!("{}{}", home.display(), suffix)];
    }
    vec![raw.to_string()]
}

fn tmpdir_paths() -> Vec<String> {
    let var_folders = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    // Normalize trailing slash
    let var_folders = var_folders.trim_end_matches('/').to_string();
    let mut paths = vec!["/tmp".to_string(), "/private/tmp".to_string()];
    if var_folders != "/tmp" && var_folders != "/private/tmp" {
        paths.push(var_folders);
    }
    paths
}

/// Convert a glob pattern to an SBPL-compatible regex.
/// `*` → `[^/]*` (within one path component), `?` → `[^/]` (single char, not separator).
/// Regex metacharacters in literal portions are escaped.
pub fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::with_capacity(glob.len() + 16);
    regex.push('^');
    for ch in glob.chars() {
        match ch {
            '*' => regex.push_str("[^/]*"),
            '?' => regex.push_str("[^/]"),
            '.' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
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
    fn expand_cwd() {
        let paths = resolve_path("(cwd)", &test_cwd(), &test_home());
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].path, "/Users/test/project");
        assert!(!paths[0].is_glob);
    }

    #[test]
    fn expand_cwd_suffix() {
        let paths = resolve_path("(cwd)/src", &test_cwd(), &test_home());
        assert_eq!(paths[0].path, "/Users/test/project/src");
    }

    #[test]
    fn expand_home() {
        let paths = resolve_path("(home)", &test_cwd(), &test_home());
        assert_eq!(paths[0].path, "/Users/test");
    }

    #[test]
    fn expand_home_suffix() {
        let paths = resolve_path("(home)/.ssh/config", &test_cwd(), &test_home());
        assert_eq!(paths[0].path, "/Users/test/.ssh/config");
    }

    #[test]
    fn expand_tilde() {
        let paths = resolve_path("~/.config", &test_cwd(), &test_home());
        assert_eq!(paths[0].path, "/Users/test/.config");
    }

    #[test]
    fn expand_tmpdir_produces_multiple() {
        let paths = resolve_path("(tmpdir)", &test_cwd(), &test_home());
        assert!(paths.len() >= 2);
        assert!(paths.iter().any(|p| p.path == "/tmp"));
        assert!(paths.iter().any(|p| p.path == "/private/tmp"));
    }

    #[test]
    fn expand_tmpdir_suffix() {
        let paths = resolve_path("(tmpdir)/myapp", &test_cwd(), &test_home());
        assert!(paths.iter().any(|p| p.path == "/tmp/myapp"));
        assert!(paths.iter().any(|p| p.path == "/private/tmp/myapp"));
    }

    #[test]
    fn literal_passthrough() {
        let paths = resolve_path("/usr/lib", &test_cwd(), &test_home());
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].path, "/usr/lib");
        assert!(!paths[0].is_glob);
    }

    #[test]
    fn glob_detected() {
        let paths = resolve_path("(home)/.ssh/id_*", &test_cwd(), &test_home());
        assert_eq!(paths[0].path, "/Users/test/.ssh/id_*");
        assert!(paths[0].is_glob);
    }

    #[test]
    fn question_mark_is_glob() {
        let paths = resolve_path("/tmp/file?.txt", &test_cwd(), &test_home());
        assert!(paths[0].is_glob);
    }

    #[test]
    fn bundle_returns_empty() {
        let paths = resolve_path("(bundle com.apple.Xcode)", &test_cwd(), &test_home());
        assert!(paths.is_empty());
    }

    #[test]
    fn glob_to_regex_star() {
        assert_eq!(glob_to_regex("/home/.ssh/id_*"), r"^/home/\.ssh/id_[^/]*$");
    }

    #[test]
    fn glob_to_regex_question() {
        assert_eq!(glob_to_regex("/tmp/f?le"), r"^/tmp/f[^/]le$");
    }

    #[test]
    fn glob_to_regex_dots_escaped() {
        assert_eq!(
            glob_to_regex("/usr/lib/foo.dylib"),
            r"^/usr/lib/foo\.dylib$"
        );
    }

    #[test]
    fn glob_to_regex_no_special() {
        assert_eq!(glob_to_regex("/usr/lib"), "^/usr/lib$");
    }

    #[test]
    fn full_profile_resolution() {
        let profile: Profile = serde_yaml::from_str(
            r#"
version: 1
filesystem:
  read:
    - /usr/lib
    - (cwd)
  write:
    - (cwd)
    - (tmpdir)
  deny:
    - (home)/.ssh/id_*
"#,
        )
        .unwrap();
        let resolved = resolve(&profile, &test_cwd(), &test_home()).unwrap();
        assert!(resolved
            .filesystem_read
            .iter()
            .any(|p| p.path == "/usr/lib"));
        assert!(resolved
            .filesystem_read
            .iter()
            .any(|p| p.path == "/Users/test/project"));
        assert!(resolved.filesystem_write.iter().any(|p| p.path == "/tmp"));
        assert!(resolved.filesystem_deny[0].is_glob);
    }
}
