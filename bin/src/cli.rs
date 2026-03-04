use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "seatbelt", about = "sandbox-exec with human ergonomics")]
#[command(version, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run a command under a sandbox profile
    Run(RunArgs),

    /// Observe a command and generate a sandbox profile from its behavior
    Generate(GenerateArgs),

    /// Explain sandbox violations in plain English
    Explain(ExplainArgs),

    /// Lint and validate a profile file
    Check(CheckArgs),

    /// Compile a YAML profile to SBPL
    Compile(CompileArgs),

    /// Passthrough: any unrecognized subcommand is treated as the command to sandbox.
    /// Only works when a default profile is resolvable.
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Args)]
pub struct RunArgs {
    /// Path to a YAML profile file
    #[arg(long, conflicts_with = "preset")]
    pub profile: Option<PathBuf>,

    /// Use a built-in named preset
    #[arg(long, conflicts_with = "profile")]
    pub preset: Option<String>,

    /// Print generated SBPL and exit without running
    #[arg(long)]
    pub dry_run: bool,

    /// Show detailed per-violation explanations after the process exits
    #[arg(long)]
    pub explain: bool,

    /// Stream violation events to stderr in real time
    #[arg(long, short)]
    pub verbose: bool,

    /// The command to run and its arguments
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Args)]
pub struct GenerateArgs {
    /// Write generated profile to this path (default: stdout)
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Start from a preset, only emit additional rules
    #[arg(long)]
    pub base_preset: Option<String>,

    /// Output format
    #[arg(long, default_value = "yaml", value_parser = ["yaml", "sbpl"])]
    pub format: String,

    /// Run the command this many times, union the access patterns
    #[arg(long, default_value = "1")]
    pub runs: u32,

    /// The command to observe
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Args)]
pub struct ExplainArgs {
    /// Show violations for this PID (default: most recent seatbelt run)
    #[arg(long)]
    pub pid: Option<u32>,

    /// Read violations from this log file instead of the system log
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Show all violation types, not just file access
    #[arg(long)]
    pub all: bool,
}

#[derive(Args)]
pub struct CheckArgs {
    /// Profile file to check (YAML)
    pub profile: PathBuf,

    /// Check an SBPL file instead of YAML
    #[arg(long)]
    pub sbpl: bool,

    /// Treat warnings as errors
    #[arg(long)]
    pub strict: bool,
}

#[derive(Args)]
pub struct CompileArgs {
    /// YAML profile to compile
    pub profile: PathBuf,

    /// Write SBPL to this file (default: stdout)
    #[arg(long, short)]
    pub output: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_run_with_profile() {
        let cli = Cli::parse_from([
            "seatbelt",
            "run",
            "--profile",
            "test.yaml",
            "--",
            "echo",
            "hi",
        ]);
        match cli.command {
            Command::Run(args) => {
                assert_eq!(args.profile, Some(PathBuf::from("test.yaml")));
                assert!(args.preset.is_none());
                assert_eq!(args.command, vec!["echo", "hi"]);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_run_with_preset() {
        let cli = Cli::parse_from([
            "seatbelt",
            "run",
            "--preset",
            "ai-agent-strict",
            "--",
            "bash",
        ]);
        match cli.command {
            Command::Run(args) => {
                assert_eq!(args.preset, Some("ai-agent-strict".into()));
                assert!(args.profile.is_none());
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_run_dry_run() {
        let cli = Cli::parse_from([
            "seatbelt",
            "run",
            "--dry-run",
            "--preset",
            "read-only",
            "--",
            "ls",
        ]);
        match cli.command {
            Command::Run(args) => assert!(args.dry_run),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_compile() {
        let cli = Cli::parse_from(["seatbelt", "compile", "profile.yaml"]);
        match cli.command {
            Command::Compile(args) => {
                assert_eq!(args.profile, PathBuf::from("profile.yaml"));
                assert!(args.output.is_none());
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn parse_compile_with_output() {
        let cli = Cli::parse_from(["seatbelt", "compile", "--output", "out.sb", "profile.yaml"]);
        match cli.command {
            Command::Compile(args) => {
                assert_eq!(args.output, Some(PathBuf::from("out.sb")));
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn profile_and_preset_conflict() {
        let result = Cli::try_parse_from([
            "seatbelt",
            "run",
            "--profile",
            "x.yaml",
            "--preset",
            "y",
            "--",
            "echo",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_external_subcommand() {
        let cli = Cli::parse_from(["seatbelt", "echo", "hello"]);
        match cli.command {
            Command::External(args) => {
                assert_eq!(args, vec!["echo", "hello"]);
            }
            _ => panic!("expected External"),
        }
    }
}
