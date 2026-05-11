---
name: rust-dead-code-detection
description: Detect and remove unused Rust code paths when dead code unused udeps unreachable or stale feature keywords appear.
license: MIT
---

# rust-dead-code-detection

## When to use
Use for unused functions, stale feature flags, unreachable branches, dead modules, leftover tests, `cargo udeps`, or warnings about `dead_code`. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-dead-code-detection` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Remove code that has no live owner while protecting intentionally dormant contracts, fixtures, and forward-compatible protocol variants. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Inventory candidates with compiler warnings, code search, symbol references, and `cargo build --message-format=json` for machine-readable diagnostics.
2. If installed, run `cargo +nightly udeps --workspace --all-targets`; report if unavailable instead of installing new global tools.
3. Classify each item as unused implementation, reserved public contract, test fixture, feature-gated path, or generated/protocol artifact.
4. Remove only bounded confirmed-dead slices and update manifests/features/tests that referenced them.
5. Validate with `cargo check --workspace --locked` and targeted tests; use `/agent` for parallel read-only reachability reports on independent crates.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/testing/CI_GATES.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not delete reserved wire/disk variants just because not currently constructed.
- Do not remove roadmap docs from this skill.
- Do not add broad `#[allow(dead_code)]` to hide issues.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
