---
name: rust-dead-code-detection
description: "Use to detect and remove unused Rust code paths."
category: rust-refactor
---

# rust-dead-code-detection

## When to use
Code may contain unused functions, types, modules, examples, or tests.

## Purpose
Use to detect and remove unused Rust code paths.

## Process
- Use compiler warnings, clippy, cargo check, cargo test, and search evidence.
- Distinguish truly dead code from feature-gated or test-only code.
- Propose removal only when no active reference or documented owner exists.

## Expected output
- Dead code list, evidence, removal plan, affected tests.

## Guardrails
- Do not remove public API without compatibility review.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
