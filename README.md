# seatbelt

A macOS CLI that makes `sandbox-exec` usable by humans. Replaces raw SBPL authoring with a YAML config format, automates profile generation by observing process behavior, and explains sandbox violations in plain English.

**Target users:** developers running AI coding agents (Claude Code, Codex, Gemini CLI) who want process-level sandboxing without Docker overhead.

macOS 13 Ventura or later. Single static binary.

## Quickstart

```bash
# Run a command under a built-in preset
seatbelt run --preset ai-agent-strict -- claude --dangerously-skip-permissions

# With network access
seatbelt run --preset ai-agent-networked -- python3 agent.py

# See what SBPL gets generated (without running)
seatbelt run --dry-run --preset ai-agent-strict -- echo hello

# Compile a custom YAML profile to SBPL
seatbelt compile my-profile.yaml

# Short form (auto-discovers ./seatbelt.yaml)
seatbelt -- npm run build
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

## License

MIT OR Apache-2.0
