use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SeatbeltError {
    #[error("Profile file not found: {0}")]
    ProfileNotFound(PathBuf),

    #[error("Invalid profile YAML: {0}")]
    ProfileParseError(#[from] serde_yaml::Error),

    #[error("Unknown preset '{0}'. Available presets: ai-agent-strict, ai-agent-networked, ai-agent-permissive, read-only, build-tool, network-only")]
    UnknownPreset(String),

    #[error("{0}")]
    NoProfileFound(String),

    #[error("sandbox-exec not found. This tool requires macOS.")]
    SandboxExecNotFound,

    #[error("Profile has {0} lint error(s)")]
    LintErrors(usize),

    #[error("Profile compilation failed: {0}")]
    CompilationError(String),

    #[error("Log stream error: {0}")]
    LogStreamError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SeatbeltError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn display_profile_not_found() {
        let err = SeatbeltError::ProfileNotFound(PathBuf::from("/tmp/missing.yaml"));
        assert_eq!(err.to_string(), "Profile file not found: /tmp/missing.yaml");
    }

    #[test]
    fn display_unknown_preset() {
        let err = SeatbeltError::UnknownPreset("nope".into());
        assert!(err.to_string().contains("nope"));
        assert!(err.to_string().contains("ai-agent-strict"));
    }

    #[test]
    fn display_sandbox_exec_not_found() {
        let err = SeatbeltError::SandboxExecNotFound;
        assert!(err.to_string().contains("sandbox-exec"));
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err: SeatbeltError = io_err.into();
        assert!(matches!(err, SeatbeltError::Io(_)));
    }
}
