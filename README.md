# seatbelt

A macOS CLI that makes `sandbox-exec` usable by humans. Replaces raw SBPL authoring with a YAML config format, automates profile generation by observing process behavior, and explains sandbox violations in plain English.

**Target users:** developers running AI coding agents (Claude Code, Codex, Gemini CLI) who want process-level sandboxing without Docker overhead.

macOS 13 Ventura or later. Single static binary.

## Installation

```bash
brew tap kulesh/tap
brew install seatbelt
```

Or without adding the tap first:

```bash
brew install kulesh/tap/seatbelt
```

## Quickstart

```bash
# Run a command under a built-in preset
seatbelt run --preset ai-agent-strict -- claude --dangerously-skip-permissions

# With network access
seatbelt run --preset ai-agent-networked -- python3 agent.py

# See what SBPL gets generated (without running)
seatbelt run --dry-run --preset ai-agent-strict -- echo hello

# Short form (auto-discovers ./seatbelt.yaml)
seatbelt -- npm run build
```

## Commands

### `seatbelt run`

Run a command inside a sandbox. Loads a YAML profile or built-in preset, compiles it to SBPL, and invokes `sandbox-exec`.

```bash
seatbelt run --preset ai-agent-strict -- npm test
seatbelt run --profile my-profile.yaml -- python3 script.py
seatbelt run --verbose --preset read-only -- ls /tmp    # stream violations in real time
seatbelt run --explain --preset ai-agent-strict -- make  # explain violations after exit
```

### `seatbelt check`

Lint and validate a profile without running anything. Catches common mistakes before they hit the kernel.

```bash
seatbelt check my-profile.yaml
seatbelt check --strict my-profile.yaml  # treat warnings as errors
```

Six lint rules: version validation, allow_domains consistency, write path safety, unrestricted network warning, missing exec permissions, unnamed profiles.

### `seatbelt explain`

Parse sandbox violations and explain them in plain English with YAML fix suggestions.

```bash
seatbelt explain                     # explain violations from the last seatbelt run
seatbelt explain --pid 12345         # explain violations for a specific PID
seatbelt explain --log sandbox.log   # explain violations from a log file
seatbelt explain --all               # include non-file violations (network, mach, etc.)
```

### `seatbelt generate`

Observe a command's behavior and generate a minimal sandbox profile automatically.

```bash
seatbelt generate -- npm test                             # observe and emit YAML
seatbelt generate --output profile.yaml -- python3 app.py # write to file
seatbelt generate --base-preset ai-agent-strict -- make   # only emit rules beyond the preset
seatbelt generate --runs 3 -- npm test                    # union access across 3 runs
seatbelt generate --format sbpl -- echo hello             # emit raw SBPL
```

### `seatbelt compile`

Compile a YAML profile to SBPL for use with raw `sandbox-exec`.

```bash
seatbelt compile my-profile.yaml
seatbelt compile --output profile.sb my-profile.yaml
```

## YAML profile format

```yaml
version: 1
name: my-project

filesystem:
  read:
    - /usr/lib
    - /opt/homebrew
    - (cwd)
  write:
    - (cwd)
    - (tmpdir)
  deny:
    - (home)/.ssh/id_*
    - (home)/.aws

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

Magic variables: `(cwd)`, `(home)`, `(tmpdir)`, `~`. Glob patterns (`*`, `?`) are supported and compile to SBPL regex matchers.

Profiles can inherit from presets via `extends`:

```yaml
version: 1
extends: ai-agent-strict
network:
  outbound:
    allow: true
```

## Built-in presets

| Preset | Filesystem | Network | Use case |
|--------|-----------|---------|----------|
| `ai-agent-strict` | System libs + cwd read, cwd + tmpdir write | Blocked | Claude Code / Codex |
| `ai-agent-networked` | Same as strict | Outbound unrestricted | Agents needing APIs |
| `ai-agent-permissive` | Read anywhere, cwd + tmpdir write | Outbound unrestricted | Broad read access |
| `read-only` | Read anywhere, write nowhere | Blocked | Static analysis |
| `build-tool` | Read anywhere, cwd + tmpdir write | Blocked | Compilers, test runners |
| `network-only` | System libs only | Outbound unrestricted | curl wrappers |

## Building

```bash
cargo build --release
```

## Homebrew Release Setup (Maintainers)

`cargo-dist` publishes the formula to `kulesh/homebrew-tap` from the Release workflow.

Required once per repo:

1. Create a GitHub fine-grained PAT with `Contents: Read and write` for `kulesh/homebrew-tap`.
2. Add that token as a repository secret in `kulesh/seatbelt` named `HOMEBREW_TAP_TOKEN`.
3. Push a release tag (example: `git tag v0.1.1 && git push origin v0.1.1`).
4. Verify the workflow job `publish-homebrew-formula` succeeds and `Formula/seatbelt.rb` is committed in `kulesh/homebrew-tap`.

## License

MIT OR Apache-2.0
