# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-03-06

### Changed

- Homebrew publish now emits `Formula/seatbelt.rb` (install path `kulesh/tap/seatbelt`) instead of `seatbelt-bin.rb`
- Release workflow now fails fast with a clear error if `HOMEBREW_TAP_TOKEN` is missing
- Added installation and maintainer release setup docs for the Homebrew tap flow

## [0.1.0] - 2026-03-04

### Added

- **`seatbelt run`** — run a command under a sandbox defined by a YAML profile or built-in preset
- **`seatbelt compile`** — compile a YAML profile to raw SBPL for use with `sandbox-exec`
- **`seatbelt check`** — lint and validate a profile file with six rules covering version, network, filesystem, exec, and naming
- **`seatbelt explain`** — parse sandbox violations from system log or file and explain them in plain English with YAML fix suggestions
- **`seatbelt generate`** — observe a process under a report-all sandbox and emit a minimal profile from its access patterns
- **`--verbose` flag** on `run` — stream violations to stderr in real time
- **`--explain` flag** on `run` — print per-violation explanations after the process exits
- **`--strict` flag** on `check` — treat warnings as errors
- **`--base-preset` flag** on `generate` — subtract already-covered rules from the generated profile
- Six built-in presets: `ai-agent-strict`, `ai-agent-networked`, `ai-agent-permissive`, `read-only`, `build-tool`, `network-only`
- YAML profile format with magic variables (`(cwd)`, `(home)`, `(tmpdir)`, `~`), glob patterns, and preset inheritance via `extends`
- Profile discovery chain: `./seatbelt.yaml`, `./.seatbelt.yaml`, `$XDG_CONFIG_HOME/seatbelt/profile.yaml`
- External subcommand passthrough (`seatbelt -- npm test` auto-discovers default profile)
