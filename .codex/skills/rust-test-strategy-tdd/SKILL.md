---
name: rust-test-strategy-tdd
description: "Use to design tests and TDD approach for Rust changes."
category: rust-testing
---

# rust-test-strategy-tdd

## When to use
Need tests for new implementation, refactor, or bug fix.

## Purpose
Use to design tests and TDD approach for Rust changes.

## Process
- Map tests to risk: unit, integration, property, fuzz, Miri, Loom, crash/recovery.
- Use red/green/refactor for local algorithms and parsers.
- Place tests near code or integration boundary appropriately.
- Ensure tests validate behavior, not implementation noise.

## Expected output
- Test plan, test placement, acceptance criteria.

## Guardrails
- Do not replace crash/recovery with only unit tests.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
