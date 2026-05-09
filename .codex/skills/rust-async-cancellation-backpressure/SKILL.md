---
name: rust-async-cancellation-backpressure
description: "Use for async runtime, cancellation, shutdown and backpressure design."
category: rust-runtime
---

# rust-async-cancellation-backpressure

## When to use
Async code uses Tokio/QUIC streams/channels/tasks.

## Purpose
Use for async runtime, cancellation, shutdown and backpressure design.

## Process
- Avoid holding locks across await.
- Ensure cancellation safety and supervised tasks.
- Bound channels and buffers.
- Implement graceful shutdown and backpressure signals.

## Expected output
- Async risk review or implementation plan.

## Guardrails
- No fire-and-forget critical WAL/commit work.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
