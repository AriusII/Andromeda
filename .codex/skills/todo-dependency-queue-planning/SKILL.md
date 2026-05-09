---
name: todo-dependency-queue-planning
description: "Use to represent TODO/SUB-TODO/dependencies and decide worker execution order."
category: codex-orchestration
---

# todo-dependency-queue-planning

## When to use
A task has multiple steps or workers that may depend on each other.

## Purpose
Use to represent TODO/SUB-TODO/dependencies and decide worker execution order.

## Process
- Build a graph of TODOs and SUB-TODOs.
- Mark prerequisites, blocked tasks, file collisions, and validation dependencies.
- Group tasks into sequential and parallel-safe batches.
- Only schedule write workers whose prerequisites are complete.

## Expected output
- Dependency queue with groups, gates, blocked items, and worker count.

## Guardrails
- Do not schedule parallel writes to overlapping files.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
