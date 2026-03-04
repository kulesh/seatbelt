mod cli;
mod generator;
mod runner;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use seatbelt_lib::profile::linter::{self, Severity};
use seatbelt_lib::profile::{compiler, loader, resolver};

use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Run(args) => runner::run(&args).await,

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

        Command::External(args) => runner::run_external(&args).await,

        Command::Generate(args) => generator::generate(&args).await,

        Command::Explain(args) => {
            if let Some(ref log_path) = args.log {
                runner::explain_from_log(log_path, args.all)
            } else if let Some(pid) = args.pid {
                runner::explain_from_pid(pid, args.all)
            } else {
                runner::explain_last_run(args.all)
            }
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
