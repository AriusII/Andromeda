---
name: implementation-direct-change-protocol
description: "Use for direct implementation requests with bounded code changes."
category: implementation
---

# implementation-direct-change-protocol

## When to use
User asks to implement a target feature or improvement.

## Purpose
Use for direct implementation requests with bounded code changes.

## Process
- Confirm exact target and success criteria.
- Read necessary docs and existing code first.
- Prefer smallest complete implementation slice.
- Add tests and validation matching risk.
- Write mission report.

## Expected output
- Implementation plan and acceptance checks.

## Guardrails
- No feature creep.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
