---
name: rust-deprecated-obsolete-removal
description: Remove obsolete deprecated transitional Rust code when deprecated obsolete legacy temporary shim or migration keywords appear.
license: MIT
---

# rust-deprecated-obsolete-removal

## When to use
Use when code is marked deprecated, transitional, legacy, temporary, shim, compatibility layer, TODO-remove, or superseded by a newer owner. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-deprecated-obsolete-removal` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Retire obsolete implementation cleanly while preserving supported contracts and migration evidence. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Find all call sites, feature flags, docs references, tests, and public exports for the obsolete path.
2. Check whether the deprecated item is part of a persisted, wire, or public procedure contract before removal.
3. Replace internal uses with the current owner and remove tests that only assert obsolete behavior after adding coverage for the replacement.
4. Run `cargo check --workspace --locked` and relevant tests; for large retirements, route `/agent` workers per crate for read-only impact analysis.
5. Summarize removed symbols and any compatibility intentionally retained.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/specifications/SPEC_PROCEDURE_CONTRACT_V0.md`
- `docs/adr/ADR-0004-CANONICAL_BINARY_FORMAT.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not break persisted or wire compatibility silently.
- Do not delete deprecation rationale from docs unless asked.
- Do not remove safety or recovery fallbacks without replacement validation.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
