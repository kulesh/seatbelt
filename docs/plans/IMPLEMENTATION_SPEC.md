# seatbelt — Implementation Specification

## Language & Toolchain

- **Language:** Rust (stable, current edition 2021)
- **Minimum macOS:** 13 Ventura (required for modern `log stream` predicate format)
- **Targets:** `aarch64-apple-darwin`, `x86_64-apple-darwin`
- **Build:** standard `cargo build --release`

This is macOS-only. Do not add `#[cfg(target_os = "macos")]` guards everywhere — the entire codebase is macOS-only. The `Cargo.toml` should set `[target.'cfg(target_os = "macos")']` for any OS-conditional deps and the README should state the platform requirement.

---

## Dependency Manifest

### Workspace root `Cargo.toml`

```toml
[workspace]
members = ["lib", "bin"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
# Error handling
thiserror = "1"
anyhow = "1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"

# Async runtime (for concurrent process + log stream)
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Terminal output
colored = "2"

# Filesystem utilities
dirs = "5"

# Dev dependencies
tempfile = "3"
assert_cmd = "2"
predicates = "3"

[profile.release]
strip = true
lto = true
codegen-units = 1
```

### `lib/Cargo.toml`

```toml
[package]
name = "seatbelt-lib"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
serde.workspace = true
serde_yaml.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-stream.workspace = true
colored.workspace = true
dirs.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

### `bin/Cargo.toml`

```toml
[package]
name = "seatbelt-bin"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "seatbelt"
path = "src/main.rs"

[dependencies]
seatbelt-lib = { path = "../lib" }
anyhow.workspace = true
clap.workspace = true
tokio.workspace = true
colored.workspace = true
tempfile.workspace = true

[dev-dependencies]
assert_cmd.workspace = true
predicates.workspace = true
```

---

## Project Structure

```
seatbelt/
├── Cargo.toml                  # workspace configuration
├── lib/
│   ├── Cargo.toml              # seatbelt-lib
│   └── src/
│       ├── lib.rs              # re-exports public API
│       ├── error.rs            # SeatbeltError enum (thiserror)
│       ├── profile/
│       │   ├── mod.rs          # re-exports
│       │   ├── schema.rs       # Profile serde structs
│       │   ├── loader.rs       # load YAML, resolve extends (YAML-level merge)
│       │   ├── resolver.rs     # expand magic variables + detect glob patterns
│       │   ├── compiler.rs     # Profile → SBPL string
│       │   ├── linter.rs       # lint rules → Vec<LintDiagnostic>
│       │   └── default.rs      # default profile discovery chain
│       ├── presets/
│       │   ├── mod.rs          # preset registry (include_str! embedded YAMLs)
│       │   └── profiles/       # built-in YAML files embedded at compile time
│       │       ├── ai-agent-strict.yaml
│       │       ├── ai-agent-networked.yaml
│       │       ├── ai-agent-permissive.yaml
│       │       ├── read-only.yaml
│       │       ├── build-tool.yaml
│       │       └── network-only.yaml
│       ├── explainer.rs        # violation → human-readable explanation
│       ├── log_stream.rs       # spawn + parse macOS `log stream` output
│       └── sbpl/
│           ├── mod.rs
│           └── ops.rs          # violation string → SBPL operation mapping
├── bin/
│   ├── Cargo.toml              # seatbelt-bin
│   └── src/
│       ├── main.rs             # entry point, command dispatch
│       ├── cli.rs              # clap derive structs
│       ├── runner.rs           # seatbelt run: orchestrate sandbox-exec
│       └── generator.rs        # seatbelt generate: observe + emit profile
├── tests/
│   ├── integration/
│   │   ├── run_test.rs
│   │   ├── generate_test.rs
│   │   └── compile_test.rs
│   └── fixtures/
│       ├── hello.py
│       ├── read_home.py
│       └── profiles/
├── docs/
│   ├── specs/PRODUCT_SPEC.md
│   ├── plans/IMPLEMENTATION_SPEC.md
│   └── adrs/
└── README.md
```

**Crate boundary principle:** `seatbelt-lib` contains all reusable logic (profile handling, SBPL compilation, violation parsing, explanations). `seatbelt-bin` contains CLI argument parsing and command orchestration (spawning processes, managing temp files, coordinating I/O).

---

## Module Specifications

### `bin/src/cli.rs`

Define the full CLI surface using `clap` derive macros.

```rust
use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

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
```

---

### `lib/src/error.rs`

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SeatbeltError {
    #[error("Profile file not found: {0}")]
    ProfileNotFound(PathBuf),

    #[error("Invalid profile YAML: {0}")]
    ProfileParseError(#[from] serde_yaml::Error),

    #[error("Unknown preset '{0}'. Run `seatbelt run --list-presets` to see available presets.")]
    UnknownPreset(String),

    #[error("{0}")]
    NoProfileFound(String),

    #[error("sandbox-exec not found. This tool requires macOS.")]
    SandboxExecNotFound,

    #[error("Profile compilation failed: {0}")]
    CompilationError(String),

    #[error("Log stream failed to start: {0}")]
    LogStreamError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Lint failed with {0} error(s). Run `seatbelt check` for details.")]
    LintErrors(usize),
}

pub type Result<T> = std::result::Result<T, SeatbeltError>;
```

---

### `lib/src/profile/schema.rs`

The full serde-deserializable representation of a YAML profile. All fields optional with `#[serde(default)]` except `version`.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u8,
    pub name: Option<String>,
    pub description: Option<String>,

    /// Inherit rules from a named preset, then apply overrides
    pub extends: Option<String>,

    #[serde(default)]
    pub filesystem: FilesystemRules,

    #[serde(default)]
    pub network: NetworkRules,

    #[serde(default)]
    pub process: ProcessRules,

    #[serde(default)]
    pub system: SystemRules,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemRules {
    #[serde(default)]
    pub read: Vec<String>,

    #[serde(default)]
    pub write: Vec<String>,

    /// Explicitly denied paths (takes priority via last-rule-wins)
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkRules {
    #[serde(default)]
    pub outbound: OutboundNetworkRules,

    #[serde(default)]
    pub inbound: InboundNetworkRules,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundNetworkRules {
    #[serde(default)]
    pub allow: bool,

    /// Domain filter (requires proxy — out of scope for v1)
    #[serde(default)]
    pub allow_domains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundNetworkRules {
    #[serde(default)]
    pub allow: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRules {
    /// Specific executables this process is allowed to exec
    #[serde(default)]
    pub allow_exec: Vec<String>,

    /// Allow unrestricted exec (overrides allow_exec)
    #[serde(default)]
    pub allow_exec_any: bool,

    /// Allow forking child processes
    #[serde(default = "default_true")]
    pub allow_fork: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemRules {
    #[serde(default = "default_true")]
    pub allow_sysctl_read: bool,

    #[serde(default)]
    pub allow_sysctl_write: bool,

    /// XPC/Mach services this process is allowed to look up
    #[serde(default)]
    pub allow_mach_lookup: Vec<String>,
}

fn default_true() -> bool { true }
```

Note: `#[serde(deny_unknown_fields)]` on all structs catches typos and unknown keys at deserialization time, as required by `seatbelt check`.

---

### `lib/src/profile/loader.rs`

Loads a YAML profile from disk, resolving the `extends` chain via YAML-level deep merge.

```rust
use serde_yaml::Value;

/// Load a profile from a YAML file path, resolving `extends` if present.
pub fn load_profile(path: &Path) -> Result<Profile> {
    let raw = std::fs::read_to_string(path)
        .map_err(|_| SeatbeltError::ProfileNotFound(path.to_path_buf()))?;
    load_profile_from_str(&raw)
}

/// Load a profile from a YAML string, resolving `extends` if present.
pub fn load_profile_from_str(yaml: &str) -> Result<Profile> {
    let child_value: Value = serde_yaml::from_str(yaml)?;

    let merged = if let Some(Value::String(preset_name)) = child_value.get("extends") {
        let parent_yaml = presets::get_preset(preset_name)
            .ok_or_else(|| SeatbeltError::UnknownPreset(preset_name.clone()))?;
        let parent_value: Value = serde_yaml::from_str(parent_yaml)?;
        deep_merge_yaml(parent_value, child_value)
    } else {
        child_value
    };

    let profile: Profile = serde_yaml::from_value(merged)?;
    Ok(profile)
}

/// Deep merge two YAML Values. Child values override parent at leaf level.
/// For mappings: recursively merge. For all other types (scalars, sequences):
/// child replaces parent entirely.
fn deep_merge_yaml(parent: Value, child: Value) -> Value {
    match (parent, child) {
        (Value::Mapping(mut parent_map), Value::Mapping(child_map)) => {
            for (key, child_val) in child_map {
                let merged_val = if let Some(parent_val) = parent_map.remove(&key) {
                    deep_merge_yaml(parent_val, child_val)
                } else {
                    child_val
                };
                parent_map.insert(key, merged_val);
            }
            Value::Mapping(parent_map)
        }
        // For sequences (lists) and scalars: child replaces parent
        (_parent, child) => child,
    }
}
```

**Merge semantics (matches product spec):**
- Scalars (booleans, strings, numbers): child value replaces parent value
- Lists (read paths, deny paths, allow_exec, etc.): child list replaces parent list entirely
- Maps: recursively merged — only keys present in the child are overridden
- Absent keys: parent value is preserved

This means `extends: ai-agent-strict` with only `network: outbound: allow: true` inherits all of the parent's filesystem rules unchanged but overrides the network configuration.

---

### `lib/src/profile/resolver.rs`

Expands magic variables in path strings and detects glob patterns. Called after deserialization, before compilation.

```rust
/// A resolved path with metadata about whether it contains glob patterns.
pub struct ResolvedPath {
    pub path: String,
    pub is_glob: bool,
}

/// Magic variables supported in path strings:
/// (cwd)         → std::env::current_dir()
/// (cwd)/suffix  → cwd joined with suffix
/// (home)        → dirs::home_dir()
/// (home)/suffix → home joined with suffix
/// (tmpdir)      → /tmp, /private/tmp, and $TMPDIR
/// (bundle <id>) → /Applications/<name>.app resolved by mdfind
/// ~/suffix      → home joined with suffix
pub fn resolve_path(raw: &str, cwd: &Path, home: &Path) -> Vec<ResolvedPath> {
    let expanded = expand_variables(raw, cwd, home);
    expanded.into_iter().map(|path| {
        let is_glob = path.contains('*') || path.contains('?');
        ResolvedPath { path, is_glob }
    }).collect()
}

fn expand_variables(raw: &str, cwd: &Path, home: &Path) -> Vec<String> {
    match raw {
        "(cwd)" => vec![cwd.to_string_lossy().into()],
        "(home)" => vec![home.to_string_lossy().into()],
        "(tmpdir)" => vec![
            "/tmp".to_string(),
            "/private/tmp".to_string(),
            get_var_folders_temp(),
        ],
        s if s.starts_with("(cwd)") => {
            let suffix = &s["(cwd)".len()..];
            vec![format!("{}{}", cwd.display(), suffix)]
        }
        s if s.starts_with("(home)") => {
            let suffix = &s["(home)".len()..];
            vec![format!("{}{}", home.display(), suffix)]
        }
        s if s.starts_with("(bundle ") => {
            resolve_bundle_path(s)
        }
        s if s.starts_with("~") => {
            vec![s.replacen("~", &home.to_string_lossy(), 1)]
        }
        s => vec![s.to_string()],
    }
}

/// Convert a shell glob pattern to an SBPL-compatible regex string.
/// * → [^/]* (match within one path component)
/// ? → [^/]  (single character, not separator)
/// All regex-special characters in the literal portions are escaped.
pub fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::from("^");
    for ch in glob.chars() {
        match ch {
            '*' => regex.push_str("[^/]*"),
            '?' => regex.push_str("[^/]"),
            '.' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
}

fn get_var_folders_temp() -> String {
    std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string())
}

fn resolve_bundle_path(s: &str) -> Vec<String> {
    // parse "(bundle com.apple.Xcode)" → run mdfind → return path
    // fall back to empty vec if not found
    todo!()
}
```

---

### `lib/src/profile/compiler.rs`

Converts a resolved `Profile` into an SBPL string. This is the core transformation.

**SBPL generation rules:**

The generated profile always follows this structure:
1. `(version 1)`
2. `(deny default)` — deny-by-default is non-negotiable
3. Symlink attack prevention
4. Baseline read rules (system libraries, dyld cache)
5. Filesystem read rules
6. Filesystem write rules (emit both `file-read*` and `file-write*`)
7. Explicit deny rules (after allows — last-rule-wins)
8. Network rules
9. Process rules
10. System/Mach rules

**Critical SBPL knowledge to encode:**

- Use `(subpath "/foo")` for recursive directory match, `(literal "/foo/bar")` for exact match, `(regex #"^/foo/.*$"#)` for patterns
- `file-write*` does NOT imply `file-read*`. Always emit both for write paths.
- `(deny sysctl-write)` requires an explicit `(allow sysctl-read)` even under `(deny default)` — this is a known kernel bug. Always emit `(allow sysctl-read)` when `allow_sysctl_read` is true.
- Mach lookup: `(allow mach-lookup (global-name "com.apple.foo"))`

```rust
use crate::profile::resolver::ResolvedPath;

/// Maximum SBPL profile size (kernel limit)
const SBPL_MAX_SIZE: usize = 65_535;
/// Warning threshold
const SBPL_WARN_SIZE: usize = 50_000;

pub fn compile(profile: &Profile, command_binary: Option<&str>) -> Result<String> {
    let mut rules: Vec<String> = Vec::new();

    rules.push("(version 1)".to_string());
    rules.push("(deny default)".to_string());
    rules.push("(deny file-write-create (vnode-type SYMLINK))".to_string());

    // Baseline read rules every process needs
    emit_baseline_rules(&mut rules);

    // Filesystem
    for rp in &profile.resolved_filesystem_write {
        emit_path_rule(&mut rules, "allow", "file-read*", rp);
        emit_path_rule(&mut rules, "allow", "file-write*", rp);
    }
    for rp in &profile.resolved_filesystem_read {
        emit_path_rule(&mut rules, "allow", "file-read*", rp);
    }
    // Explicit denies go last (last-rule-wins)
    for rp in &profile.resolved_filesystem_deny {
        emit_path_rule(&mut rules, "deny", "file-read*", rp);
        emit_path_rule(&mut rules, "deny", "file-write*", rp);
    }

    // Network
    if profile.network.outbound.allow {
        rules.push("(allow network-outbound)".to_string());
    }
    if profile.network.inbound.allow {
        rules.push("(allow network-inbound)".to_string());
    }

    // Process — no baseline exec; all exec permissions come from the profile
    if profile.process.allow_fork {
        rules.push("(allow process-fork)".to_string());
    }
    // Auto-add exec permission for the sandboxed command's own binary
    if let Some(binary) = command_binary {
        rules.push(format!(r#"(allow process-exec (literal "{}"))"#, binary));
    }
    if profile.process.allow_exec_any {
        rules.push("(allow process-exec)".to_string());
    } else {
        for exec in &profile.process.allow_exec {
            emit_path_rule_raw(&mut rules, "allow", "process-exec", exec);
        }
    }

    // System
    if profile.system.allow_sysctl_read {
        rules.push("(allow sysctl-read)".to_string());
    }
    if profile.system.allow_sysctl_write {
        rules.push("(allow sysctl-write)".to_string());
    }
    for service in &profile.system.allow_mach_lookup {
        rules.push(format!(r#"(allow mach-lookup (global-name "{}"))"#, service));
    }

    let sbpl = rules.join("\n");

    // Size check
    if sbpl.len() > SBPL_MAX_SIZE {
        return Err(SeatbeltError::CompilationError(
            format!("Generated SBPL is {} bytes, exceeding the {} byte kernel limit",
                    sbpl.len(), SBPL_MAX_SIZE)
        ));
    }
    if sbpl.len() > SBPL_WARN_SIZE {
        eprintln!("seatbelt: warning: generated SBPL is {} bytes (limit: {})", sbpl.len(), SBPL_MAX_SIZE);
    }

    Ok(sbpl)
}

/// Emit a rule using the correct SBPL matcher for the resolved path.
/// Glob paths → (regex ...), literal paths → (subpath ...).
fn emit_path_rule(rules: &mut Vec<String>, action: &str, operation: &str, rp: &ResolvedPath) {
    if rp.is_glob {
        let regex = resolver::glob_to_regex(&rp.path);
        rules.push(format!(r#"({} {} (regex #"{}"#))"#, action, operation, regex));
    } else {
        rules.push(format!(r#"({} {} (subpath "{}"))"#, action, operation, rp.path));
    }
}

/// Emit a rule for a raw path string (used for allow_exec entries).
/// Detects globs inline.
fn emit_path_rule_raw(rules: &mut Vec<String>, action: &str, operation: &str, path: &str) {
    if path.contains('*') || path.contains('?') {
        let regex = resolver::glob_to_regex(path);
        rules.push(format!(r#"({} {} (regex #"{}"#))"#, action, operation, regex));
    } else {
        rules.push(format!(r#"({} {} (literal "{}"))"#, action, operation, path));
    }
}

fn emit_baseline_rules(rules: &mut Vec<String>) {
    // Read-only access to system libraries and caches — required by virtually every process
    let baseline = [
        // System libraries
        r#"(allow file-read* (subpath "/usr/lib"))"#,
        r#"(allow file-read* (subpath "/usr/share"))"#,
        r#"(allow file-read* (subpath "/System/Library"))"#,
        r#"(allow file-read* (subpath "/Library/Apple"))"#,
        // Dyld shared cache
        r#"(allow file-read* (subpath "/private/var/db/dyld"))"#,
        // Metadata for path traversal
        r#"(allow file-read-metadata (literal "/"))"#,
        r#"(allow file-read-metadata (literal "/usr"))"#,
        r#"(allow file-read-metadata (literal "/var"))"#,
        // Rosetta 2 (harmless on native arm64, required for x86_64 under translation)
        r#"(allow process-exec (subpath "/Library/Apple/usr/libexec/oah"))"#,
        // Self-signaling
        r#"(allow signal (target self))"#,
    ];
    rules.extend(baseline.iter().map(|s| s.to_string()));

    // NOTE: No broad process-exec rules in baseline.
    // Exec permissions are controlled entirely by the profile's process section.
    // The runner auto-adds exec permission for the sandboxed command's own binary.

    // NOTE: sysctl-read is NOT in baseline — it comes from the profile's
    // system.allow_sysctl_read field (defaults to true).
}
```

---

### `lib/src/profile/linter.rs`

Returns a list of diagnostics. Checks are run on the resolved `Profile` struct (after variable expansion, before compilation).

```rust
#[derive(Debug)]
pub enum Severity { Error, Warning, Info }

#[derive(Debug)]
pub struct LintDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub suggestion: Option<String>,
}

pub fn lint(profile: &Profile) -> Vec<LintDiagnostic> {
    let mut diags = Vec::new();

    // ERROR: version must be 1
    if profile.version != 1 {
        diags.push(LintDiagnostic {
            severity: Severity::Error,
            message: format!("version must be 1, got {}", profile.version),
            suggestion: Some("Set `version: 1`".to_string()),
        });
    }

    // WARNING: write paths that are not under cwd or tmpdir are suspicious
    for path in &profile.filesystem.write {
        if !path.contains("/tmp") && !path.contains("TMPDIR") && !is_project_like(path) {
            diags.push(LintDiagnostic {
                severity: Severity::Warning,
                message: format!("Write access to '{}' is broad", path),
                suggestion: Some("Consider restricting to (cwd) or (tmpdir)".to_string()),
            });
        }
    }

    // WARNING: outbound network allowed without proxy-level domain controls
    if profile.network.outbound.allow {
        diags.push(LintDiagnostic {
            severity: Severity::Warning,
            message: "Outbound network is unrestricted".to_string(),
            suggestion: Some("If you need domain restrictions, route traffic through an external proxy".to_string()),
        });
    }

    // ERROR: allow_domains is reserved and not supported in v1
    if !profile.network.outbound.allow_domains.is_empty() {
        diags.push(LintDiagnostic {
            severity: Severity::Error,
            message: "allow_domains is not supported in v1".to_string(),
            suggestion: Some("Use outbound.allow as a coarse switch; enforce domain limits via an external proxy".to_string()),
        });
    }

    // WARNING: no exec permissions and allow_exec_any is false
    if !profile.process.allow_exec_any && profile.process.allow_exec.is_empty() {
        diags.push(LintDiagnostic {
            severity: Severity::Warning,
            message: "No exec permissions configured. The sandboxed process can only run its own binary.".to_string(),
            suggestion: Some("Set `process: allow_exec_any: true` or list specific binaries in `allow_exec`".to_string()),
        });
    }

    // INFO: no name or description
    if profile.name.is_none() {
        diags.push(LintDiagnostic {
            severity: Severity::Info,
            message: "Profile has no name".to_string(),
            suggestion: None,
        });
    }

    diags
}
```

---

### `lib/src/profile/default.rs`

Implements the default profile lookup chain.

```rust
/// Probe locations in order and return the first profile path found.
pub fn find_default_profile() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let candidates = default_profile_candidates(&cwd);
    candidates.into_iter().find(|p| p.exists())
}

pub fn default_profile_candidates(cwd: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        cwd.join("seatbelt.yaml"),
        cwd.join(".seatbelt.yaml"),
    ];

    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".config"));
    candidates.push(xdg_config.join("seatbelt/profile.yaml"));

    candidates
}

pub fn no_profile_error(cwd: &Path) -> SeatbeltError {
    let candidates = default_profile_candidates(cwd);
    let paths_listed = candidates.iter()
        .map(|p| format!("  - {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");

    SeatbeltError::NoProfileFound(format!(
        "No profile specified and no default profile found.\nLooked in:\n{}\n\nOptions:\n  \
        seatbelt run --profile <path> -- <command>\n  \
        seatbelt run --preset ai-agent-strict -- <command>\n  \
        Create a seatbelt.yaml in the current directory",
        paths_listed
    ))
}
```

---

### `lib/src/log_stream.rs`

Spawn and parse macOS's `log stream` command. Used by `runner.rs` (for `--verbose`) and `generator.rs`.

Also provides a post-hoc query function for the default violation summary and `seatbelt explain`.

```rust
use tokio::process::Command;
use tokio::io::{BufReader, AsyncBufReadExt};

/// A single sandbox violation parsed from log output
#[derive(Debug, Clone)]
pub struct Violation {
    pub process_name: String,
    pub pid: u32,
    pub operation: String,   // e.g. "file-read-data", "network-outbound"
    pub path: String,        // the resource that was denied
    pub raw: String,         // original log line
}

/// Spawn `log stream` filtered to sandbox denials for a given PID.
/// Returns a stream of Violation events (for --verbose real-time output).
pub async fn stream_violations(pid: u32) -> impl tokio_stream::Stream<Item = Violation> {
    let predicate = format!(
        r#"sender == "Sandbox" AND processID == {}"#,
        pid
    );
    let mut child = Command::new("log")
        .args(["stream", "--predicate", &predicate, "--style", "compact"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn log stream");

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    tokio_stream::wrappers::LinesStream::new(reader.lines())
        .filter_map(|line| async {
            line.ok().and_then(|l| parse_violation_line(&l))
        })
}

/// Query the system log for sandbox violations that occurred for a given PID
/// between start_time and now. Used for post-exit violation summary and
/// `seatbelt explain` default mode.
pub fn query_violations(pid: u32, start_time: &str) -> Result<Vec<Violation>> {
    let predicate = format!(
        r#"sender == "Sandbox" AND processID == {}"#,
        pid
    );
    let output = std::process::Command::new("log")
        .args(["show", "--predicate", &predicate, "--start", start_time, "--style", "compact"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let violations: Vec<Violation> = stdout.lines()
        .filter_map(parse_violation_line)
        .collect();
    Ok(violations)
}

/// Parse a log line like:
/// 2024-01-15 12:34:56.789 Sandbox[1234]: deny(1) file-read-data /Users/foo/.ssh/config
pub fn parse_violation_line(line: &str) -> Option<Violation> {
    let deny_pos = line.find("deny(")?;
    let after_deny = &line[deny_pos..];

    let paren_close = after_deny.find(") ")?;
    let operation_and_path = &after_deny[paren_close + 2..];
    let space_pos = operation_and_path.find(' ')?;
    let operation = operation_and_path[..space_pos].to_string();
    let path = operation_and_path[space_pos + 1..].trim().to_string();

    let (process_name, pid) = parse_process_from_line(line)?;

    Some(Violation { process_name, pid, operation, path, raw: line.to_string() })
}
```

---

### `bin/src/runner.rs`

Implements `seatbelt run`. Key responsibilities:
1. Load and resolve the profile (from `--profile` path or `--preset` name)
2. Run `seatbelt check` internally; abort if errors (warnings are printed)
3. Compile profile to SBPL, passing the command binary for auto exec-allow
4. Write SBPL to a temp file
5. Build `sandbox-exec -f <tempfile> -- <command>` invocation
6. Record start timestamp and PID for post-exit log query
7. Persist PID to `~/.cache/seatbelt/last-run.json` for `seatbelt explain` default mode
8. If `--verbose`, concurrently spawn `log stream` and print violations to stderr
9. After process exit, query system log for violation count and print summary
10. If `--explain`, call `explainer::explain_violations()` with the collected violations
11. Exit with the same exit code as the sandboxed process

```rust
use seatbelt_lib::{log_stream, explainer, profile};
use std::time::SystemTime;

/// State persisted for `seatbelt explain` default mode
#[derive(serde::Serialize, serde::Deserialize)]
struct LastRun {
    pid: u32,
    start_time: String,  // ISO 8601 format for `log show --start`
    command: Vec<String>,
}

fn last_run_path() -> PathBuf {
    let cache = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".cache"));
    cache.join("seatbelt/last-run.json")
}

pub async fn run(args: &RunArgs) -> anyhow::Result<()> {
    let profile = load_profile_or_preset(&args.profile, &args.preset)?;
    let resolved = resolve_profile(&profile)?;

    // Run linter — abort on errors, print warnings
    let diags = profile::linter::lint(&resolved);
    print_diagnostics(&diags);
    let error_count = diags.iter()
        .filter(|d| matches!(d.severity, profile::linter::Severity::Error))
        .count();
    if error_count > 0 {
        return Err(SeatbeltError::LintErrors(error_count).into());
    }

    // Resolve the command binary path for auto exec-allow
    let command_binary = which::which(&args.command[0]).ok()
        .map(|p| p.to_string_lossy().to_string());
    let sbpl = profile::compiler::compile(&resolved, command_binary.as_deref())?;

    if args.dry_run {
        println!("{}", sbpl);
        return Ok(());
    }

    // Write SBPL to a temp file
    let tmp = tempfile::NamedTempFile::new()?;
    std::fs::write(tmp.path(), &sbpl)?;

    // Record start time for post-exit log query
    let start_time = chrono_or_format_timestamp();

    // Build the sandbox-exec command
    let mut cmd = vec![
        "/usr/bin/sandbox-exec".to_string(),
        "-f".to_string(),
        tmp.path().to_string_lossy().to_string(),
        "--".to_string(),
    ];
    cmd.extend(args.command.iter().cloned());

    // Spawn the sandboxed process
    let mut child = tokio::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .spawn()?;

    let pid = child.id().expect("failed to get pid");

    // Persist last-run state for `seatbelt explain`
    let last_run = LastRun {
        pid,
        start_time: start_time.clone(),
        command: args.command.clone(),
    };
    persist_last_run(&last_run);

    // If --verbose, spawn concurrent log stream
    if args.verbose {
        let violation_stream = log_stream::stream_violations(pid).await;
        // Print each violation to stderr as it arrives (in a spawned task)
        tokio::spawn(async move {
            tokio::pin!(violation_stream);
            while let Some(v) = violation_stream.next().await {
                eprintln!("seatbelt: denied {} {}", v.operation, v.path);
            }
        });
    }

    let status = child.wait().await?;

    // Post-exit: query system log for violation summary
    let violations = log_stream::query_violations(pid, &start_time)?;
    if !violations.is_empty() {
        eprintln!(
            "seatbelt: {} sandbox violation(s) occurred. Run `seatbelt explain` for details.",
            violations.len()
        );
    }

    // If --explain, show detailed explanations
    if args.explain {
        for v in &violations {
            let explanation = explainer::explain_violation(v);
            print_explanation(&explanation);
        }
    }

    std::process::exit(status.code().unwrap_or(1));
}

fn persist_last_run(last_run: &LastRun) {
    let path = last_run_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string(last_run).unwrap());
}
```

---

### `bin/src/generator.rs`

Implements `seatbelt generate`. This is the most complex module.

**Algorithm:**

1. Spawn the target command under a "report-all" sandbox that allows everything but logs operations
2. Simultaneously stream `log stream` for the process to capture all access patterns
3. Collect all logged file paths, network connections, process execs, and Mach lookups
4. After the process exits, deduplicate and categorize the collected operations
5. Build a `Profile` struct from the observations, minimizing the number of rules
6. If `--base-preset` is given, subtract rules already covered by the preset
7. Serialize to YAML (or SBPL if `--format sbpl`)

**Log strategy for observation mode:**

Use a "report-all" SBPL profile:
```scheme
(version 1)
(allow default)
(deny file-write* (with report) (subpath "/"))
(deny network-outbound (with report))
```
This allows everything (so the process runs normally) but logs every file write and network attempt.

**Path minimization:** Don't emit a rule per file. Group paths into their minimal subpath set. E.g., if we see `/opt/homebrew/lib/python3.11/site-packages/foo.py` and `/opt/homebrew/lib/python3.11/site-packages/bar.py`, emit a single `(allow file-read* (subpath "/opt/homebrew/lib"))` rule.

```rust
pub async fn generate(args: &GenerateArgs) -> anyhow::Result<()> {
    let observations = observe_process(&args.command, args.runs).await?;
    let profile = build_profile_from_observations(&observations, &args.base_preset)?;

    let output = match args.format.as_str() {
        "sbpl" => profile::compiler::compile(&profile, None)?,
        _ => serde_yaml::to_string(&profile)?,
    };

    match &args.output {
        Some(path) => std::fs::write(path, output)?,
        None => print!("{}", output),
    }

    Ok(())
}

#[derive(Default)]
struct Observations {
    file_reads: HashSet<PathBuf>,
    file_writes: HashSet<PathBuf>,
    network_outbound: HashSet<String>,
    process_execs: HashSet<PathBuf>,
    mach_lookups: HashSet<String>,
}

fn minimize_paths(paths: &HashSet<PathBuf>) -> Vec<String> {
    // Sort paths, then use a prefix-tree approach:
    // if >2 paths share a parent directory, emit the parent instead
    // Cap depth at ~4 components to avoid over-permissive rules
    // Never emit (subpath "/") or (subpath "/usr") — too broad
    todo!()
}
```

---

### `lib/src/explainer.rs`

Maps raw violation strings to human-readable explanations with suggested YAML fixes.

```rust
pub struct Explanation {
    pub headline: String,
    pub context: String,
    pub yaml_fix: Option<String>,
}

pub fn explain_violation(v: &Violation) -> Explanation {
    match classify_path(&v.path) {
        PathClass::SshKey => Explanation {
            headline: format!("{} tried to read an SSH key: {}", v.process_name, v.path),
            context: "SSH private keys are sensitive. Only allow this if your script needs SSH.".to_string(),
            yaml_fix: Some("filesystem:\n  read:\n    - (home)/.ssh/config  # read-only, not keys".to_string()),
        },
        PathClass::HomeDir => Explanation {
            headline: format!("{} tried to access your home directory: {}", v.process_name, v.path),
            context: "The process wants to read from your home directory.".to_string(),
            yaml_fix: Some(format!("filesystem:\n  read:\n    - {}", v.path)),
        },
        PathClass::SystemLib => Explanation {
            headline: format!("{} needs a system library: {}", v.process_name, v.path),
            context: "This is a standard macOS system library. It should be safe to allow.".to_string(),
            yaml_fix: Some(format!("filesystem:\n  read:\n    - {}", v.path)),
        },
        PathClass::Network => Explanation {
            headline: format!("{} tried to make a network connection", v.process_name),
            context: format!("Connection to: {}", v.path),
            yaml_fix: Some("network:\n  outbound:\n    allow: true".to_string()),
        },
        PathClass::Unknown => Explanation {
            headline: format!("{} was blocked from: {} ({})", v.process_name, v.path, v.operation),
            context: String::new(),
            yaml_fix: Some(format!("filesystem:\n  read:\n    - {}", v.path)),
        },
    }
}

enum PathClass {
    SshKey, HomeDir, SystemLib, TmpDir, HomeBrew, Network, MachService, Unknown
}

fn classify_path(path: &str) -> PathClass {
    if path.contains("/.ssh/id_") || path.contains("/.ssh/id_rsa") { return PathClass::SshKey; }
    if path.starts_with("/usr/lib") || path.starts_with("/System/Library") { return PathClass::SystemLib; }
    // ... etc
    PathClass::Unknown
}
```

---

### `lib/src/presets/mod.rs`

Presets are embedded into the binary at compile time using `include_str!()`.

```rust
use std::collections::HashMap;

pub fn get_preset(name: &str) -> Option<&'static str> {
    let presets: HashMap<&str, &str> = [
        ("ai-agent-strict",      include_str!("profiles/ai-agent-strict.yaml")),
        ("ai-agent-networked",   include_str!("profiles/ai-agent-networked.yaml")),
        ("ai-agent-permissive",  include_str!("profiles/ai-agent-permissive.yaml")),
        ("read-only",            include_str!("profiles/read-only.yaml")),
        ("build-tool",           include_str!("profiles/build-tool.yaml")),
        ("network-only",         include_str!("profiles/network-only.yaml")),
    ].into_iter().collect();

    presets.get(name).copied()
}

pub fn list_presets() -> Vec<&'static str> {
    vec!["ai-agent-strict", "ai-agent-networked", "ai-agent-permissive",
         "read-only", "build-tool", "network-only"]
}
```

---

## Built-in Preset YAML Files

### `ai-agent-strict.yaml`
```yaml
version: 1
name: ai-agent-strict
description: >
  AI coding agent sandboxed to the project directory. Read access to
  system libraries and cwd; write access to cwd and temp dirs only.
  No network access. Unrestricted exec (agents need to run various tools).

filesystem:
  read:
    - /usr/lib
    - /usr/local/lib
    - /opt/homebrew
    - /System/Library
    - (cwd)
  write:
    - (cwd)
    - (tmpdir)
  deny:
    - (home)/.ssh/id_*
    - (home)/.aws
    - (home)/.config/gcloud
    - (home)/.npmrc
    - (home)/.pypirc

network:
  outbound:
    allow: false
  inbound:
    allow: false

process:
  allow_fork: true
  allow_exec_any: true

system:
  allow_sysctl_read: true
  allow_mach_lookup:
    - com.apple.system.logger
    - com.apple.system.notification_center
```

### `ai-agent-networked.yaml`
```yaml
version: 1
name: ai-agent-networked
description: >
  AI coding agent with outbound network access. Same filesystem
  restrictions as ai-agent-strict.

extends: ai-agent-strict

network:
  outbound:
    allow: true
  inbound:
    allow: false
```

### `ai-agent-permissive.yaml`
```yaml
version: 1
name: ai-agent-permissive
description: >
  AI coding agent with broad read access and outbound network.
  Can read anywhere on disk, write to project dir and temp dirs.

filesystem:
  read:
    - /
  write:
    - (cwd)
    - (tmpdir)
  deny:
    - (home)/.ssh/id_*
    - (home)/.aws
    - (home)/.config/gcloud

network:
  outbound:
    allow: true
  inbound:
    allow: false

process:
  allow_fork: true
  allow_exec_any: true

system:
  allow_sysctl_read: true
  allow_mach_lookup:
    - com.apple.system.logger
    - com.apple.system.notification_center
```

### `read-only.yaml`
```yaml
version: 1
name: read-only
description: >
  Read anywhere, write nowhere. For untrusted scripts, static analysis,
  and code review agents.

filesystem:
  read:
    - /
  write: []
  deny:
    - (home)/.ssh/id_*

network:
  outbound:
    allow: false
  inbound:
    allow: false

process:
  allow_fork: true
  allow_exec_any: true

system:
  allow_sysctl_read: true
```

### `build-tool.yaml`
```yaml
version: 1
name: build-tool
description: >
  Build steps, compilers, and test runners. Read anywhere,
  write to project dir and temp dirs. No network access.

filesystem:
  read:
    - /
  write:
    - (cwd)
    - (tmpdir)
  deny:
    - (home)/.ssh/id_*

network:
  outbound:
    allow: false
  inbound:
    allow: false

process:
  allow_fork: true
  allow_exec_any: true

system:
  allow_sysctl_read: true
```

### `network-only.yaml`
```yaml
version: 1
name: network-only
description: >
  Network utilities and curl wrappers. Read system libraries only,
  no filesystem writes. Unrestricted outbound network.

filesystem:
  read:
    - /usr/lib
    - /usr/local/lib
    - /System/Library
  write: []
  deny:
    - (home)/.ssh/id_*

network:
  outbound:
    allow: true
  inbound:
    allow: false

process:
  allow_fork: true
  allow_exec_any: true

system:
  allow_sysctl_read: true
```

---

## Error Message Quality

Errors must be actionable. Never show a raw Rust error to the user without context.

**Bad:**
```
Error: Os { code: 2, kind: NotFound, message: "No such file or directory" }
```

**Good:**
```
Error: Profile file not found: ./my-profile.yaml
  Make sure the path is correct, or use --preset to use a built-in profile.
  Run `seatbelt --list-presets` to see available presets.
```

Use `anyhow::Context` to add context at each call site:
```rust
std::fs::read_to_string(&path)
    .with_context(|| format!("Profile file not found: {}", path.display()))?;
```

---

## Testing Strategy

### Unit tests (in-module, under `lib/src/`)
- `profile/compiler.rs`: test that specific profile structs generate correct SBPL snippets, including glob→regex paths
- `profile/resolver.rs`: test each magic variable expansion, `(cwd)/suffix`, `(home)/suffix`, glob detection
- `profile/loader.rs`: test `deep_merge_yaml` with various override scenarios
- `profile/linter.rs`: test each lint rule in isolation
- `explainer.rs`: test that known violation patterns produce correct explanations
- `log_stream.rs`: test `parse_violation_line` against known log line formats

### Integration tests (`tests/integration/`)
Use `assert_cmd` to invoke the actual `seatbelt` binary.

```rust
// tests/integration/run_test.rs
#[test]
fn run_with_read_only_preset_blocks_writes() {
    let script = r#"
import sys
with open('/tmp/seatbelt_test_write.txt', 'w') as f:
    f.write('hello')
sys.exit(0)
"#;
    Command::cargo_bin("seatbelt").unwrap()
        .args(["run", "--preset", "read-only", "--", "python3", "-c", script])
        .assert()
        .failure();
}

#[test]
fn compile_outputs_valid_sbpl() {
    // create a minimal temp YAML profile, compile it, check output contains (deny default)
}

#[test]
fn check_catches_invalid_yaml() {
    // pass a malformed YAML file, assert non-zero exit and error message
}

#[test]
fn check_catches_unknown_keys() {
    // profile with a typo like "filesytem:" should error due to deny_unknown_fields
}

#[test]
fn extends_inherits_parent_rules() {
    // profile extending ai-agent-strict with only network override
    // should retain parent's filesystem rules in compiled SBPL
}
```

---

## Implementation Order

Build in this exact sequence to ensure each phase is independently useful:

**Phase 1 — Foundation (do this first)**
1. `lib/src/error.rs` — error types including `NoProfileFound`
2. `lib/src/profile/schema.rs` — YAML schema structs with `deny_unknown_fields`
3. `lib/src/profile/loader.rs` — load YAML, resolve `extends` via YAML-level deep merge
4. `lib/src/profile/resolver.rs` — expand magic variables, glob detection, `glob_to_regex`
5. `lib/src/profile/compiler.rs` — compile resolved Profile to SBPL (with glob→regex, size check, auto exec-allow)
6. `lib/src/profile/default.rs` — default profile discovery
7. `lib/src/presets/mod.rs` + all 6 preset YAML files
8. `bin/src/cli.rs` — all CLI structs including `Command::External`
9. `bin/src/main.rs` — entry point, dispatch
10. `seatbelt compile` command — end-to-end test of phases 1-9
11. `seatbelt run` command — with auto exec-allow, default profile discovery, `Command::External` passthrough

**Phase 2 — Safety layer**
12. `lib/src/profile/linter.rs` — lint rules
13. `seatbelt check` command (with `--strict`)
14. Wire linter into `seatbelt run` (warn on warnings, abort on errors)

**Phase 3 — Killer feature**
15. `lib/src/log_stream.rs` — stream and query violations from `log stream`/`log show`
16. `lib/src/explainer.rs` — violation explanations
17. Post-exit violation summary in `seatbelt run` (always-on, queries log after exit)
18. `--verbose` flag in `seatbelt run` (real-time streaming)
19. `--explain` flag in `seatbelt run` (detailed post-exit explanations)
20. `seatbelt explain` command — with PID persistence via `last-run.json`

**Phase 4 — Generator**
21. `bin/src/generator.rs` — observe mode + path minimization
22. `seatbelt generate` command

---

## Known SBPL Quirks to Encode

These must be handled correctly in `compiler.rs` and reflected in `linter.rs`:

1. **`(deny sysctl-write)` requires `(allow sysctl-read)`** even under `(deny default)`. Always emit both together.
2. **`file-write*` does not imply `file-read*`**. Always emit both for write-allowed paths.
3. **`(deny default)` is the only safe baseline.** Never generate `(allow default)` profiles — they are security theater. (Exception: the generator's observation mode uses `(allow default)` with `(with report)` to log operations without blocking them.)
4. **Last rule wins**, not first. Explicit denies must come after the allows they override.
5. **`/private/tmp` vs `/tmp`** — on macOS, `/tmp` is a symlink to `/private/tmp`. Allow both.
6. **`/var/folders`** — macOS uses `$TMPDIR` for per-user temp space, which resolves to `/var/folders/XX/YYYY/T/`. The resolver must expand `(tmpdir)` to include it.
7. **Dyld shared cache** — processes reading `/private/var/db/dyld/` is normal and must be in the baseline.
8. **Rosetta 2** — on Apple Silicon running x86_64 binaries, processes need `(allow process-exec (subpath "/Library/Apple/usr/libexec/oah"))`. This is in the baseline.
9. **Profile size limit** — the serialized SBPL cannot exceed 65,535 bytes. The compiler errors if exceeded and warns above 50KB.
10. **Glob patterns** — SBPL has no native glob support. Paths containing `*` or `?` must be compiled to `(regex ...)` matchers. The `resolver::glob_to_regex` function handles this conversion.
