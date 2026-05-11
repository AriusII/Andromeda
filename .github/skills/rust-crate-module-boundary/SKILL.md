---
name: rust-crate-module-boundary
description: Design and audit Rust crate and module boundaries when crate ownership common utils misc helpers god_engine or layering keywords appear.
license: MIT
---

# rust-crate-module-boundary

## When to use
Use for crate split/merge reviews, new crate proposals, module placement, dependency cycles, ownership questions, or keywords `common`, `utils`, `misc`, `helpers`, `god_engine`, `boundary`, `layering`, and `workspace`. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-crate-module-boundary` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Preserve Andromeda's boundary-heavy architecture: one primary responsibility per crate, narrow dependencies, and explicit domain ownership. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Read the owning crate README/AGENTS when present, then inspect manifests and module roots with code intelligence tools before grep.
2. Map the proposed symbol to catalog, SRPL, execution, transaction, storage, network, optimizer, operations, or contract/codec ownership.
3. Reject generic buckets; propose a named crate/module that matches the domain and public contract.
4. Use `/agent <name>` read-only workers for independent boundary audits across many crates, then consolidate findings.
5. Validate topology-sensitive changes with `cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture` plus `cargo check --workspace --locked`.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/REPOSITORY_ARCHITECTURE.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/README.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not add catch-all crates or modules.
- Do not move runtime behavior into contract-only crates.
- Do not hide dependency cycles behind re-exports.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
