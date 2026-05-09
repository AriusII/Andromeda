---
name: rust-duplicate-code-consolidation
description: "Use to consolidate duplicated Rust logic safely."
category: rust-refactor
---

# rust-duplicate-code-consolidation

## When to use
Similar code appears in multiple modules or crates.

## Purpose
Use to consolidate duplicated Rust logic safely.

## Process
- Identify duplicated behavior and invariants.
- Name the stable concept before extracting.
- Prefer local duplication over bad generic abstraction when concepts differ.
- Add regression tests around consolidated behavior.

## Expected output
- Duplication clusters, consolidation target, tests.

## Guardrails
- Do not create vague `common`, `utils`, `helper` modules.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
