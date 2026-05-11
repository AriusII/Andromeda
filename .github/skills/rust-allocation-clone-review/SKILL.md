---
name: rust-allocation-clone-review
description: Review allocations clones buffer growth and ownership pressure when clone allocation Vec String Arc or performance keywords appear.
license: MIT
---

# rust-allocation-clone-review

## When to use
Use for suspected excessive `clone`, allocation hotspots, `Vec` growth, `String` conversion, `Arc` churn, zero-copy questions, and ownership pressure in Rust code. Also use it when another agent, review comment, CI failure, or maintainer note uses those trigger words. Load it explicitly with `/skill rust-allocation-clone-review` before planning substantial work, and mention it in `/agent <name>` missions that need the same guardrails.

## Purpose
Reduce unnecessary memory traffic while keeping lifetimes understandable and correctness first. The skill is written for Copilot CLI workflow: prefer Rust code intelligence or rust-lsp for semantic questions, then `glob`, `grep`, and `view` for file discovery, `powershell` for cargo commands on Windows, `sql` for dependency queues, GitHub tools for PR/CI evidence, and the agent/task tool only when parallel analysis or bounded write missions add value.

## Process
1. Locate hot paths with benchmarks/profiles if available; otherwise classify code as critical path, setup path, or test helper.
2. Search for clones/allocations and inspect type ownership with code intelligence before changing signatures.
3. Prefer borrowing, `Cow`, preallocation, buffer reuse, and domain-owned arenas only when they simplify or measurably help.
4. Add regression tests or benchmarks for performance-sensitive changes and run targeted validation.
5. Use `/agent` read-only workers to inventory independent crates, but keep write slices small.

## Expected output
- A concise plan or patch summary tied to the trigger that loaded this skill.
- File paths, symbols, commands, and validation results that a reviewer can reproduce.
- If workers were used, a consolidated handoff that names each `/agent <name>` mission, its report path, and any conflicts resolved.
- Clear blockers when a command, tool, or reference is unavailable; do not turn missing evidence into a success claim.

## Reference docs
- `docs/architecture/HARDWARE_ARCHITECTURE.md`
- `docs/testing/CI_GATES.md`
- `docs/adr/ADR-0009-GPU_OUTSIDE_COMMIT_PATH.md`
- `docs/README.md`
- `docs/architecture/ENGINE_OVERVIEW.md`
- `docs/project/ANDROMEDA_DOCTRINE.md`

## Guardrails
- Do not trade clarity for speculative micro-optimization.
- Do not remove clones that protect isolation, ownership, or async lifetimes without proof.
- Do not alter serialization formats for allocation wins unless requested.
- Keep changes bounded to the requested owner and avoid unrelated cleanup.
- Preserve Andromeda's typed Procedure execution path, explicit codecs, durability, security admission, and deterministic testing expectations.

## Andromeda baseline
Rust 1.95.0 / Edition 2024 / resolver 3, QUIC + custom Protobuf RPC, no gRPC, no JSON native protocol, procedure-only surface, WAL-before-commit, GPU outside commit/recovery/security path.
