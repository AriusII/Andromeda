---
name: roadmap-task-decomposition
description: "Use to decompose roadmap stages into executable work packets."
category: roadmap
---

# roadmap-task-decomposition

## When to use
A stage has multiple deliverables.

## Purpose
Use to decompose roadmap stages into executable work packets.

## Process
- Break down deliverables into TODO/SUB-TODO.
- Assign read-only analysis or write work type.
- Build dependency queues and validation gates.
- Identify worker groups and forbidden parallelism.

## Expected output
- Roadmap work packets and queue.

## Guardrails
- Do not over-parallelize shared files.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
