use std::fs::OpenOptions;
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::error::SeatbeltError;

const BOOTSTRAP_PRESET: &str = "ai-agent-networked";

/// Probe the default profile locations and return the first that exists.
pub fn find_default_profile() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let candidates = default_profile_candidates(&cwd);
    candidates.into_iter().find(|p| p.exists())
}

/// Find the first existing default profile, or bootstrap a global one.
/// Returns `(path, created)` where `created` indicates whether a profile was written.
pub fn find_or_bootstrap_default_profile() -> Result<(PathBuf, bool)> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let candidates = default_profile_candidates(&cwd);
    if let Some(path) = candidates.iter().find(|p| p.exists()) {
        return Ok((path.clone(), false));
    }

    let global_path = candidates
        .last()
        .cloned()
        .unwrap_or_else(global_default_profile_path);
    let created = bootstrap_profile_at(&global_path)?;
    Ok((global_path, created))
}

/// Return the ordered list of candidate paths for default profile resolution.
pub fn default_profile_candidates(cwd: &Path) -> Vec<PathBuf> {
    vec![
        cwd.join("seatbelt.yaml"),
        cwd.join(".seatbelt.yaml"),
        global_default_profile_path(),
    ]
}

fn global_default_profile_path() -> PathBuf {
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".config")
        });
    xdg_config.join("seatbelt/profile.yaml")
}

fn bootstrap_profile_at(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    format!(
                        "refusing to write bootstrap profile to symlink: {}",
                        path.display()
                    ),
                )
                .into());
            }
            return Ok(false);
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    write_bytes_atomically(path, bootstrap_profile_contents().as_bytes())?;
    Ok(true)
}

fn bootstrap_profile_contents() -> String {
    format!(
        "version: 1\nname: default\ndescription: auto-generated global default profile\nextends: {BOOTSTRAP_PRESET}\n"
    )
}

fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("cannot determine parent for {}", path.display()),
        )
    })?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("profile");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{nanos}", std::process::id()));

    let mut opts = OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut file = opts.open(&tmp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;

    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                format!("refusing to overwrite symlink: {}", path.display()),
            ));
        }
    }

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    Ok(())
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
         seatbelt run --dry-run -- <command>  # auto-creates ~/.config/seatbelt/profile.yaml\n  \
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

    #[test]
    fn bootstrap_profile_contents_extends_networked() {
        let yaml = bootstrap_profile_contents();
        let profile: crate::profile::schema::Profile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(profile.extends.as_deref(), Some("ai-agent-networked"));
    }

    #[test]
    fn bootstrap_profile_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seatbelt/profile.yaml");
        assert!(bootstrap_profile_at(&path).unwrap());
        assert!(!bootstrap_profile_at(&path).unwrap());
    }

    #[test]
    fn bootstrap_profile_rejects_symlink_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real-profile.yaml");
        let path = dir.path().join("seatbelt/profile.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&real, "original").unwrap();
        symlink(&real, &path).unwrap();

        let err = bootstrap_profile_at(&path).unwrap_err();
        assert!(err.to_string().contains("symlink"));
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "original");
    }
}
