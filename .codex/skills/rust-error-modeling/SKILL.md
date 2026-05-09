---
name: rust-error-modeling
description: "Use to design typed Rust errors and protocol error conversion."
category: rust-implementation
---

# rust-error-modeling

## When to use
Errors are stringly, vague, or mixed across domains.

## Purpose
Use to design typed Rust errors and protocol error conversion.

## Process
- Define domain error families.
- Use typed errors for libraries and preserve source chains.
- Use `anyhow` only in CLI/tooling/orchestration.
- Convert to RPC/protocol error families at boundaries without leaking secrets.

## Expected output
- Error enum design, conversion rules, tests.

## Guardrails
- No string-only critical errors.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
