use std::path::{Path, PathBuf};

use crate::error::SeatbeltError;

/// Probe the default profile locations and return the first that exists.
pub fn find_default_profile() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let candidates = default_profile_candidates(&cwd);
    candidates.into_iter().find(|p| p.exists())
}

/// Return the ordered list of candidate paths for default profile resolution.
pub fn default_profile_candidates(cwd: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![cwd.join("seatbelt.yaml"), cwd.join(".seatbelt.yaml")];

    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("cannot determine home directory")
                .join(".config")
        });
    candidates.push(xdg_config.join("seatbelt/profile.yaml"));

    candidates
}

/// Construct an actionable error message listing all paths that were checked.
pub fn no_profile_error(cwd: &Path) -> SeatbeltError {
    let candidates = default_profile_candidates(cwd);
    let paths_listed = candidates
        .iter()
        .map(|p| format!("  - {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");

    SeatbeltError::NoProfileFound(format!(
        "No profile specified and no default profile found.\n\
         Looked in:\n{paths_listed}\n\n\
         Options:\n  \
         seatbelt run --profile <path> -- <command>\n  \
         seatbelt run --preset ai-agent-strict -- <command>\n  \
         Create a seatbelt.yaml in the current directory"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_ordering() {
        let cwd = Path::new("/Users/test/project");
        let candidates = default_profile_candidates(cwd);
        assert_eq!(
            candidates[0],
            PathBuf::from("/Users/test/project/seatbelt.yaml")
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("/Users/test/project/.seatbelt.yaml")
        );
        assert!(candidates[2].ends_with("seatbelt/profile.yaml"));
    }

    #[test]
    fn error_message_content() {
        let cwd = Path::new("/Users/test/project");
        let err = no_profile_error(cwd);
        let msg = err.to_string();
        assert!(msg.contains("seatbelt.yaml"));
        assert!(msg.contains(".seatbelt.yaml"));
        assert!(msg.contains("--profile"));
        assert!(msg.contains("--preset"));
    }

    #[test]
    fn find_default_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let profile_path = dir.path().join("seatbelt.yaml");
        std::fs::write(&profile_path, "version: 1\n").unwrap();

        // We can't easily test find_default_profile() because it uses env::current_dir,
        // but we can verify the candidate list includes the right paths.
        let candidates = default_profile_candidates(dir.path());
        assert!(candidates.contains(&profile_path));
    }
}
