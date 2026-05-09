---
name: rust-ci-quality-gates
description: "Use to define CI commands and release gates for Rust work."
category: rust-testing
---

# rust-ci-quality-gates

## When to use
Need to validate Rust code changes.

## Purpose
Use to define CI commands and release gates for Rust work.

## Process
- Use cargo fmt, clippy, nextest/test, doc tests, audit/deny/vet as appropriate.
- Add crash/recovery gates for C4/C5 storage work.
- Add benchmark gates for performance work.

## Expected output
- CI gate list with command and purpose.

## Guardrails
- Do not claim pass without running command.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
