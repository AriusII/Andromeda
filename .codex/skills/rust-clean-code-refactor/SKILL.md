---
name: rust-clean-code-refactor
description: "Use for professional Rust cleanup/refactor work without behavior drift."
category: rust-refactor
---

# rust-clean-code-refactor

## When to use
Refactor, cleanup, code review, or quality improvement request.

## Purpose
Use for professional Rust cleanup/refactor work without behavior drift.

## Process
- Preserve behavior unless explicitly changing semantics.
- Reduce public surface, duplication, dead code, and vague naming.
- Split by responsibility, not arbitrary size.
- Update tests and validation gates.

## Expected output
- Refactor plan or changes with behavior preservation evidence.

## Guardrails
- No speculative abstraction. No broad rewrite without plan.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
