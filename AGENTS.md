# AGENTS.md

Guidance for AI coding assistants working in this repository.

## Role

You are collaborating with Kulesh on `seatbelt`. Act like an owner of the codebase: preserve clarity, enforce strong boundaries, and leave the repo better than you found it.

## Product Context

`seatbelt` is a macOS-only CLI that makes `sandbox-exec` usable:

- author profiles in YAML, not raw SBPL
- compile YAML to SBPL
- run commands in sandbox
- explain sandbox violations in plain English
- generate profiles from observed process behavior

Primary users are developers running AI coding agents locally.

## Engineering Philosophy

1. Design bicycles, not Rube Goldberg machines.
2. Prefer solutions that remove classes of problems, not one-off patches.
3. Keep domain language precise and consistent.
4. Separate intent (schema/lint) from execution (run/generate/explain).
5. Favor maintainable, idiomatic Rust over cleverness.

## Ubiquitous Language

- `Profile`: YAML model (`schema.rs`) before runtime expansion.
- `ResolvedProfile`: profile after path expansion and glob classification (`resolver.rs`).
- `SBPL`: compiled sandbox rules string (`compiler.rs`).
- `Preset`: embedded YAML profile shipped in binary (`lib/src/presets/profiles/*.yaml`).
- `Violation`: parsed `log` deny event (`log_stream.rs`).
- `Explanation`: human-readable diagnosis + YAML fix suggestion (`explainer.rs`).

Use these terms consistently in code, tests, docs, and commit messages.

## Workspace Map

```text
seatbelt/
├── Cargo.toml                    # Workspace + shared dependencies + dist metadata
├── .mise.toml                    # Toolchain management (Rust = latest)
├── README.md
├── CHANGELOG.md
├── docs/
│   ├── specs/PRODUCT_SPEC.md
│   └── plans/IMPLEMENTATION_SPEC.md
├── lib/                          # seatbelt-lib (domain + compilation pipeline)
│   └── src/
│       ├── error.rs
│       ├── explainer.rs
│       ├── log_stream.rs
│       ├── presets/
│       │   ├── mod.rs
│       │   └── profiles/*.yaml
│       ├── profile/
│       │   ├── schema.rs
│       │   ├── loader.rs
│       │   ├── resolver.rs
│       │   ├── compiler.rs
│       │   ├── linter.rs
│       │   └── default.rs
│       └── sbpl/ops.rs
├── bin/                          # seatbelt-bin (CLI orchestration)
│   ├── src/{main.rs,cli.rs,runner.rs,generator.rs}
│   └── tests/integration.rs      # CLI acceptance tests
└── docs/
```

## Architecture Invariants

1. **Pipeline is explicit:** `schema` -> `resolver` -> `compiler`.
2. **`compiler` accepts `ResolvedProfile` only** to prevent unresolved path leakage.
3. **`seatbelt-lib` owns core logic**; `seatbelt-bin` owns process orchestration and terminal UX.
4. **Deny-by-default SBPL is mandatory.**
5. **Deny rules are emitted after allow rules** (SBPL last-rule-wins behavior).
6. **Write permission emits both read and write rules** in compiler output.
7. **Default profile discovery order must remain deterministic:**
   1. `./seatbelt.yaml`
   2. `./.seatbelt.yaml`
   3. `$XDG_CONFIG_HOME/seatbelt/profile.yaml` (fallback `~/.config/seatbelt/profile.yaml`)

## CLI Surface (Current)

- `seatbelt run`
- `seatbelt generate`
- `seatbelt explain`
- `seatbelt check`
- `seatbelt compile`
- external subcommand passthrough: `seatbelt -- <cmd>` or `seatbelt <cmd>` when default profile exists

If you change flags or behavior in `bin/src/cli.rs`, update:

1. `README.md`
2. tests in `bin/tests/integration.rs`
3. relevant docs under `docs/`

## Development Workflow

```bash
# install toolchain declared in .mise.toml
mise install

# build
cargo build --all-targets

# test (CI parity)
cargo test --all-targets

# lint + format
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Optional local speedups:

```bash
cargo check --all-targets
cargo watch -x build
```

## Testing Expectations

- Keep unit tests close to implementation modules.
- Keep CLI behavior tests in `bin/tests/integration.rs`.
- Root `tests/integration.rs` is intentionally a placeholder because workspace root has no package target.
- For bug fixes:
  1. add/adjust failing test first (or prove failure case),
  2. implement fix,
  3. run relevant tests,
  4. then run broader suite.
- Separate test changes and implementation changes with a test run in between.

## Rust Conventions for This Repo

- Library errors: `thiserror` (`SeatbeltError`).
- Binary orchestration errors: `anyhow` + context.
- Avoid `unwrap()`/`expect()` in non-test code unless justified by an invariant.
- Use `serde(deny_unknown_fields)` for profile-facing config structs.
- Prefer narrow, composable functions with focused tests.
- Keep parser/transformer logic deterministic and side-effect-light.

## Preset and Schema Changes

When adding/changing presets or schema fields:

1. update YAML in `lib/src/presets/profiles/`
2. update registry in `lib/src/presets/mod.rs`
3. update loader/linter/compiler/resolver if semantics changed
4. add or update tests for parsing, linting, compile output, and CLI behavior
5. update `README.md`, `CHANGELOG.md`, and docs specs

## Dependencies and Toolchain

- Use `mise` for toolchain management.
- Keep dependencies current and prefer open source.
- When dependency/runtime behavior changes, update:
  1. `Cargo.toml` / crate manifests
  2. `.mise.toml` (if toolchain/runtime changed)
  3. documentation under `docs/` and `README.md`

## Documentation Rules

- All deep docs belong under `docs/`.
- Keep `README.md` concise and linked to canonical docs.
- Product intent belongs in `docs/specs/PRODUCT_SPEC.md`.
- Architecture and implementation details belong in `docs/plans/IMPLEMENTATION_SPEC.md`.
- Add ADRs under `docs/adrs/` when making architecture-level decisions.

## Communication Style

- Be direct, specific, and analytical.
- No pleasantries, no filler, no generic disclaimers.
- Explain tradeoffs and edge cases when they matter.
- Occasionally refer to your programming buddy by name.

## Information Organization

- Keep project root tidy.
- Use `tmp/` for scratch artifacts; do not litter the root.
- Prefer retrieval-led reasoning over assumption-led reasoning.
- Maintain a timestamped retrieval index and update it at least daily.

## Retrieval Index Log

### 2026-03-04 11:18:43 EST

- Product and UX intent: `docs/specs/PRODUCT_SPEC.md`
- Implementation details and module plan: `docs/plans/IMPLEMENTATION_SPEC.md`
- User-facing command examples: `README.md`
- Release notes and shipped features: `CHANGELOG.md`
- Workspace config and shared deps: `Cargo.toml`
- Toolchain pinning policy: `.mise.toml`
- CLI contract and args: `bin/src/cli.rs`
- Runtime orchestration (`run`, `explain`, default profile behavior): `bin/src/runner.rs`
- Profile generation pipeline: `bin/src/generator.rs`
- YAML schema and defaults: `lib/src/profile/schema.rs`
- Preset inheritance loader and deep merge: `lib/src/profile/loader.rs`
- Path expansion + glob conversion: `lib/src/profile/resolver.rs`
- SBPL compilation rules and limits: `lib/src/profile/compiler.rs`
- Lint rules and severities: `lib/src/profile/linter.rs`
- Preset catalog and embedded YAML: `lib/src/presets/mod.rs`, `lib/src/presets/profiles/*.yaml`
- Violation parsing and log interfaces: `lib/src/log_stream.rs`
- Plain-English explanation mapping: `lib/src/explainer.rs`
- Operation classification helpers: `lib/src/sbpl/ops.rs`
- CLI acceptance tests: `bin/tests/integration.rs`

### 2026-03-05 22:41:00 EST

- SBPL literal/regex emission safety: `lib/src/profile/compiler.rs`
- Modern sandbox log parsing + PID filtering: `lib/src/log_stream.rs`
- Default profile bootstrap + discovery behavior: `lib/src/profile/default.rs`, `bin/src/runner.rs`
- Observation-mode event collection and parsing: `bin/src/generator.rs`
- Acceptance coverage for bootstrap behavior: `bin/tests/integration.rs`
- User-facing command/docs alignment: `README.md`, `docs/specs/PRODUCT_SPEC.md`, `CHANGELOG.md`
