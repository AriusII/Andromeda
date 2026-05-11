---
name: rust-toolchain-1950-policy
description: Enforce the Andromeda Rust 1.95.0 Edition 2024 resolver 3 MSRV policy when toolchain baseline or compatibility keywords appear.
license: MIT
---

# rust-toolchain-1950-policy

## When to use
Use `/skill rust-toolchain-1950-policy` for requests mentioning MSRV, Rust 1.95, toolchain, Edition 2024, resolver 3, rust-toolchain, Cargo.toml edition, dependency MSRV, or baseline drift. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-toolchain-1950-policy` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Keep every crate, command, dependency decision, and generated patch compatible with the repository baseline instead of opportunistically following newer local tooling. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Inspect `rust-toolchain.toml`, workspace `Cargo.toml`, and changed crate manifests before editing.
2. Use `cargo +1.95.0 fmt --all -- --check` when rustup has the toolchain; otherwise run the canonical command and report the installed-toolchain limitation.
3. Check each touched `Cargo.toml` for `edition = "2024"`, resolver 3 inheritance, and dependency features that raise MSRV.
4. When dispatching `/agent <name>` workers, include the MSRV constraint in the mission and require them to report commands run.
5. Validate with `cargo check --workspace --locked` or the narrow owning crate if the full workspace is disproportionate.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/adr/ADR-0001-RUST_BASELINE_AND_MSRV.md`
- `docs/project/RUST_BASELINE_1_95.md`
- `docs/testing/CI_GATES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not introduce APIs stabilized after Rust 1.95.0.
- Do not change the baseline documents from this skill.
- Do not use nightly-only features except as optional diagnostics explicitly reported as such.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
