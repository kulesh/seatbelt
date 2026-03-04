mod cli;
mod runner;

use anyhow::{Context, Result};
use clap::Parser;
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
        Command::Check(_) => {
            eprintln!("seatbelt check: not yet implemented (Phase 2)");
            std::process::exit(1);
        }
    }
}
