mod cli;
mod runner;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use seatbelt_lib::profile::linter::{self, Severity};
use seatbelt_lib::profile::{compiler, loader, resolver};

use cli::{Cli, Command};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Run(args) => runner::run(&args),

        Command::Compile(args) => {
            let profile = loader::load_profile(&args.profile)?;
            let cwd = std::env::current_dir().context("cannot determine current directory")?;
            let home = dirs::home_dir().context("cannot determine home directory")?;
            let resolved = resolver::resolve(&profile, &cwd, &home)?;
            let sbpl = compiler::compile(&resolved, None)?;

            if let Some(output) = &args.output {
                std::fs::write(output, &sbpl)
                    .with_context(|| format!("failed to write SBPL to {}", output.display()))?;
            } else {
                println!("{sbpl}");
            }
            Ok(())
        }

        Command::External(args) => runner::run_external(&args),

        Command::Generate(_) => {
            eprintln!("seatbelt generate: not yet implemented (Phase 4)");
            std::process::exit(1);
        }
        Command::Explain(_) => {
            eprintln!("seatbelt explain: not yet implemented (Phase 3)");
            std::process::exit(1);
        }
        Command::Check(args) => {
            if args.sbpl {
                eprintln!("SBPL checking not yet supported");
                std::process::exit(1);
            }

            let profile = loader::load_profile(&args.profile)?;
            let diags = linter::lint(&profile);

            let mut errors = 0usize;
            let mut warnings = 0usize;
            for d in &diags {
                let prefix = match d.severity {
                    Severity::Error => {
                        errors += 1;
                        "error".red().bold()
                    }
                    Severity::Warning => {
                        warnings += 1;
                        "warning".yellow().bold()
                    }
                    Severity::Info => "info".blue().bold(),
                };
                eprintln!("{prefix}: {}", d.message);
                if let Some(ref suggestion) = d.suggestion {
                    eprintln!("  {} {suggestion}", "hint:".dimmed());
                }
            }

            if errors > 0 || (args.strict && warnings > 0) {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}
