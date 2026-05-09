# andromeda-cli

## Purpose

`andromeda-cli` provides the operator and developer command-line surface for Andromeda workspace diagnostics, administration previews, and bounded local demonstrations.

The CLI is an operations tool. It must not become an application-facing runtime, an ad hoc SQL shell, or a bypass around typed, cataloged Procedure contracts.

## Scope

This crate owns command dispatch, argument parsing, and operator output for:

- Local vertical demonstrations: `vertical`, `vertical-v0`, `protocol-smoke`, and `recovery-inspect`.
- HADR administration: `hadr status`, `hadr node`, `hadr promote`, `hadr demote`, `hadr failover-prepare`, and `hadr quorum`.
- Durable audit inspection and retention operations: `audit inspect`, `audit compact`, and `audit verify`.
- Bounded benchmark orchestration through the administration surface: `benchmark workloads`, `benchmark contract`, `benchmark run`, `benchmark crud-scenarios`, and `benchmark crud`.
- Backup and restore administration previews and file-backed artifact checks: `backup`, `restore`, and their subcommands.
- Catalog inspection previews: `catalog list-procedures`, `catalog invalidate-cache`, `catalog show-contract`, and `catalog resolve-manifest`.

## Non-goals

- Do not expose application execution through CLI-only shortcuts.
- Do not introduce application-facing ad hoc SQL, dynamic table names, dynamic predicates, or shape-shifting result payloads.
- Do not bypass cataloged Procedure contracts, permission checks, WAL durability, recovery evidence, or audit requirements.
- Do not expose HA/DR, backup, restore, or catalog mutation behavior through the Application Surface.
- Do not treat diagnostic JSON as the runtime protocol.

## Prerequisites

- Run commands from the workspace root.
- Build with the workspace Rust toolchain and Rust 2024 Edition settings.
- Provide durable files explicitly for operations that require them, such as WAL inspection, audit journal verification, backup artifact verification, or restore preflight.
- Use the relevant operator credentials and environment controls outside this crate. The CLI documents and validates command shape; it does not grant authority by itself.

## Procedure

1. Use `cargo run -p andromeda-cli -- --help` to inspect the supported command surface.
2. Use preview commands first when working with backup, restore, catalog, HADR, or audit operations.
3. Use `--json` or `--diagnostic-json` only for operator tooling and test assertions.
4. Treat mutating administration commands as contracts that require the owning runtime, durable state, and audit evidence before production use.
5. Keep new commands under the appropriate administration module instead of adding generic command buckets.

## Validation

For CLI changes, prefer targeted tests close to the command family:

```powershell
cargo test -p andromeda-cli --test cli_admin_commands -- --nocapture
cargo test -p andromeda-cli --test audit_cli_commands -- --nocapture
cargo test -p andromeda-cli --test benchmark_cli_commands -- --nocapture
```

For workspace boundary changes, run:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Before accepting source changes, use the broader workspace gates listed in `crates/README.md`.

## Troubleshooting

- If a command reports `contract preview`, the durable runtime or artifact path was not supplied. Provide the documented `--journal`, `--artifact`, `--artifact-dir`, `--runtime`, or equivalent option for file-backed inspection where supported.
- If an audit command rejects a journal path, verify that the path names an existing file. The CLI must not create missing audit journals during verification.
- If benchmark output looks machine-readable, confirm whether `--diagnostic-json` was used. Diagnostic JSON remains advisory operator evidence, not a runtime wire protocol.
- If a catalog command shows preview data, do not treat it as a live catalog mutation unless the owning catalog runtime is wired and validated.

## References

- [Workspace crate rules](../README.md)
- [`src/cmd.rs`](src/cmd.rs)
- [`src/cmd_vertical.rs`](src/cmd_vertical.rs)
- [`src/audit/mod.rs`](src/audit/mod.rs)
- [`src/benchmark/mod.rs`](src/benchmark/mod.rs)
