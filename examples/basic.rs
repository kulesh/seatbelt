use seatbelt_lib::presets;
use seatbelt_lib::profile::{compiler, loader, resolver};
use std::path::PathBuf;

fn main() {
    // Load a built-in preset
    let yaml = presets::get_preset("ai-agent-strict").expect("preset exists");
    let profile = loader::load_profile_from_str(yaml).expect("valid profile");

    // Resolve magic variables
    let cwd = std::env::current_dir().expect("cwd");
    let home = dirs::home_dir().expect("home");
    let resolved = resolver::resolve(&profile, &cwd, &home).expect("resolution");

    // Compile to SBPL
    let binary = Some("/usr/bin/echo");
    let sbpl = compiler::compile(&resolved, binary).expect("compilation");

    println!("Generated SBPL ({} bytes):\n", sbpl.len());
    println!("{sbpl}");
}
