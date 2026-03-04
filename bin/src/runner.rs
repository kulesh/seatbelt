use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use seatbelt_lib::presets;
use seatbelt_lib::profile::{compiler, default, loader, resolver};

use crate::cli::RunArgs;

/// Execute `seatbelt run` — the primary command.
pub fn run(args: &RunArgs) -> Result<()> {
    let profile = load_profile_or_preset(&args.profile, &args.preset)?;

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

    let status = std::process::Command::new(sandbox_exec)
        .args(["-f", &tmp.path().to_string_lossy()])
        .arg("--")
        .args(&args.command)
        .status()
        .context("failed to spawn sandbox-exec")?;

    std::process::exit(status.code().unwrap_or(1));
}

/// Execute `seatbelt <external>` — find default profile, then run.
pub fn run_external(args: &[String]) -> Result<()> {
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
    run(&run_args)
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
        // echo should be findable on any system
        let result = resolve_binary("echo");
        assert!(result.is_some());
        assert!(result.unwrap().starts_with('/'));
    }

    #[test]
    fn resolve_nonexistent() {
        let result = resolve_binary("definitely_not_a_real_binary_xyz");
        assert!(result.is_none());
    }
}
