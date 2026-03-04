# seatbelt — Product Specification

## Overview

`seatbelt` is a macOS command-line tool that makes `sandbox-exec` usable by humans. It replaces raw SBPL (Sandbox Profile Language) authoring with a clean YAML config format, automates profile generation by observing real process behavior, and explains sandbox violations in plain English. It is aimed at developers who want to harden their local workflow — especially those running AI coding agents like Claude Code — without becoming experts in an undocumented Scheme-based DSL.

---

## Problem

`sandbox-exec` is the only mechanism on macOS for sandboxing arbitrary CLI processes. It is used by Bazel, Nix, Chromium, Firefox, Swift Package Manager, Claude Code, OpenAI Codex, and Google Gemini CLI. It is also:

- **Deprecated** since macOS 10.12 with no replacement for CLI use cases
- **Completely undocumented** — no official spec for SBPL exists
- **Hostile to author** — Scheme/Lisp syntax, silent failures, no linting
- **Impossible to debug** — Apple's built-in trace directive broke in macOS 10.13 and was never fixed
- **Fragile across macOS versions** — profiles silently break when Apple changes internal paths

The result is a tool every security-conscious macOS developer needs but almost none can actually use. Every project that depends on sandbox-exec has independently invented a subset of what `seatbelt` would provide.

---

## Target Users

**Primary:** macOS developers running AI coding agents (Claude Code, Codex, Gemini CLI) who want process-level sandboxing without Docker overhead.

**Secondary:** Developers who run untrusted scripts, build steps, or third-party tools and want to limit what those processes can touch.

**Not targeted:** App Store developers (use App Sandbox entitlements), security researchers doing sandbox escape research.

---

## Core Commands

### `seatbelt run`

Run a command under a sandbox defined by a YAML profile.

```
seatbelt run --profile agent-strict.yaml -- claude --dangerously-skip-permissions
seatbelt run --profile my-project.yaml -- npm run build
seatbelt run --preset ai-agent-networked -- python3 agent.py
```

Compiles the YAML profile to SBPL, invokes `sandbox-exec`, and streams stdout/stderr transparently. The runner automatically adds an exec-allow rule for the sandboxed command's binary — you don't need to list it in `allow_exec`.

After the process exits, seatbelt queries the system log for sandbox violations that occurred during execution. If any are found, a brief summary is printed to stderr (e.g., `seatbelt: 3 sandbox violations occurred. Run 'seatbelt explain' for details.`). The last run's PID is persisted to `~/.cache/seatbelt/last-run.json` so `seatbelt explain` works without arguments.

**Key flags:**
- `--profile <path>` — path to a YAML profile file
- `--preset <name>` — use a built-in named preset
- `--explain` — after the process exits, print detailed per-violation explanations with suggested YAML fixes
- `--dry-run` — print the generated SBPL and exit without running anything
- `--verbose` — stream sandbox violation events to stderr in real time during execution

---

### `seatbelt generate`

Observe a process's behavior and generate a minimal sandbox profile.

```
seatbelt generate -- python3 my_script.py
seatbelt generate --output my_script.yaml -- npm install
seatbelt generate --base-preset ai-agent-strict -- claude
```

Runs the target command with no restrictions while simultaneously monitoring the macOS system log for what the process actually touches. On completion, emits a minimal YAML profile that allows exactly those operations and denies everything else.

This is the feature no one has built for modern macOS. It replaces the manual trial-and-error loop that every sandbox-exec user currently does by hand.

**Key flags:**
- `--output <path>` — write the generated profile here (default: stdout)
- `--base-preset <name>` — start from a preset and only emit the additional rules needed on top of it
- `--format yaml|sbpl` — output format (default: yaml)
- `--runs <n>` — run the command n times and union the observed access patterns (for non-deterministic programs)

---

### `seatbelt explain`

Parse sandbox violation logs and explain them in plain English.

```
seatbelt explain                        # explain violations from the last seatbelt run
seatbelt explain --pid 12345            # explain violations for a running or recent process
seatbelt explain --log violations.txt   # explain violations from a saved log file
seatbelt explain --all                  # include network, Mach, sysctl violations (not just file access)
```

Maps raw denial entries like `deny(1) file-read-data /Users/you/.ssh/config` to:

```
⛔  python3 was blocked from reading /Users/you/.ssh/config
    This is your SSH private key directory. Only allow this if your
    script legitimately needs SSH access.

    To allow the exact file, add to your profile:

        filesystem:
          read:
            - (home)/.ssh/config
```

---

### `seatbelt check`

Lint and validate a profile file without running anything.

```
seatbelt check my-profile.yaml
seatbelt check --sbpl my-profile.sb
```

Checks for: YAML syntax errors, unknown keys, logical contradictions, dangerous patterns (overly permissive rules), missing commonly-required rules, and rules that will silently never match.

**Key flags:**
- `--sbpl` — check an SBPL file instead of YAML
- `--strict` — treat warnings as errors (useful for CI)

---

### `seatbelt compile`

Compile a YAML profile to SBPL for use with raw `sandbox-exec` or other tools.

```
seatbelt compile my-profile.yaml
seatbelt compile --output my-profile.sb my-profile.yaml
```

Useful for integrating with tools that invoke `sandbox-exec` directly (Bazel, Nix, etc.) or for auditing what SBPL gets generated.

---

## Built-in Presets

Shipped with the binary, tested against current macOS versions, maintained across releases. Named after how much trust they grant to the sandboxed process.

| Preset | Filesystem | Network | Use case |
|--------|-----------|---------|----------|
| `ai-agent-strict` | Read system libs + cwd, write cwd + tmpdir | Blocked | Claude Code / Codex on a specific project |
| `ai-agent-networked` | Same as above | Outbound unrestricted | AI agents that need to fetch packages or call APIs |
| `ai-agent-permissive` | Read anywhere, write cwd + tmpdir | Outbound unrestricted | Agents that need broad read access |
| `read-only` | Read anywhere, write nowhere | Blocked | Code review, static analysis, untrusted scripts |
| `build-tool` | Read anywhere, write cwd + tmpdir | Blocked | Build steps, compilers, test runners |
| `network-only` | Read system libs only | Outbound unrestricted | Network utilities, curl wrappers |

---

## YAML Profile Format

The config format is the heart of the product. It should be immediately readable by any developer without documentation.

```yaml
# my-project.yaml
name: my-project
description: Sandbox for running AI agent on the acme repo
version: 1

extends: ai-agent-strict  # inherit a preset, then override

filesystem:
  read:
    - /usr/lib
    - /usr/local/lib
    - /opt/homebrew
    - ~/Library/Python
    - (cwd)           # magic variable: current working directory at runtime
    - (home)/.ssh/known_hosts  # read-only ssh, not private keys
  write:
    - (cwd)
    - (tmpdir)        # /tmp and /var/folders/...
  deny:
    - (home)/.ssh/id_*  # explicit deny even if a parent is allowed

network:
  outbound:
    allow: true
    # optional domain filter (implemented via proxy)
    allow_domains:
      - api.anthropic.com
      - pypi.org
  inbound:
    allow: false

process:
  allow_exec:              # specific executables this process may exec
    - /usr/bin/git
    - /opt/homebrew/bin/python3
  allow_exec_any: false    # true = unrestricted exec (overrides allow_exec)
  allow_fork: true         # allow spawning child processes

system:
  allow_sysctl_read: true    # needed by many runtimes
  allow_sysctl_write: false  # rarely needed, off by default
  allow_mach_lookup:         # XPC services the process needs
    - com.apple.hiservices-xpcservice   # clipboard
```

**Magic variables** resolved at runtime:
- `(cwd)` — working directory when `seatbelt run` is invoked
- `(home)` — the invoking user's home directory
- `(tmpdir)` — macOS temp directories (`/tmp`, `/var/folders/...`)
- `(bundle <id>)` — path to an installed application bundle

Magic variables can be used as prefixes: `(home)/.ssh/known_hosts` resolves to `/Users/you/.ssh/known_hosts`.

**Glob patterns** are supported in path strings. A `*` matches any characters within a single path component. For example, `(home)/.ssh/id_*` matches `id_rsa`, `id_ed25519`, etc. The compiler converts globs to SBPL `(regex ...)` matchers. Paths without globs use `(subpath ...)` for recursive directory matching.

---

## Profile Inheritance (`extends`)

A profile can inherit from a named preset using `extends`, then override specific fields:

```yaml
extends: ai-agent-strict

network:
  outbound:
    allow: true   # override parent's false → enable outbound network
```

**Merge semantics:** The parent and child are deep-merged at the YAML level before deserialization. At each leaf:
- **Scalars** (booleans, strings, numbers): child value replaces parent value
- **Lists** (read paths, deny paths, allow_exec, etc.): child list **replaces** parent list entirely
- **Absent keys**: parent value is preserved

This means if you extend a preset and want to modify a list, you must re-specify the complete list. This is intentional — it keeps the merge model simple and predictable. If you only need to change behavioral flags (like enabling network), the parent's lists carry over untouched.

The `extends` field is resolved before all other processing (variable expansion, linting, compilation).

---

## Default Profile Resolution

When no `--profile` or `--preset` flag is given, `seatbelt` resolves a profile automatically from the following locations, in order:

1. `./seatbelt.yaml` — project-level profile in the current directory
2. `./.seatbelt.yaml` — hidden variant
3. `$XDG_CONFIG_HOME/seatbelt/profile.yaml` — user global default (typically `~/.config/seatbelt/profile.yaml`)

If none of these exist, `seatbelt` exits with a clear error listing the paths it checked.

This enables the short-form invocation:

```
seatbelt python3 agent.py
seatbelt npm run build
seatbelt -- claude --dangerously-skip-permissions
```

When a default profile is resolvable, `run` becomes an optional subcommand. Unrecognized first arguments are treated as the start of the command to sandbox. The explicit form `seatbelt run --profile ...` always works regardless.

A project can commit a `seatbelt.yaml` to its repository so every contributor gets consistent sandboxing without any setup. A user can place a global profile in `~/.config/seatbelt/profile.yaml` as their personal default for all AI agent usage.

---

## Non-Goals

- **Network filtering by domain name in-process.** This requires running a proxy outside the sandbox. Out of scope for v1; potentially a v2 feature.
- **Resource limits** (RAM, CPU). sandbox-exec cannot enforce these. Out of scope entirely.
- **Linux support.** The tool is macOS-only. The problem it solves (SBPL ergonomics) is macOS-only.
- **GUI.** CLI only.
- **App Sandbox replacement.** This is not for App Store apps.

---

## Distribution

- **Homebrew**: `brew install seatbelt` (primary)
- **GitHub Releases**: pre-built binaries for arm64 and x86_64
- **cargo install seatbelt**: for Rust developers

Single static binary. No runtime dependencies. Works on macOS 13 Ventura and later.

---

## Success Metrics (for v1)

- A developer can go from zero to a working sandbox for Claude Code in under 5 minutes
- `seatbelt generate` produces a working profile for a standard Python script without manual editing
- All built-in presets tested and confirmed working on macOS 13, 14, and 15
- `seatbelt check` catches the five most common SBPL mistakes without false positives
