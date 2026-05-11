---
name: dependency-supply-chain
description: Govern Cargo dependencies and supply chain risk when dependency cargo audit pruning license or vendor keywords appear.
license: MIT
---

# dependency-supply-chain

## When to use
Use for adding/removing/upgrading crates, dependency pruning, feature minimization, `cargo audit`, license review, supply-chain risk, vendor policy, or lockfile changes. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill dependency-supply-chain` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Keep dependencies minimal, justified, locked, compatible with Rust 1.95.0, and aligned with Andromeda's mission-critical risk model. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Before adding a dependency, check if std or an existing workspace crate suffices; record why a new crate is necessary.
2. Inspect MSRV, license, maintenance, transitive dependencies, default features, unsafe usage, and critical-path placement.
3. Use `cargo metadata --locked`, `cargo tree -e features`, and available audit/deny tools; do not install new tools unless requested.
4. Prune unused dependencies with targeted manifest edits and validate lockfile behavior.
5. For broad audits, dispatch `/agent` read-only workers by dependency cluster and consolidate into an accept/remove/update table.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/adr/ADR-0010-SUPPLY_CHAIN_POLICY.md`
- `docs/adr/ADR-0001-RUST_BASELINE_AND_MSRV.md`
- `docs/testing/CI_GATES.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not introduce dependencies that raise MSRV beyond 1.95.0.
- Do not enable broad default features blindly.
- Do not add network, crypto, or serialization crates to critical paths without explicit justification.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
