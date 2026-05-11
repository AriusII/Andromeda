---
name: rust-public-api-minimization
description: Minimize Rust public APIs when pub surface exports stability or lib.rs re-export keywords appear.
license: MIT
---

# rust-public-api-minimization

## When to use
Use for `pub` audits, re-export cleanup, crate API stabilization, accidental public surface, `lib.rs` bloat, semver boundary questions, and protocol contract exposure. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-public-api-minimization` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Keep public APIs intentional, typed, and stable while internal implementation remains private and easy to refactor. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. List exported symbols with code intelligence, `cargo doc --no-deps` if useful, and direct reads of `lib.rs`/module roots.
2. Classify each `pub` as external contract, crate-internal coordination, test-only helper, or accidental exposure.
3. Prefer `pub(crate)`/private items, sealed traits, narrow constructors, and explicit re-exports from `lib.rs`.
4. When using `/agent <name>`, give workers read-only API inventory missions and merge results before edits.
5. Validate changed APIs by compiling the workspace and running owning integration tests that exercise downstream use.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not remove public API used by another crate without updating that owner.
- Do not expose native struct layout as a wire or disk contract.
- Do not turn `lib.rs` into an implementation file.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
