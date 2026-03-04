use seatbelt_lib::presets;
use seatbelt_lib::profile::{compiler, loader, resolver};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let yaml = presets::get_preset("ai-agent-strict").unwrap();
    let cwd = PathBuf::from("/Users/test/project");
    let home = PathBuf::from("/Users/test");

    let iterations = 10_000;

    // Benchmark: load + resolve + compile
    let start = Instant::now();
    for _ in 0..iterations {
        let profile = loader::load_profile_from_str(yaml).unwrap();
        let resolved = resolver::resolve(&profile, &cwd, &home).unwrap();
        let _sbpl = compiler::compile(&resolved, Some("/usr/bin/echo")).unwrap();
    }
    let duration = start.elapsed();

    println!(
        "load+resolve+compile: {} iterations in {:?} ({:?}/iter)",
        iterations,
        duration,
        duration / iterations
    );

    // Benchmark: compile only (profile already resolved)
    let profile = loader::load_profile_from_str(yaml).unwrap();
    let resolved = resolver::resolve(&profile, &cwd, &home).unwrap();

    let start = Instant::now();
    for _ in 0..iterations {
        let _sbpl = compiler::compile(&resolved, Some("/usr/bin/echo")).unwrap();
    }
    let duration = start.elapsed();

    println!(
        "compile only: {} iterations in {:?} ({:?}/iter)",
        iterations,
        duration,
        duration / iterations
    );
}
