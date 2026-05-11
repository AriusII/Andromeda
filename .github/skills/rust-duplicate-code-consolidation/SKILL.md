---
name: rust-duplicate-code-consolidation
description: Consolidate duplicated Rust logic safely when duplicate copy paste shared helper or abstraction keywords appear.
license: MIT
---

# rust-duplicate-code-consolidation

## When to use
Use for repeated algorithms, duplicated validation, parallel test helpers, copy-pasted codec logic, or requests to DRY up Rust code. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-duplicate-code-consolidation` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Remove duplication only where a single owner can own the abstraction without weakening crate boundaries. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Identify duplicates with grep/structural search and compare behavior, error semantics, and feature gates.
2. Choose the narrowest owner; prefer private module functions over public shared crates.
3. Add or preserve characterization tests before consolidation when behavior is subtle.
4. Replace one duplicate group at a time and keep names domain-specific.
5. Validate all affected crates; use `/agent` read-only comparison workers for independent duplicate clusters.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/testing/TEST_STRATEGY.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not create generic utility crates.
- Do not merge similar-looking code with different durability/security semantics.
- Do not change wire/disk encodings while deduplicating.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
